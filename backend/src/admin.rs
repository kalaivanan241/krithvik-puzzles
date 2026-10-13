//! Catalog operations shared by the HTTP API and the MCP server.
//!
//! The `pub async fn` operations validate input and touch the database; the
//! `*_handler` functions add Bearer-token authorization for HTTP callers.
//! MCP requests are authorized once by middleware before reaching these operations.

use axum::{body::Bytes, extract::{Path, Query, State}, http::{HeaderMap, StatusCode}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{AppState, db::{CategoryInput, DbError, ItemInput}, speech::{category_phrase, item_phrases}};

pub const MAX_UPLOAD_BYTES: usize = 20 * 1024 * 1024;

pub type ApiError = (StatusCode, Json<Value>);
pub type ApiResult = Result<Value, ApiError>;
type HandlerResult = Result<Json<Value>, ApiError>;

fn error(status: StatusCode, message: &str) -> ApiError { (status, Json(json!({"error": message}))) }

fn db_error(failure: DbError) -> ApiError {
    tracing::warn!(error = %failure, "Catalog operation failed");
    match failure {
        DbError::Conflict => error(StatusCode::CONFLICT, &failure.to_string()),
        DbError::MissingCategory | DbError::InvalidMedia(_) => error(StatusCode::BAD_REQUEST, &failure.to_string()),
        DbError::Other(_) => error(StatusCode::INTERNAL_SERVER_ERROR, "Database operation failed"),
    }
}

pub fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(&format!("Bearer {}", state.token)) {
        return Err(error(StatusCode::UNAUTHORIZED, "Enter the admin token"));
    }
    Ok(())
}

fn validate_category(input: &CategoryInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() || input.name.len() > 100 || input.description.len() > 1000 {
        return Err(error(StatusCode::BAD_REQUEST, "Name is required (max 100 characters), description max 1000"));
    }
    Ok(())
}

fn is_slug(key: &str) -> bool {
    !key.is_empty() && key.len() <= 80 && key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_hex_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate_item(input: &ItemInput) -> Result<(), ApiError> {
    if input.category_id <= 0 || input.name.trim().is_empty() || input.name.len() > 100 ||
        !is_slug(&input.key) || input.sound.len() > 500 || !is_hex_color(&input.color) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid item: use a name, lowercase slug key, and hex color"));
    }
    if input.image.is_empty() || input.audio_url.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Upload both an image and an audio file"));
    }
    Ok(())
}

fn saved(id: Option<i64>) -> ApiResult {
    id.map(|id| json!({"id": id})).ok_or_else(|| error(StatusCode::NOT_FOUND, "Record not found"))
}

fn deleted(id: i64, found: bool) -> ApiResult { saved(found.then_some(id)) }

// Accept only passive image formats and browser audio formats, never HTML/SVG.
fn media_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some(("image", "png")) }
    else if bytes.starts_with(b"\xff\xd8\xff") { Some(("image", "jpg")) }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { Some(("image", "webp")) }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") { Some(("audio", "wav")) }
    else if bytes.starts_with(b"ID3") || (bytes.len() > 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0) { Some(("audio", "mp3")) }
    else if bytes.starts_with(b"OggS") { Some(("audio", "ogg")) }
    else if bytes.get(4..8) == Some(b"ftyp") && [b"M4A ", b"M4B "].iter().any(|brand| bytes.get(8..12) == Some(*brand)) { Some(("audio", "m4a")) }
    else { None }
}

// Shared operations.

pub async fn list_categories(state: &AppState) -> ApiResult {
    Ok(json!(state.db.categories().await.map_err(db_error)?))
}

pub async fn list_items(state: &AppState, category_id: Option<i64>) -> ApiResult {
    Ok(json!(state.db.items(category_id).await.map_err(db_error)?))
}

/// Saves a category, then generates its spoken name. The response's `speech`
/// field is `ready`, `failed`, or `disabled`; a voice failure never undoes the save.
pub async fn save_category(state: &AppState, id: Option<i64>, input: CategoryInput) -> ApiResult {
    validate_category(&input)?;
    let phrases = vec![category_phrase(&input.name)];
    let mut response = saved(state.db.save_category(id, input).await.map_err(db_error)?)?;
    response["speech"] = generate_speech(state, phrases).await.into();
    Ok(response)
}

/// Saves an item, then generates its spoken prompts (see `save_category`).
pub async fn save_item(state: &AppState, id: Option<i64>, input: ItemInput) -> ApiResult {
    validate_item(&input)?;
    let phrases = item_phrases(&input.name, &input.sound).into_iter().map(|(_, text)| text).collect();
    let mut response = saved(state.db.save_item(id, input).await.map_err(db_error)?)?;
    response["speech"] = generate_speech(state, phrases).await.into();
    Ok(response)
}

