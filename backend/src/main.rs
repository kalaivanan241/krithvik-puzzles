mod db;
mod admin;
mod mcp;

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use db::{Animal, Database};
use serde::Serialize;
use std::{env, path::PathBuf};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    db: Database,
    uploads: PathBuf,
    token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GameConfig {
    say_name: bool,
    look_seconds: u8,
    rounds_per_level: u8,
    max_level: u8,
}

async fn animals(State(state): State<AppState>) -> Result<Json<Vec<Animal>>, (StatusCode, &'static str)> {
    state.db.animals().await.map(Json).map_err(|error| {
        tracing::error!(%error, "Failed to load animal catalog");
        (StatusCode::INTERNAL_SERVER_ERROR, "Could not load animal catalog")
    })
}

fn api() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { Json(serde_health()) }))
        .route("/config", get(|| async {
            Json(GameConfig { say_name: true, look_seconds: 3, rounds_per_level: 3, max_level: 6 })
        }))
        .route("/animals", get(animals))
        .route("/admin/session", get(admin::session))
        .route("/categories", get(admin::categories).post(admin::create_category))
        .route("/categories/{id}", axum::routing::put(admin::update_category).delete(admin::delete_category))
        .route("/items", get(admin::items).post(admin::create_item))
        .route("/items/{id}", axum::routing::put(admin::update_item).delete(admin::delete_item))
        .route("/uploads", axum::routing::post(admin::upload))
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024))
        .fallback(|| async { (StatusCode::NOT_FOUND, "Unknown API endpoint") })
}

fn serde_health() -> impl Serialize {
    #[derive(Serialize)]
    struct Health { status: &'static str, service: &'static str }
    Health { status: "ok", service: "peekaboo-pals" }
}

