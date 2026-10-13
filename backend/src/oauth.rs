//! OAuth 2.1 for remote MCP clients such as claude.ai custom connectors.
//!
//! "Simple auth": the parent approves a client once by entering the admin token
//! on the consent page. Implements the pieces MCP clients expect:
//! protected-resource metadata (RFC 9728), authorization-server metadata
//! (RFC 8414), dynamic client registration (RFC 7591), and the authorization
//! code grant with mandatory PKCE S256 plus rotating refresh tokens.
//!
//! Enabled only when `PUBLIC_URL` is set. Secrets are stored as SHA-256 hashes,
//! and every grant is tied to the admin token that approved it, so rotating
//! `ADMIN_TOKEN` revokes all OAuth sessions.

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::{db::DbError, AppState};

const CODE_TTL: i64 = 5 * 60;
const ACCESS_TTL: i64 = 60 * 60;
const REFRESH_TTL: i64 = 30 * 24 * 60 * 60;
const MAX_CLIENTS: i64 = 500;
const SCOPE: &str = "mcp";

/// Validates `PUBLIC_URL`: an HTTPS origin (HTTP only for localhost), no path.
pub fn public_url(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(url) = value.map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let url = url.trim_end_matches('/');
    let rest = url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://").filter(|host| is_loopback(host)))
        .ok_or("PUBLIC_URL must start with https:// (http:// is allowed only for localhost)")?;
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', ' ']) {
        return Err("PUBLIC_URL must be an origin such as https://games.example.com, without a path".into());
    }
    Ok(Some(url.to_string()))
}

/// The hostname of a validated public URL, for the MCP Host allowlist.
pub fn public_host(url: &str) -> String {
    let authority = url.split_once("://").map_or(url, |(_, rest)| rest);
    authority.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit()))
        .map_or(authority, |(host, _)| host).to_ascii_lowercase()
}

fn is_loopback(authority: &str) -> bool {
    let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map_or(host, |(h, _)| h);
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/.well-known/oauth-protected-resource", get(protected_resource))
        .route("/.well-known/oauth-protected-resource/mcp", get(protected_resource))
        .route("/.well-known/oauth-authorization-server", get(authorization_server))
        .route("/oauth/register", post(register))
        .route("/oauth/authorize", get(authorize_page).post(authorize_submit))
        .route("/oauth/token", post(token))
}