pub async fn delete_category(state: &AppState, id: i64) -> ApiResult {
    deleted(id, state.db.delete_category(id).await.map_err(db_error)?)
}

pub async fn delete_item(state: &AppState, id: i64) -> ApiResult {
    deleted(id, state.db.delete_item(id).await.map_err(db_error)?)
}

/// Stores a validated upload under a generated filename and registers it as media.
pub async fn upload(state: &AppState, body: Bytes) -> ApiResult {
    let (kind, url) = store_media(state, body).await?;
    Ok(json!({"url": url, "kind": kind}))
}

/// Writes passive media to the uploads directory. Returns its kind and URL.
async fn store_media(state: &AppState, body: Bytes) -> Result<(&'static str, String), ApiError> {
    let (kind, extension) = media_type(&body).ok_or_else(|| error(StatusCode::BAD_REQUEST, "Supported files: PNG, JPEG, WebP, MP3, WAV, OGG, M4A"))?;
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let path = state.uploads.join(&filename);
    let url = format!("/uploads/{filename}");
    let write_path = path.clone();
    if !matches!(tokio::task::spawn_blocking(move || std::fs::write(write_path, body)).await, Ok(Ok(()))) {
        return Err(error(StatusCode::INTERNAL_SERVER_ERROR, "Upload failed"));
    }
    if let Err(e) = state.db.register_media(url.clone(), kind).await {
        let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(path)).await;
        return Err(db_error(e));
    }
    Ok((kind, url))
}

/// Generates and stores clips for phrases that have none yet.
/// Returns `ready`, `failed` (some clips are still missing), or `disabled`.
pub async fn generate_speech(state: &AppState, phrases: Vec<String>) -> &'static str {
    let Some(speech) = state.speech.clone() else { return "disabled" };
    let missing = match state.db.missing_speech(phrases).await {
        Ok(missing) => missing,
        Err(failure) => { tracing::warn!(error = %failure, "Could not check voice clips"); return "failed"; }
    };
    let mut tasks = tokio::task::JoinSet::new();
    for text in missing {
        let (state, speech) = (state.clone(), speech.clone());
        tasks.spawn(async move {
            let result = async {
                let audio = speech.synthesize(&text).await?;
                let (kind, url) = store_media(&state, audio).await.map_err(message)?;
                if kind != "audio" { return Err("ElevenLabs returned unsupported audio".to_string()); }
                state.db.save_speech(text.clone(), url).await.map_err(|e| e.to_string())
            }.await;
            if let Err(failure) = &result { tracing::warn!(error = %failure, phrase = %text, "Voice clip generation failed"); }
            result.is_ok()
        });
    }
    let mut ready = true;
    while let Some(result) = tasks.join_next().await { ready &= result.unwrap_or(false); }
    if ready { "ready" } else { "failed" }
}

/// Fills in clips for every catalog phrase, such as seeded animals or earlier failures.
pub async fn backfill_speech(state: AppState) {
    if state.speech.is_none() { return; }
    match state.db.catalog_phrases().await {
        Ok(phrases) => {
            let status = generate_speech(&state, phrases).await;
            tracing::info!(status, "Voice clip backfill finished");
        }
        Err(failure) => tracing::warn!(error = %failure, "Could not list catalog phrases"),
    }
}

/// The user-facing message from an API error.
pub fn message((_, Json(body)): ApiError) -> String {
    body["error"].as_str().unwrap_or("Operation failed").to_string()
}

// HTTP handlers.

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemQuery { category_id: Option<i64> }

