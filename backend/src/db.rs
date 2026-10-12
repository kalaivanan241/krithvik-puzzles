use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use rusqlite::{params, OptionalExtension};
use std::{path::Path, sync::{Arc, Mutex}, time::Duration};

#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>);

#[derive(Serialize)]
pub struct Animal {
    key: String,
    name: String,
    sound: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    id: i64,
    name: String,
    description: String,
    item_count: i64,
}

#[derive(Deserialize)]
pub struct CategoryInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemInput {
    pub category_id: i64,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub sound: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub audio_url: String,
    #[serde(default = "default_color")]
    pub color: String,
}
fn default_color() -> String { "#ffe7b8".into() }

#[derive(Serialize)]
pub struct Item {
    id: i64,
    #[serde(flatten)]
    data: ItemInput,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::initialize(Connection::open(path)?)
    }

    fn initialize(mut connection: Connection) -> rusqlite::Result<Self> {
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("animals.sql"))?;
        transaction.execute_batch(include_str!("catalog.sql"))?;
        let migrated: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = 'categories-v1')", [], |row| row.get(0))?;
        if !migrated {
            transaction.execute("INSERT INTO categories (name, description) VALUES ('Animals', 'Meet your animal friends')", [])?;
            let category_id = transaction.last_insert_rowid();
            transaction.execute(
                "INSERT INTO items (category_id, key, name, sound) SELECT ?1, key, name, sound FROM animals ORDER BY sort_order",
                [category_id])?;
            transaction.execute("INSERT INTO migrations VALUES ('categories-v1')", [])?;
        }
        transaction.commit()?;
        Ok(Self(Arc::new(Mutex::new(connection))))
    }

    pub async fn animals(&self) -> Result<Vec<Animal>, String> {
        self.run(|connection| {
            let mut statement = connection
                .prepare("SELECT key, name, sound FROM animals ORDER BY sort_order, key")?;
            let rows = statement.query_map([], |row| {
                Ok(Animal { key: row.get(0)?, name: row.get(1)?, sound: row.get(2)? })
            })?;
            rows.collect()
        }).await
    }

    pub async fn run<T: Send + 'static>(&self, operation: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static) -> Result<T, String> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let connection = db.0.lock().map_err(|e| e.to_string())?;
            operation(&connection).map_err(|e| e.to_string())
        }).await.map_err(|e| e.to_string())?
    }

    pub async fn categories(&self) -> Result<Vec<Category>, String> {
        self.run(|connection| {
            let mut query = connection.prepare("SELECT c.id, c.name, c.description, count(i.id) FROM categories c LEFT JOIN items i ON i.category_id = c.id GROUP BY c.id ORDER BY c.id")?;
            let rows = query.query_map([], |row| Ok(Category { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, item_count: row.get(3)? }))?;
            rows.collect()
        }).await
    }

    pub async fn items(&self, category_id: Option<i64>) -> Result<Vec<Item>, String> {
        self.run(move |connection| {
            let mut query = connection.prepare("SELECT id, category_id, key, name, sound, image, audio_url, color FROM items WHERE (?1 IS NULL OR category_id = ?1) ORDER BY id")?;
            let rows = query.query_map([category_id], |row| Ok(Item { id: row.get(0)?, data: ItemInput {
                category_id: row.get(1)?, key: row.get(2)?, name: row.get(3)?, sound: row.get(4)?,
                image: row.get(5)?, audio_url: row.get(6)?, color: row.get(7)?,
            } }))?;
            rows.collect()
        }).await
    }

    pub async fn save_category(&self, id: Option<i64>, input: CategoryInput) -> Result<i64, String> {
        self.run(move |connection| {
            if let Some(id) = id {
                let changed = connection.execute("UPDATE categories SET name=?1, description=?2 WHERE id=?3", params![input.name.trim(), input.description, id])?;
                Ok(if changed == 0 { 0 } else { id })
            } else {
                connection.execute("INSERT INTO categories(name, description) VALUES (?1, ?2)", params![input.name.trim(), input.description])?;
                Ok(connection.last_insert_rowid())
            }
        }).await
    }

    pub async fn save_item(&self, id: Option<i64>, input: ItemInput) -> Result<i64, String> {
        self.run(move |connection| {
            // File URLs must refer to uploaded, validated media of the right type.
            for (url, kind) in [(&input.image, "image"), (&input.audio_url, "audio")] {
                if !url.is_empty() {
                    let exists: Option<String> = connection.query_row("SELECT kind FROM media WHERE url=?1", [url], |row| row.get(0)).optional()?;
                    if exists.as_deref() != Some(kind) { return Err(rusqlite::Error::InvalidParameterName(format!("Upload a valid {kind} file first"))); }
                }
            }
            let values = params![input.category_id, input.key, input.name.trim(), input.sound, input.image, input.audio_url, input.color, id];
            if let Some(id) = id {
                let changed = connection.execute("UPDATE items SET category_id=?1,key=?2,name=?3,sound=?4,image=?5,audio_url=?6,color=?7 WHERE id=?8", values)?;
                Ok(if changed == 0 { 0 } else { id })
            } else {
                connection.execute("INSERT INTO items(category_id,key,name,sound,image,audio_url,color) VALUES (?1,?2,?3,?4,?5,?6,?7)", params![input.category_id, input.key, input.name.trim(), input.sound, input.image, input.audio_url, input.color])?;
                Ok(connection.last_insert_rowid())
            }
        }).await
    }

    pub async fn delete(&self, category: bool, id: i64) -> Result<usize, String> {
        self.run(move |connection| connection.execute(if category { "DELETE FROM categories WHERE id=?1" } else { "DELETE FROM items WHERE id=?1" }, [id])).await
    }

    pub async fn register_media(&self, url: String, kind: String) -> Result<(), String> {
        self.run(move |connection| { connection.execute("INSERT INTO media(url,kind) VALUES (?1,?2)", params![url,kind])?; Ok(()) }).await
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::initialize(Connection::open_in_memory().unwrap()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn seeds_are_idempotent_and_keep_edits() {
        let db = Database::in_memory();
        {
            let connection = db.0.lock().unwrap();
            connection.execute("UPDATE animals SET sound = ?1 WHERE key = ?2", ["Purr!", "cat"]).unwrap();
            connection.execute_batch(include_str!("animals.sql")).unwrap();
        }
        let animals = db.animals().await.unwrap();
        assert_eq!(animals.len(), 8);
        assert_eq!(animals[0].key, "cat");
        assert_eq!(animals[0].sound, "Purr!");
    }

    #[tokio::test]
    async fn data_survives_reopening_database() {
        let path = std::env::temp_dir().join(format!("peekaboo-db-test-{}.sqlite", std::process::id()));
        {
            let db = Database::open(&path).unwrap();
            db.0.lock().unwrap().execute("UPDATE animals SET name = 'Kitten' WHERE key = 'cat'", []).unwrap();
        }
        {
            let db = Database::open(&path).unwrap();
            assert_eq!(db.animals().await.unwrap()[0].name, "Kitten");
        }
        std::fs::remove_file(path).unwrap();
    }
}