/// `WWW-Authenticate` value pointing MCP clients at the metadata, if OAuth is on.
pub fn challenge(state: &AppState) -> Option<HeaderValue> {
    let base = state.public_url.as_deref()?;
    HeaderValue::from_str(&format!(r#"Bearer resource_metadata="{base}/.well-known/oauth-protected-resource/mcp""#)).ok()
}

/// Whether `token` is a live OAuth access token approved under the current admin token.
pub async fn access_token_is_valid(state: &AppState, token: &str) -> bool {
    if state.public_url.is_none() || token.is_empty() { return false; }
    let (hash, fingerprint) = (sha256_hex(token), fingerprint(state));
    state.db.run(move |connection| {
        let found: Option<(i64, String)> = connection.query_row(
            "SELECT expires_at, admin_fingerprint FROM oauth_tokens WHERE token_hash=?1 AND kind='access'",
            [hash], |row| Ok((row.get(0)?, row.get(1)?))).optional()?;
        Ok(found.is_some_and(|(expires, approved_by)| expires > now() && same_secret(&approved_by, &fingerprint)))
    }).await.unwrap_or(false)
}

// Helpers.

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

fn sha256_hex(value: &str) -> String {
    Sha256::digest(value.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// 256 bits of randomness from two v4 UUIDs (OS CSPRNG).
fn random_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn fingerprint(state: &AppState) -> String { sha256_hex(&state.token) }

/// Constant-time comparison for secrets of possibly different lengths.
pub fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn not_enabled() -> Response {
    (StatusCode::NOT_FOUND, "OAuth is not enabled").into_response()
}

/// Allowed redirect targets: HTTPS anywhere, or HTTP on loopback (desktop clients).
fn valid_redirect_uri(uri: &str) -> bool {
    uri.len() <= 2000 && !uri.contains(['#', ' ', '"', '<', '>'])
        && (uri.strip_prefix("https://").is_some_and(|rest| !rest.is_empty())
            || uri.strip_prefix("http://").is_some_and(is_loopback))
}

fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (status, no_store(), Json(json!({"error": error, "error_description": description}))).into_response()
}

fn no_store() -> [(header::HeaderName, &'static str); 2] {
    [(header::CACHE_CONTROL, "no-store"), (header::PRAGMA, "no-cache")]
}

fn server_error(error: DbError) -> Response {
    tracing::warn!(%error, "OAuth storage failed");
    oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Storage failed")
}

// Metadata.

async fn protected_resource(State(state): State<AppState>) -> Response {
    let Some(base) = state.public_url.as_deref() else { return not_enabled() };
    Json(json!({
        "resource": format!("{base}/mcp"),
        "authorization_servers": [base],
        "scopes_supported": [SCOPE],
        "bearer_methods_supported": ["header"],
        "resource_name": "Peekaboo Pals",
    })).into_response()
}

async fn authorization_server(State(state): State<AppState>) -> Response {
    let Some(base) = state.public_url.as_deref() else { return not_enabled() };
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/oauth/authorize"),
        "token_endpoint": format!("{base}/oauth/token"),
        "registration_endpoint": format!("{base}/oauth/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
        "scopes_supported": [SCOPE],
    })).into_response()
}

// Dynamic client registration.

#[derive(Deserialize)]
struct Registration {
    #[serde(default)]
    redirect_uris: Vec<String>,
    client_name: Option<String>,
    token_endpoint_auth_method: Option<String>,
}

async fn register(State(state): State<AppState>, Json(request): Json<Registration>) -> Response {
    if state.public_url.is_none() { return not_enabled(); }
    if request.redirect_uris.is_empty() || request.redirect_uris.len() > 10 || !request.redirect_uris.iter().all(|uri| valid_redirect_uri(uri)) {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_redirect_uri", "Use HTTPS redirect URIs, or HTTP on localhost");
    }
    let method = request.token_endpoint_auth_method.unwrap_or_else(|| "client_secret_basic".into());
    if !["none", "client_secret_post", "client_secret_basic"].contains(&method.as_str()) {
        return oauth_error(StatusCode::BAD_REQUEST, "invalid_client_metadata", "Unsupported token_endpoint_auth_method");
    }
    let name: String = request.client_name.unwrap_or_else(|| "MCP client".into()).chars().filter(|c| !c.is_control()).take(100).collect();
    let client_id = random_token();
    let secret = (method != "none").then(random_token);
    let (id, uris, stored_name, stored_method) = (client_id.clone(), serde_json::to_string(&request.redirect_uris).unwrap_or_default(), name.clone(), method.clone());
    let secret_hash = secret.as_deref().map(sha256_hex);
    let saved = state.db.run(move |connection| {
        let count: i64 = connection.query_row("SELECT count(*) FROM oauth_clients", [], |row| row.get(0))?;
        if count >= MAX_CLIENTS { return Ok(false); }
        connection.execute(
            "INSERT INTO oauth_clients(client_id, name, redirect_uris, auth_method, secret_hash, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![id, stored_name, uris, stored_method, secret_hash, now()])?;
        Ok(true)
    }).await;
    match saved {
        Ok(true) => {}
        Ok(false) => return oauth_error(StatusCode::BAD_REQUEST, "invalid_client_metadata", "Too many registered clients"),
        Err(error) => return server_error(error),
    }
    let mut body = json!({
        "client_id": client_id,
        "client_id_issued_at": now(),
        "client_name": name,
        "redirect_uris": request.redirect_uris,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "token_endpoint_auth_method": method,
        "scope": SCOPE,
    });
    if let Some(secret) = secret {
        body["client_secret"] = secret.into();
        body["client_secret_expires_at"] = 0.into();
    }
    (StatusCode::CREATED, no_store(), Json(body)).into_response()
}

struct Client { name: String, redirect_uris: Vec<String>, auth_method: String, secret_hash: Option<String> }

async fn load_client(state: &AppState, client_id: &str) -> Result<Option<Client>, DbError> {
    let id = client_id.to_string();
    state.db.run(move |connection| {
        Ok(connection.query_row(
            "SELECT name, redirect_uris, auth_method, secret_hash FROM oauth_clients WHERE client_id=?1", [id],
            |row| Ok(Client {
                name: row.get(0)?,
                redirect_uris: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                auth_method: row.get(2)?,
                secret_hash: row.get(3)?,
            })).optional()?)
    }).await
}

// Authorization (consent page).

#[derive(Deserialize, Default)]
struct AuthorizeParams {
    #[serde(default)] response_type: String,
    #[serde(default)] client_id: String,
    #[serde(default)] redirect_uri: String,
    #[serde(default)] code_challenge: String,
    #[serde(default)] code_challenge_method: String,
    #[serde(default)] state: String,
    #[serde(default)] scope: String,
    #[serde(default)] resource: String,
    /// Consent form only.
    #[serde(default)] admin_token: String,
    #[serde(default)] action: String,
}

/// Checks the request before showing or accepting consent. Errors here are
/// shown on the page, never redirected, because the redirect target is not yet trusted.
async fn check_request(state: &AppState, params: &AuthorizeParams) -> Result<Client, String> {
    let base = state.public_url.as_deref().ok_or("OAuth is not enabled")?;
    let client = load_client(state, &params.client_id).await.map_err(|e| e.to_string())?
        .ok_or("This app is not registered. Remove the connector and add it again.")?;
    if !client.redirect_uris.contains(&params.redirect_uri) { return Err("The redirect address does not match this app.".into()); }
    if params.response_type != "code" { return Err("Unsupported response_type; only \"code\" is supported.".into()); }
    if params.code_challenge_method != "S256" || params.code_challenge.len() < 43 || params.code_challenge.len() > 128 {
        return Err("A PKCE S256 code_challenge is required.".into());
    }
    if !params.resource.is_empty() && params.resource.trim_end_matches('/') != format!("{base}/mcp") && params.resource.trim_end_matches('/') != base {
        return Err("This authorization is for a different server.".into());
    }
    Ok(client)
}

fn redirect_with(redirect_uri: &str, pairs: &[(&str, &str)]) -> Response {
    let query = serde_urlencoded::to_string(pairs.iter().filter(|(_, v)| !v.is_empty()).collect::<Vec<_>>()).unwrap_or_default();
    let separator = if redirect_uri.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{redirect_uri}{separator}{query}")).into_response()
}

async fn authorize_page(State(state): State<AppState>, Query(params): Query<AuthorizeParams>) -> Response {
    match check_request(&state, &params).await {
        Ok(client) => consent_page(StatusCode::OK, &client, &params, None),
        Err(message) => error_page(&message),
    }
}

async fn authorize_submit(State(state): State<AppState>, Form(params): Form<AuthorizeParams>) -> Response {
    let client = match check_request(&state, &params).await {
        Ok(client) => client,
        Err(message) => return error_page(&message),
    };
    if params.action == "deny" {
        return redirect_with(&params.redirect_uri, &[("error", "access_denied"), ("state", &params.state)]);
    }
    if !same_secret(params.admin_token.trim(), &state.token) {
        // Slow down guessing; the admin token is the only secret on this page.
        tokio::time::sleep(Duration::from_millis(750)).await;
        return consent_page(StatusCode::UNAUTHORIZED, &client, &params, Some("That admin token is not right. Try again."));
    }
    let code = random_token();
    let (hash, client_id, redirect_uri, challenge, approved_by) =
        (sha256_hex(&code), params.client_id.clone(), params.redirect_uri.clone(), params.code_challenge.clone(), fingerprint(&state));
    let saved = state.db.run(move |connection| {
        connection.execute("DELETE FROM oauth_codes WHERE expires_at <= ?1", [now()])?;
        connection.execute(
            "INSERT INTO oauth_codes(code_hash, client_id, redirect_uri, challenge, admin_fingerprint, expires_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![hash, client_id, redirect_uri, challenge, approved_by, now() + CODE_TTL])?;
        Ok(())
    }).await;
    if let Err(error) = saved { return server_error(error); }
    tracing::info!(client = %client.name, "MCP client authorized");
    redirect_with(&params.redirect_uri, &[("code", &code), ("state", &params.state)])
}

// Token endpoint.

#[derive(Deserialize)]
struct TokenRequest {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
}

/// Client credentials from HTTP Basic auth or the form body.
fn client_credentials(headers: &HeaderMap, request: &TokenRequest) -> (Option<String>, Option<String>) {
    let basic = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic ")).and_then(|v| STANDARD.decode(v.trim()).ok())
        .and_then(|v| String::from_utf8(v).ok())
        .and_then(|v| v.split_once(':').map(|(id, secret)| (id.to_string(), secret.to_string())));
    match basic {
        Some((id, secret)) => (Some(id), Some(secret)),
        None => (request.client_id.clone(), request.client_secret.clone()),
    }
}

async fn token(State(state): State<AppState>, headers: HeaderMap, Form(request): Form<TokenRequest>) -> Response {
    if state.public_url.is_none() { return not_enabled(); }
    let (client_id, secret) = client_credentials(&headers, &request);
    let Some(client_id) = client_id else { return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", "client_id is required") };
    let client = match load_client(&state, &client_id).await {
        Ok(Some(client)) => client,
        Ok(None) => return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", "Unknown client"),
        Err(error) => return server_error(error),
    };
    if client.auth_method != "none" {
        let presented = secret.as_deref().map(sha256_hex).unwrap_or_default();
        if !client.secret_hash.as_deref().is_some_and(|stored| same_secret(stored, &presented)) {
            return oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", "Client authentication failed");
        }
    }
    match request.grant_type.as_str() {
        "authorization_code" => exchange_code(&state, client_id, &request).await,
        "refresh_token" => refresh(&state, client_id, &request).await,
        _ => oauth_error(StatusCode::BAD_REQUEST, "unsupported_grant_type", "Use authorization_code or refresh_token"),
    }
}

async fn exchange_code(state: &AppState, client_id: String, request: &TokenRequest) -> Response {
    let invalid = |why| oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", why);
    let (Some(code), Some(verifier)) = (request.code.as_deref(), request.code_verifier.as_deref()) else {
        return invalid("code and code_verifier are required");
    };
    let hash = sha256_hex(code);
    // Deleting while reading makes each code single-use.
    let row = state.db.run(move |connection| {
        Ok(connection.query_row(
            "DELETE FROM oauth_codes WHERE code_hash=?1 RETURNING client_id, redirect_uri, challenge, admin_fingerprint, expires_at",
            [hash], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, i64>(4)?)))
            .optional()?)
    }).await;
    let (owner, redirect_uri, challenge, approved_by, expires_at) = match row {
        Ok(Some(row)) => row,
        Ok(None) => return invalid("The authorization code is invalid or already used"),
        Err(error) => return server_error(error),
    };
    if owner != client_id || expires_at <= now() { return invalid("The authorization code is invalid or expired"); }
    if request.redirect_uri.as_deref().is_some_and(|uri| uri != redirect_uri) { return invalid("redirect_uri does not match"); }
    if !(43..=128).contains(&verifier.len()) || !same_secret(&URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())), &challenge) {
        return invalid("PKCE verification failed");
    }
    if !same_secret(&approved_by, &fingerprint(state)) { return invalid("The admin token changed; authorize again"); }
    issue_tokens(state, client_id).await
}

async fn refresh(state: &AppState, client_id: String, request: &TokenRequest) -> Response {
    let invalid = |why| oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", why);
    let Some(token) = request.refresh_token.as_deref() else { return invalid("refresh_token is required") };
    let hash = sha256_hex(token);
    // Refresh tokens rotate: the presented one is consumed.
    let row = state.db.run(move |connection| {
        Ok(connection.query_row(
            "DELETE FROM oauth_tokens WHERE token_hash=?1 AND kind='refresh' RETURNING client_id, admin_fingerprint, expires_at",
            [hash], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))).optional()?)
    }).await;
    match row {
        Ok(Some((owner, approved_by, expires_at)))
            if owner == client_id && expires_at > now() && same_secret(&approved_by, &fingerprint(state)) => issue_tokens(state, client_id).await,
        Ok(_) => invalid("The refresh token is invalid, expired, or revoked"),
        Err(error) => server_error(error),
    }
}

