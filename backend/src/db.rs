use rusqlite::{ffi, params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, fmt, path::Path, sync::{Arc, Mutex}, time::Duration};
use crate::speech::{category_phrase, item_phrases};

pub const DEFAULT_COLOR: &str = "#ffe7b8";

#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>);

#[derive(Debug)]
pub enum DbError {
    /// A unique name, key, or media URL already exists.
    Conflict,
    /// The referenced category does not exist.
    MissingCategory,
    /// An item refers to media that was not uploaded with the expected kind.
    InvalidMedia(&'static str),
    Other(String),
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict => f.write_str("Name or key already exists"),
            Self::MissingCategory => f.write_str("Category does not exist"),
            Self::InvalidMedia(kind) => write!(f, "Upload a valid {kind} file first"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error().map(|e| e.extended_code) {
            Some(ffi::SQLITE_CONSTRAINT_UNIQUE | ffi::SQLITE_CONSTRAINT_PRIMARYKEY) => Self::Conflict,
            Some(ffi::SQLITE_CONSTRAINT_FOREIGNKEY) => Self::MissingCategory,
            _ => Self::Other(error.to_string()),
        }
    }
}

pub type DbResult<T> = Result<T, DbError>;

#[derive(Serialize)]
pub struct Animal {
    key: String,
    name: String,
    sound: String,
}

/// Generated voice clip URLs, keyed by slot (for example `prompt` or `name`).
pub type SpeechUrls = BTreeMap<&'static str, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    id: i64,
    name: String,
    description: String,
    item_count: i64,
    speech: SpeechUrls,
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
fn default_color() -> String { DEFAULT_COLOR.into() }

#[derive(Serialize)]
pub struct Item {
    id: i64,
    #[serde(flatten)]
    data: ItemInput,
    speech: SpeechUrls,
}

/// Looks up stored clip URLs for `(slot, phrase)` pairs, omitting missing ones.
fn speech_urls(connection: &Connection, phrases: Vec<(&'static str, String)>) -> rusqlite::Result<SpeechUrls> {
    let mut query = connection.prepare_cached("SELECT url FROM speech WHERE text=?1")?;
    let mut urls = SpeechUrls::new();
    for (slot, text) in phrases {
        if let Some(url) = query.query_row([text], |row| row.get(0)).optional()? { urls.insert(slot, url); }
    }
    Ok(urls)
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

    /// Runs a database operation on the blocking thread pool.
    pub(crate) async fn run<T: Send + 'static>(&self, operation: impl FnOnce(&Connection) -> DbResult<T> + Send + 'static) -> DbResult<T> {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let connection = db.0.lock().map_err(|e| DbError::Other(e.to_string()))?;
            operation(&connection)
        }).await.map_err(|e| DbError::Other(e.to_string()))?
    }

    pub async fn animals(&self) -> DbResult<Vec<Animal>> {
        self.run(|connection| {
            let mut statement = connection
                .prepare("SELECT key, name, sound FROM animals ORDER BY sort_order, key")?;
            let rows = statement.query_map([], |row| {
                Ok(Animal { key: row.get(0)?, name: row.get(1)?, sound: row.get(2)? })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        }).await
    }

    pub async fn categories(&self) -> DbResult<Vec<Category>> {
        self.run(|connection| {
            let mut query = connection.prepare("SELECT c.id, c.name, c.description, count(i.id) FROM categories c LEFT JOIN items i ON i.category_id = c.id GROUP BY c.id ORDER BY c.id")?;
            let rows = query.query_map([], |row| Ok(Category { id: row.get(0)?, name: row.get(1)?, description: row.get(2)?, item_count: row.get(3)?, speech: SpeechUrls::new() }))?;
            let mut categories = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            for category in &mut categories {
                category.speech = speech_urls(connection, vec![("name", category_phrase(&category.name))])?;
            }
            Ok(categories)
        }).await
    }

    pub async fn items(&self, category_id: Option<i64>) -> DbResult<Vec<Item>> {
        self.run(move |connection| {
            let mut query = connection.prepare("SELECT id, category_id, key, name, sound, image, audio_url, color FROM items WHERE (?1 IS NULL OR category_id = ?1) ORDER BY id")?;
            let rows = query.query_map([category_id], |row| Ok(Item { id: row.get(0)?, data: ItemInput {
                category_id: row.get(1)?, key: row.get(2)?, name: row.get(3)?, sound: row.get(4)?,
                image: row.get(5)?, audio_url: row.get(6)?, color: row.get(7)?,
            }, speech: SpeechUrls::new() }))?;
            let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            for item in &mut items {
                item.speech = speech_urls(connection, item_phrases(&item.data.name, &item.data.sound))?;
            }
            Ok(items)
        }).await
    }

    /// Inserts when `id` is `None`; otherwise updates. Returns the saved id, or `None` if no row matched.
    pub async fn save_category(&self, id: Option<i64>, input: CategoryInput) -> DbResult<Option<i64>> {
        self.run(move |connection| {
            let (name, description) = (input.name.trim(), &input.description);
            match id {
                Some(id) => {
                    let changed = connection.execute("UPDATE categories SET name=?1, description=?2 WHERE id=?3", params![name, description, id])?;
                    Ok((changed > 0).then_some(id))
                }
                None => {
                    connection.execute("INSERT INTO categories(name, description) VALUES (?1, ?2)", params![name, description])?;
                    Ok(Some(connection.last_insert_rowid()))
                }
            }
        }).await
    }

    /// Inserts when `id` is `None`; otherwise updates. Returns the saved id, or `None` if no row matched.
    pub async fn save_item(&self, id: Option<i64>, input: ItemInput) -> DbResult<Option<i64>> {
        self.run(move |connection| {
            // File URLs must refer to uploaded, validated media of the right type.
            for (url, kind) in [(&input.image, "image"), (&input.audio_url, "audio")] {
                if !url.is_empty() {
                    let stored: Option<String> = connection.query_row("SELECT kind FROM media WHERE url=?1", [url], |row| row.get(0)).optional()?;
                    if stored.as_deref() != Some(kind) { return Err(DbError::InvalidMedia(kind)); }
                }
            }
            let ItemInput { category_id, key, name, sound, image, audio_url, color } = &input;
            let name = name.trim();
            match id {
                Some(id) => {
                    let changed = connection.execute(
                        "UPDATE items SET category_id=?1,key=?2,name=?3,sound=?4,image=?5,audio_url=?6,color=?7 WHERE id=?8",
                        params![category_id, key, name, sound, image, audio_url, color, id])?;
                    Ok((changed > 0).then_some(id))
                }
                None => {
                    connection.execute(
                        "INSERT INTO items(category_id,key,name,sound,image,audio_url,color) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![category_id, key, name, sound, image, audio_url, color])?;
                    Ok(Some(connection.last_insert_rowid()))
                }
            }
        }).await
    }

    /// Deletes a category and, through the foreign key cascade, all of its items.
    pub async fn delete_category(&self, id: i64) -> DbResult<bool> {
        self.run(move |connection| Ok(connection.execute("DELETE FROM categories WHERE id=?1", [id])? > 0)).await
    }

    pub async fn delete_item(&self, id: i64) -> DbResult<bool> {
        self.run(move |connection| Ok(connection.execute("DELETE FROM items WHERE id=?1", [id])? > 0)).await
    }

    pub async fn register_media(&self, url: String, kind: &'static str) -> DbResult<()> {
        self.run(move |connection| { connection.execute("INSERT INTO media(url,kind) VALUES (?1,?2)", params![url, kind])?; Ok(()) }).await
    }

    /// Every phrase the catalog needs spoken, for backfilling missing clips.
    pub async fn catalog_phrases(&self) -> DbResult<Vec<String>> {
        self.run(|connection| {
            let mut phrases = BTreeSet::new();
            let mut categories = connection.prepare("SELECT name FROM categories")?;
            for name in categories.query_map([], |row| row.get::<_, String>(0))? { phrases.insert(category_phrase(&name?)); }
            let mut items = connection.prepare("SELECT name, sound FROM items")?;
            for row in items.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
                let (name, sound) = row?;
                phrases.extend(item_phrases(&name, &sound).into_iter().map(|(_, text)| text));
            }
            Ok(phrases.into_iter().collect())
        }).await
    }

    /// Returns the phrases from `texts` that have no stored clip yet, without duplicates.
    pub async fn missing_speech(&self, texts: Vec<String>) -> DbResult<Vec<String>> {
        self.run(move |connection| {
            let mut query = connection.prepare_cached("SELECT EXISTS(SELECT 1 FROM speech WHERE text=?1)")?;
            let mut missing = BTreeSet::new();
            for text in texts {
                if !query.query_row([&text], |row| row.get::<_, bool>(0))? { missing.insert(text); }
            }
            Ok(missing.into_iter().collect())
        }).await
    }

    pub async fn save_speech(&self, text: String, url: String) -> DbResult<()> {
        self.run(move |connection| { connection.execute("INSERT OR REPLACE INTO speech(text,url) VALUES (?1,?2)", params![text, url])?; Ok(()) }).await
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

    #[tokio::test]
    async fn constraint_errors_are_classified() {
        let db = Database::in_memory();
        let category = || CategoryInput { name: "Animals".into(), description: String::new() };
        assert!(matches!(db.save_category(None, category()).await, Err(DbError::Conflict)));
        let item = ItemInput {
            category_id: 999, key: "x".into(), name: "X".into(), sound: String::new(),
            image: String::new(), audio_url: String::new(), color: DEFAULT_COLOR.into(),
        };
        assert!(matches!(db.save_item(None, item).await, Err(DbError::MissingCategory)));
        db.register_media("/uploads/a.png".into(), "image").await.unwrap();
        assert!(matches!(db.register_media("/uploads/a.png".into(), "image").await, Err(DbError::Conflict)));
    }
}
