use axum::{body::Bytes, extract::{Path, Query, State}, http::{HeaderMap, StatusCode}, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use crate::{AppState, db::{CategoryInput, ItemInput}};

pub type ApiError = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: &str) -> ApiError { (status, Json(json!({"error": message}))) }
fn db_error(message: String) -> ApiError {
    tracing::warn!(%message, "Catalog operation failed");
    if message.contains("UNIQUE") { error(StatusCode::CONFLICT, "Name or key already exists") }
    else if message.contains("FOREIGN KEY") { error(StatusCode::BAD_REQUEST, "Category does not exist") }
    else if message.contains("Upload a valid") { error(StatusCode::BAD_REQUEST, &message) }
    else { error(StatusCode::INTERNAL_SERVER_ERROR, "Database operation failed") }
}
pub fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(&format!("Bearer {}", state.token)) {
        return Err(error(StatusCode::UNAUTHORIZED, "Enter the admin token"));
    }
    Ok(())
}
pub async fn session(State(state): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?;
    Ok(Json(json!({"ok":true})))
}
pub async fn categories(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.categories().await.map_err(db_error)?)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemQuery { category_id: Option<i64> }
pub async fn items(State(state): State<AppState>, Query(query): Query<ItemQuery>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(state.db.items(query.category_id).await.map_err(db_error)?)))
}
fn validate_category(input: &CategoryInput) -> Result<(), ApiError> {
    if input.name.trim().is_empty() || input.name.len() > 100 || input.description.len() > 1000 {
        return Err(error(StatusCode::BAD_REQUEST, "Name is required (max 100 characters), description max 1000"));
    } Ok(())
}
fn validate_item(input: &ItemInput) -> Result<(), ApiError> {
    if input.category_id <= 0 || input.name.trim().is_empty() || input.name.len() > 100 ||
        input.key.is_empty() || input.key.len() > 80 || !input.key.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') ||
        input.sound.len() > 500 || input.color.len() != 7 || !input.color.starts_with('#') ||
        !input.color[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error(StatusCode::BAD_REQUEST, "Invalid item: use a name, lowercase slug key, and hex color"));
    }
    if input.image.is_empty() || input.audio_url.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "Upload both an image and an audio file"));
    }
    Ok(())
}
fn saved(id: i64) -> Result<Json<Value>, ApiError> {
    if id == 0 { Err(error(StatusCode::NOT_FOUND, "Record not found")) } else { Ok(Json(json!({"id":id}))) }
}
pub async fn create_category(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<CategoryInput>) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?; validate_category(&input)?;
    saved(state.db.save_category(None, input).await.map_err(db_error)?)
}
pub async fn update_category(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap, Json(input): Json<CategoryInput>) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?; validate_category(&input)?;
    saved(state.db.save_category(Some(id), input).await.map_err(db_error)?)
}
pub async fn create_item(State(state): State<AppState>, headers: HeaderMap, Json(input): Json<ItemInput>) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?; validate_item(&input)?;
    saved(state.db.save_item(None, input).await.map_err(db_error)?)
}
pub async fn update_item(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap, Json(input): Json<ItemInput>) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?; validate_item(&input)?;
    saved(state.db.save_item(Some(id), input).await.map_err(db_error)?)
}
pub async fn delete_category(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?;
    saved(if state.db.delete(true, id).await.map_err(db_error)? > 0 { id } else { 0 })
}
pub async fn delete_item(State(state): State<AppState>, Path(id): Path<i64>, headers: HeaderMap) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?;
    saved(if state.db.delete(false, id).await.map_err(db_error)? > 0 { id } else { 0 })
}

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
pub async fn upload(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Result<Json<Value>, ApiError> {
    authorize(&state, &headers)?;
    let (kind, extension) = media_type(&body).ok_or_else(|| error(StatusCode::BAD_REQUEST, "Supported files: PNG, JPEG, WebP, MP3, WAV, OGG, M4A"))?;
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), extension);
    let path = state.uploads.join(&filename);
    let url = format!("/uploads/{filename}");
    let write_path = path.clone();
    tokio::task::spawn_blocking(move || std::fs::write(write_path, body)).await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Upload failed"))?
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Upload failed"))?;
    if let Err(e) = state.db.register_media(url.clone(), kind.into()).await {
        let _ = std::fs::remove_file(path);
        return Err(db_error(e));
    }
    Ok(Json(json!({"url":url,"kind":kind})))
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
}