pub async fn session_handler(State(state): State<AppState>, headers: HeaderMap) -> HandlerResult {
    authorize(&state, &headers)?;
    Ok(Json(json!({"ok": true})))
}
pub async fn categories_handler(State(state): State<AppState>) -> HandlerResult {
    list_categories(&state).await.map(Json)
}
pub async fn items_handler(State(state): State<AppState>, Query(query): Query<ItemQuery>) -> HandlerResult {
    list_items(&state, query.category_id).await.map(Json)
}
pub async fn create_category_handler(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<CategoryInput>) -> HandlerResult {
    authorize(&state, &headers)?;
    save_category(&state, None, input).await.map(Json)
}
pub async fn update_category_handler(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap, Json(input): Json<CategoryInput>) -> HandlerResult {
    authorize(&state, &headers)?;
    save_category(&state, Some(id), input).await.map(Json)
}
pub async fn delete_category_handler(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> HandlerResult {
    authorize(&state, &headers)?;
    delete_category(&state, id).await.map(Json)
}
pub async fn create_item_handler(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ItemInput>) -> HandlerResult {
    authorize(&state, &headers)?;
    save_item(&state, None, input).await.map(Json)
}
pub async fn update_item_handler(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap, Json(input): Json<ItemInput>) -> HandlerResult {
    authorize(&state, &headers)?;
    save_item(&state, Some(id), input).await.map(Json)
}
pub async fn delete_item_handler(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> HandlerResult {
    authorize(&state, &headers)?;
    delete_item(&state, id).await.map(Json)
}
pub async fn upload_handler(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> HandlerResult {
    authorize(&state, &headers)?;
    upload(&state, body).await.map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_passive_media_signatures() {
        assert_eq!(media_type(b"\x89PNG\r\n\x1a\n"), Some(("image", "png")));
        assert_eq!(media_type(b"RIFFxxxxWAVE"), Some(("audio", "wav")));
        assert_eq!(media_type(b"ID3test"), Some(("audio", "mp3")));
        assert_eq!(media_type(b"<svg onload='alert(1)'>"), None);
        assert_eq!(media_type(b"<html>"), None);
        assert_eq!(media_type(b""), None);
    }

    #[tokio::test]
    async fn saves_generate_voice_clips_once_and_tolerate_failures() {
        use std::sync::{atomic::{AtomicUsize, Ordering}, Arc};
        use crate::{db::Database, speech::Speech};
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let mock = axum::Router::new().route("/v1/text-to-speech/{voice}", axum::routing::post(
            move |headers: HeaderMap, Json(body): Json<Value>| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    assert_eq!(headers["xi-api-key"], "test-key");
                    if body["text"].as_str().unwrap().contains("broken") { return Err(StatusCode::INTERNAL_SERVER_ERROR); }
                    Ok(b"ID3 mock audio".to_vec())
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
        let uploads = std::env::temp_dir().join(format!("peekaboo-speech-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&uploads).unwrap();
        let state = AppState {
            db: Database::in_memory(), uploads: uploads.clone(), token: "test-admin-token-123".into(),
            speech: Speech::new("test-key", &format!("http://{address}"), "voice", "model"),
            public_url: None,
        };
        let category = |name: &str| CategoryInput { name: name.into(), description: String::new() };

        let created = save_category(&state, None, category("Vehicles")).await.unwrap();
        assert_eq!(created["speech"], "ready");
        let id = created["id"].as_i64();
        assert_eq!(save_category(&state, id, category("Vehicles")).await.unwrap()["speech"], "ready");
        assert_eq!(calls.load(Ordering::SeqCst), 1, "unchanged phrases are not regenerated");
        let categories = list_categories(&state).await.unwrap();
        let url = categories.as_array().unwrap().iter().find(|c| c["name"] == "Vehicles").unwrap()["speech"]["name"].as_str().unwrap().to_string();
        assert!(uploads.join(url.trim_start_matches("/uploads/")).exists());

        state.db.register_media("/uploads/car.png".into(), "image").await.unwrap();
        state.db.register_media("/uploads/car.wav".into(), "audio").await.unwrap();
        let item = ItemInput {
            category_id: id.unwrap(), key: "car".into(), name: "Car".into(), sound: "Vroom!".into(),
            image: "/uploads/car.png".into(), audio_url: "/uploads/car.wav".into(), color: "#ffe7b8".into(),
        };
        assert_eq!(save_item(&state, None, item).await.unwrap()["speech"], "ready");
        let items = list_items(&state, id).await.unwrap();
        let speech = &items[0]["speech"];
        for slot in ["prompt", "retry", "reveal", "sound"] { assert!(speech[slot].as_str().unwrap().starts_with("/uploads/"), "{slot}"); }

        let failed = save_category(&state, None, category("broken")).await.unwrap();
        assert_eq!(failed["speech"], "failed");
        assert!(failed["id"].as_i64().is_some(), "voice failures keep the saved record");

        let silent = AppState { speech: None, ..state };
        assert_eq!(save_category(&silent, None, category("Quiet")).await.unwrap()["speech"], "disabled");
        std::fs::remove_dir_all(uploads).unwrap();
    }

    #[test]
    fn validates_slugs_and_colors() {
        assert!(is_slug("fire-engine-2"));
        assert!(!is_slug("Fire Engine") && !is_slug("") && !is_slug(&"a".repeat(81)));
        assert!(is_hex_color("#A0b1C2"));
        assert!(!is_hex_color("#abc") && !is_hex_color("a0b1c2f") && !is_hex_color("#ggggggg"));
    }
}