fn app(dist: PathBuf, state: AppState) -> Router {
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };
    let mcp_state = state.clone();
    let mut config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_max_request_body_bytes(28 * 1024 * 1024);
    config.legacy_session_mode = false;
    config.allowed_origins = vec!["http://localhost:*".into(), "http://127.0.0.1:*".into()];
    let service = StreamableHttpService::new(
        move || Ok(mcp::CatalogMcp(mcp_state.clone())),
        LocalSessionManager::default().into(),
        config,
    );
    let mcp_routes = Router::new()
        .route_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(state.clone(), mcp_auth));
    Router::new()
        .merge(mcp_routes)
        .nest("/api", api())
        .nest_service("/uploads", ServeDir::new(state.uploads.clone()))
        .fallback_service(ServeDir::new(dist))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn mcp_auth(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if let Err(error) = admin::authorize(&state, request.headers()) { return error.into_response(); }
    next.run(request).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,tower_http=debug".into()))
        .init();
    let address = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".into());
    // Resolve the frontend independently of the shell's working directory.
    let dist = env::var_os("FRONTEND_DIST").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist"));
    let database_path = env::var_os("DATABASE_PATH").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/peekaboo.sqlite"));
    if let Some(parent) = database_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let database = Database::open(&database_path)?;
    let uploads = env::var_os("UPLOAD_DIR").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/uploads"));
    std::fs::create_dir_all(&uploads)?;
    let token = match env::var("ADMIN_TOKEN") {
        Ok(token) if token.len() >= 16 => token,
        Ok(_) => return Err("ADMIN_TOKEN must be at least 16 characters".into()),
        Err(_) => {
            let path = database_path.with_extension("admin-token");
            if path.exists() { std::fs::read_to_string(&path)?.trim().to_string() }
            else {
                let token = uuid::Uuid::new_v4().to_string();
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)] {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                use std::io::Write;
                options.open(&path)?.write_all(token.as_bytes())?;
                token
            }
        }
    };
    if token.len() < 16 { return Err("Stored admin token is invalid".into()); }
    tracing::info!("Animal database: {}", database_path.display());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    tracing::info!("Peekaboo Pals listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app(dist, AppState { db: database, uploads, token }))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    {
        let mut terminate = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(signal) => signal,
            Err(_) => {
                ctrl_c.await;
                return;
            }
        };
        tokio::select! {
            _ = ctrl_c => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{to_bytes, Body}, http::Request};
    use tower::ServiceExt;

    fn test_app() -> Router {
        app(PathBuf::from("/nonexistent"), AppState { db: Database::in_memory(), uploads: PathBuf::from("/nonexistent"), token: "test-admin-token-123".into() })
    }

    async fn request(path: &str) -> (StatusCode, serde_json::Value) {
        let response = test_app()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn health_is_available() {
        let (status, body) = request("/api/health").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn defaults_match_game_rules() {
        let (status, config) = request("/api/config").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(config["maxLevel"], 6);
        assert_eq!(config["roundsPerLevel"], 3);
        assert_eq!(config["lookSeconds"], 3);
        assert_eq!(config["sayName"], true);
    }

    #[tokio::test]
    async fn catalog_has_eight_unique_pals() {
        let (_, animals) = request("/api/animals").await;
        let animals = animals.as_array().unwrap();
        assert_eq!(animals.len(), 8);
        let keys: std::collections::HashSet<_> = animals.iter().map(|a| a["key"].as_str().unwrap()).collect();
        assert_eq!(keys.len(), 8);
    }

    #[tokio::test]
    async fn unknown_api_route_is_not_frontend_html() {
        let response = test_app()
            .oneshot(Request::builder().uri("/api/missing").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    async fn mutate(app: Router, method: &str, path: &str, data: serde_json::Value, auth: bool) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder().method(method).uri(path).header("content-type", "application/json");
        if auth { request = request.header("authorization", "Bearer test-admin-token-123"); }
        let response = app.oneshot(request.body(Body::from(data.to_string())).unwrap()).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn category_crud_requires_auth_and_cascades() {
        let app = test_app();
        let data = serde_json::json!({"name":"Vehicles","description":"On the road"});
        assert_eq!(mutate(app.clone(), "POST", "/api/categories", data.clone(), false).await.0, StatusCode::UNAUTHORIZED);
        let (status, created) = mutate(app.clone(), "POST", "/api/categories", data.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        let id = created["id"].as_i64().unwrap();
        assert_eq!(mutate(app.clone(), "POST", "/api/categories", data, true).await.0, StatusCode::CONFLICT);
        assert_eq!(mutate(app.clone(), "PUT", &format!("/api/categories/{id}"), serde_json::json!({"name":"Transport"}), true).await.0, StatusCode::OK);
        assert_eq!(mutate(app.clone(), "DELETE", &format!("/api/categories/{id}"), serde_json::json!({}), true).await.0, StatusCode::OK);
        assert_eq!(mutate(app, "DELETE", &format!("/api/categories/{id}"), serde_json::json!({}), true).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn item_crud_and_media_validation() {
        let db = Database::in_memory();
        db.register_media("/uploads/test.png".into(), "image".into()).await.unwrap();
        db.register_media("/uploads/test.wav".into(), "audio".into()).await.unwrap();
        let app = app(PathBuf::from("/nonexistent"), AppState { db: db.clone(), uploads: PathBuf::from("/nonexistent"), token: "test-admin-token-123".into() });
        let mut data = serde_json::json!({"categoryId":1,"key":"horse","name":"Horse","image":"/uploads/test.png","audioUrl":"/uploads/test.wav"});
        let (status, created) = mutate(app.clone(), "POST", "/api/items", data.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        let id = created["id"].as_i64().unwrap();
        data["name"] = "Pony".into();
        assert_eq!(mutate(app.clone(), "PUT", &format!("/api/items/{id}"), data.clone(), true).await.0, StatusCode::OK);
        data["image"] = "/uploads/missing.png".into();
        assert_eq!(mutate(app.clone(), "PUT", &format!("/api/items/{id}"), data, true).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(mutate(app.clone(), "DELETE", &format!("/api/items/{id}"), serde_json::json!({}), true).await.0, StatusCode::OK);
        db.delete(true, 1).await.unwrap();
        assert!(db.items(Some(1)).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mcp_http_handshake_discovery_and_auth() {
        let app = test_app();
        let initialize = serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}
        });
        let make_request = |body: serde_json::Value, auth: bool, origin: Option<&str>| {
            let mut builder = Request::builder().method("POST").uri("/mcp")
                .header("host", "127.0.0.1:3001").header("content-type", "application/json")
                .header("accept", "application/json, text/event-stream");
            if auth { builder = builder.header("authorization", "Bearer test-admin-token-123"); }
            if let Some(origin) = origin { builder = builder.header("origin", origin); }
            builder.body(Body::from(body.to_string())).unwrap()
        };
        assert_eq!(app.clone().oneshot(make_request(initialize.clone(), false, None)).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        assert_eq!(app.clone().oneshot(make_request(initialize.clone(), true, Some("https://untrusted.example"))).await.unwrap().status(), StatusCode::FORBIDDEN);
        let response = app.clone().oneshot(make_request(initialize, true, None)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 8192).await.unwrap()).unwrap();
        assert!(body["result"]["capabilities"]["tools"].is_object());
        let request = make_request(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}), true, None);
        let response = app.oneshot(request).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&to_bytes(response.into_body(), 20000).await.unwrap()).unwrap();
        assert_eq!(body["result"]["tools"].as_array().unwrap().len(), 9);
    }
}