async fn issue_tokens(state: &AppState, client_id: String) -> Response {
    let (access, refresh) = (random_token(), random_token());
    let (access_hash, refresh_hash, approved_by) = (sha256_hex(&access), sha256_hex(&refresh), fingerprint(state));
    let saved = state.db.run(move |connection| {
        connection.execute("DELETE FROM oauth_tokens WHERE expires_at <= ?1", [now()])?;
        let insert = "INSERT INTO oauth_tokens(token_hash, kind, client_id, admin_fingerprint, expires_at) VALUES (?1,?2,?3,?4,?5)";
        connection.execute(insert, params![access_hash, "access", client_id, approved_by, now() + ACCESS_TTL])?;
        connection.execute(insert, params![refresh_hash, "refresh", client_id, approved_by, now() + REFRESH_TTL])?;
        Ok(())
    }).await;
    if let Err(error) = saved { return server_error(error); }
    (no_store(), Json(json!({
        "access_token": access,
        "token_type": "Bearer",
        "expires_in": ACCESS_TTL,
        "refresh_token": refresh,
        "scope": SCOPE,
    }))).into_response()
}

// Pages.

fn escape(value: &str) -> String {
    value.chars().fold(String::with_capacity(value.len()), |mut out, c| {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
        out
    })
}

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Peekaboo Pals</title>
<style>
:root {{ color-scheme: light; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 16px;
  background: #FFF4D6; color: #22304a; font: 600 16px/1.5 "Nunito", ui-rounded, system-ui, sans-serif; }}
main {{ width: min(440px, 100%); background: #fffaf0; border: 1px solid #e8d6a6; border-radius: 22px; padding: 28px; }}
h1 {{ font-size: 26px; line-height: 1.2; margin: 0 0 12px; }}
p {{ margin: 0 0 14px; color: #4a5670; }}
strong {{ color: #22304a; }}
label {{ display: flex; flex-direction: column; gap: 6px; font-weight: 800; font-size: 14px; margin: 18px 0; }}
input {{ min-height: 56px; padding: 12px; font: inherit; color: #22304a; background: #fffdf7; border: 1px solid #d8c399; border-radius: 12px; }}
.actions {{ display: flex; flex-wrap: wrap; gap: 12px; }}
button {{ min-height: 56px; padding: 10px 22px; border-radius: 16px; font: inherit; font-weight: 800; cursor: pointer; }}
.approve {{ border: 0; background: #E2570F; color: white; box-shadow: 0 4px 0 #a83d06; }}
.deny {{ border: 2px solid #e8d6a6; background: transparent; color: #4a5670; }}
button:active {{ transform: translateY(2px); }}
:focus-visible {{ outline: 3px solid #2F6FDB; outline-offset: 3px; }}
.error {{ background: #ffe3cb; border-radius: 12px; padding: 12px 14px; color: #22304a; font-weight: 800; }}
small {{ display: block; color: #626477; font-size: 13px; margin-top: 16px; }}
@media (prefers-reduced-motion: reduce) {{ button:active {{ transform: none; }} }}
</style></head><body><main>{body}</main></body></html>"#);
    (status, [
        (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'"),
        (header::X_FRAME_OPTIONS, "DENY"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CACHE_CONTROL, "no-store"),
    ], Html(html)).into_response()
}

fn consent_page(status: StatusCode, client: &Client, params: &AuthorizeParams, error: Option<&str>) -> Response {
    let destination = params.redirect_uri.split_once("://").map_or("", |(_, rest)| rest)
        .split(['/', '?']).next().unwrap_or_default();
    let hidden: String = [
        ("response_type", &params.response_type), ("client_id", &params.client_id),
        ("redirect_uri", &params.redirect_uri), ("code_challenge", &params.code_challenge),
        ("code_challenge_method", &params.code_challenge_method), ("state", &params.state),
        ("scope", &params.scope), ("resource", &params.resource),
    ].iter().map(|(name, value)| format!(r#"<input type="hidden" name="{name}" value="{}">"#, escape(value))).collect();
    let alert = error.map(|message| format!(r#"<p class="error" role="alert">{}</p>"#, escape(message))).unwrap_or_default();
    page(status, "Connect", &format!(r#"
<h1>Connect to Peekaboo Pals</h1>
<p><strong>{name}</strong> wants to manage your categories, items, pictures, and sounds.</p>
<p>After you approve, you'll return to <strong>{destination}</strong>.</p>
{alert}
<form method="post" action="/oauth/authorize">{hidden}
<label>Admin token
<input type="password" name="admin_token" required autocomplete="current-password" autofocus></label>
<div class="actions">
<button class="approve" type="submit" name="action" value="approve">Approve</button>
<button class="deny" type="submit" name="action" value="deny" formnovalidate>Cancel</button>
</div></form>
<small>Use the same token as parent management. Changing ADMIN_TOKEN disconnects every app.</small>"#,
        name = escape(&client.name), destination = escape(destination)))
}

fn error_page(message: &str) -> Response {
    page(StatusCode::BAD_REQUEST, "Can't connect", &format!(
        r#"<h1>Can't connect</h1><p class="error" role="alert">{}</p><p>Go back to the app and try connecting again.</p>"#,
        escape(message)))
}

#[cfg(test)]
pub(crate) fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", random_token(), "-verifier");
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{to_bytes, Body}, http::Request};
    use tower::ServiceExt;
    use crate::db::Database;
    use serde_json::Value;

    const BASE: &str = "https://games.example.com";
    const CALLBACK: &str = "https://claude.ai/api/mcp/auth_callback";
    const ADMIN: &str = "test-admin-token-123";

    fn state(db: Database, token: &str) -> AppState {
        AppState { db, uploads: "/nonexistent".into(), token: token.into(), speech: None, public_url: Some(BASE.into()) }
    }

    fn router(state: AppState) -> Router {
        crate::app("/nonexistent".into(), state, crate::mcp_allowed_hosts(Some("games.example.com")).unwrap())
    }

    async fn send(router: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let response = router.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let body = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        (status, headers, String::from_utf8_lossy(&body).into_owned())
    }

    fn form(uri: &str, body: &[(&str, &str)]) -> Request<Body> {
        Request::post(uri).header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(serde_urlencoded::to_string(body).unwrap())).unwrap()
    }

    fn mcp_initialize(bearer: &str) -> Request<Body> {
        Request::post("/mcp").header("host", "games.example.com").header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream").header("authorization", format!("Bearer {bearer}"))
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#))
            .unwrap()
    }

    async fn register(router: &Router, method: &str) -> Value {
        let body = json!({"redirect_uris": [CALLBACK], "client_name": "Claude <script>", "token_endpoint_auth_method": method});
        let (status, _, body) = send(router, Request::post("/oauth/register").header("content-type", "application/json")
            .body(Body::from(body.to_string())).unwrap()).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        serde_json::from_str(&body).unwrap()
    }

    /// Approves `client_id` on the consent page and returns the authorization code.
    async fn approve(router: &Router, client_id: &str, challenge: &str) -> String {
        let (status, headers, _) = send(router, form("/oauth/authorize", &[
            ("response_type", "code"), ("client_id", client_id), ("redirect_uri", CALLBACK),
            ("code_challenge", challenge), ("code_challenge_method", "S256"), ("state", "xyz"),
            ("resource", &format!("{BASE}/mcp")), ("admin_token", ADMIN), ("action", "approve"),
        ])).await;
        assert!(status.is_redirection(), "{status}");
        let location = headers[header::LOCATION].to_str().unwrap();
        assert!(location.starts_with(&format!("{CALLBACK}?code=")) && location.ends_with("&state=xyz"), "{location}");
        location.split("code=").nth(1).unwrap().split('&').next().unwrap().to_string()
    }

    async fn exchange(router: &Router, client_id: &str, code: &str, verifier: &str) -> (StatusCode, Value) {
        let (status, _, body) = send(router, form("/oauth/token", &[
            ("grant_type", "authorization_code"), ("client_id", client_id), ("code", code),
            ("redirect_uri", CALLBACK), ("code_verifier", verifier),
        ])).await;
        (status, serde_json::from_str(&body).unwrap())
    }

    #[tokio::test]
    async fn oauth_flow_grants_mcp_access_and_revokes_on_admin_rotation() {
        let db = Database::in_memory();
        let app = router(state(db.clone(), ADMIN));

        let (status, headers, _) = send(&app, mcp_initialize("nope")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(headers[header::WWW_AUTHENTICATE].to_str().unwrap().contains(&format!("{BASE}/.well-known/oauth-protected-resource/mcp")));
        let (_, _, body) = send(&app, Request::get("/.well-known/oauth-protected-resource/mcp").body(Body::empty()).unwrap()).await;
        let resource: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(resource["resource"], format!("{BASE}/mcp"));
        assert_eq!(resource["authorization_servers"][0], BASE);
        let (_, _, body) = send(&app, Request::get("/.well-known/oauth-authorization-server").body(Body::empty()).unwrap()).await;
        let metadata: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(metadata["code_challenge_methods_supported"], json!(["S256"]));
        assert_eq!(metadata["registration_endpoint"], format!("{BASE}/oauth/register"));

        let bad = json!({"redirect_uris": ["http://evil.example.com/cb"]}).to_string();
        let (status, _, _) = send(&app, Request::post("/oauth/register").header("content-type", "application/json").body(Body::from(bad)).unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "plain HTTP redirects are allowed only on localhost");
        let client = register(&app, "none").await;
        let client_id = client["client_id"].as_str().unwrap();
        assert!(client.get("client_secret").is_none());

        let (verifier, challenge) = pkce_pair();
        let query = serde_urlencoded::to_string([
            ("response_type", "code"), ("client_id", client_id), ("redirect_uri", CALLBACK),
            ("code_challenge", challenge.as_str()), ("code_challenge_method", "S256"), ("state", "xyz"),
        ]).unwrap();
        let (status, headers, page) = send(&app, Request::get(format!("/oauth/authorize?{query}")).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("Claude &lt;script&gt;") && page.contains("claude.ai"), "client name is escaped");
        assert!(headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("frame-ancestors 'none'"));
        let wrong_redirect = query.replace("claude.ai", "evil.example.com");
        let (status, _, _) = send(&app, Request::get(format!("/oauth/authorize?{wrong_redirect}")).body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "unregistered redirect URIs are never followed");
        let (status, _, page) = send(&app, form("/oauth/authorize", &[
            ("response_type", "code"), ("client_id", client_id), ("redirect_uri", CALLBACK),
            ("code_challenge", &challenge), ("code_challenge_method", "S256"), ("admin_token", "wrong"), ("action", "approve"),
        ])).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(page.contains("not right"));

        let code = approve(&app, client_id, &challenge).await;
        let (status, error) = exchange(&app, client_id, &code, &pkce_pair().0).await;
        assert_eq!((status, error["error"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_grant")), "wrong PKCE verifier");
        let code = approve(&app, client_id, &challenge).await;
        let (status, tokens) = exchange(&app, client_id, &code, &verifier).await;
        assert_eq!(status, StatusCode::OK, "{tokens}");
        assert_eq!(exchange(&app, client_id, &code, &verifier).await.0, StatusCode::BAD_REQUEST, "codes are single-use");

        let access = tokens["access_token"].as_str().unwrap();
        assert_eq!(send(&app, mcp_initialize(access)).await.0, StatusCode::OK);

        let refresh = tokens["refresh_token"].as_str().unwrap();
        let refresh_request = |token: &str| form("/oauth/token", &[("grant_type", "refresh_token"), ("client_id", client_id), ("refresh_token", token)]);
        let (status, _, body) = send(&app, refresh_request(refresh)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let renewed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(send(&app, refresh_request(refresh)).await.0, StatusCode::BAD_REQUEST, "refresh tokens rotate");
        assert_eq!(send(&app, mcp_initialize(renewed["access_token"].as_str().unwrap())).await.0, StatusCode::OK);

        let rotated = router(state(db, "rotated-admin-token-456"));
        assert_eq!(send(&rotated, mcp_initialize(access)).await.0, StatusCode::UNAUTHORIZED, "rotating ADMIN_TOKEN revokes grants");
        assert_eq!(send(&rotated, refresh_request(renewed["refresh_token"].as_str().unwrap())).await.0, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn confidential_clients_must_present_their_secret() {
        let app = router(state(Database::in_memory(), ADMIN));
        let client = register(&app, "client_secret_post").await;
        let (client_id, secret) = (client["client_id"].as_str().unwrap(), client["client_secret"].as_str().unwrap());
        let (verifier, challenge) = pkce_pair();
        let code = approve(&app, client_id, &challenge).await;
        assert_eq!(exchange(&app, client_id, &code, &verifier).await.0, StatusCode::UNAUTHORIZED, "missing secret");
        let code = approve(&app, client_id, &challenge).await;
        let basic = format!("Basic {}", STANDARD.encode(format!("{client_id}:{secret}")));
        let request = Request::post("/oauth/token").header("content-type", "application/x-www-form-urlencoded").header("authorization", basic)
            .body(Body::from(serde_urlencoded::to_string([("grant_type", "authorization_code"), ("code", code.as_str()), ("redirect_uri", CALLBACK), ("code_verifier", verifier.as_str())]).unwrap())).unwrap();
        assert_eq!(send(&app, request).await.0, StatusCode::OK);
    }

    #[test]
    fn public_url_must_be_an_https_origin() {
        assert_eq!(public_url(Some("https://games.example.com/")).unwrap().as_deref(), Some("https://games.example.com"));
        assert_eq!(public_url(Some("http://localhost:3001")).unwrap().as_deref(), Some("http://localhost:3001"));
        assert!(public_url(Some("http://games.example.com")).is_err());
        assert!(public_url(Some("https://games.example.com/app")).is_err());
        assert_eq!(public_url(Some(" ")).unwrap(), None);
        assert_eq!(public_host("https://Games.Example.com:8443"), "games.example.com");
        assert!(valid_redirect_uri(CALLBACK) && valid_redirect_uri("http://127.0.0.1:5555/callback"));
        assert!(!valid_redirect_uri("http://evil.example.com/cb") && !valid_redirect_uri("javascript:alert(1)"));
    }
}
