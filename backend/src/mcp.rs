use axum::body::Bytes;
use base64::{engine::general_purpose::STANDARD, Engine};
use rmcp::{
    model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool},
    service::RequestContext, ErrorData, RoleServer, ServerHandler,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use crate::{admin, AppState};

#[derive(Clone)]
pub struct CatalogMcp(pub AppState);

fn tools() -> Vec<Tool> {
    let id = json!({"type":"integer","minimum":1});
    let category = json!({"name":{"type":"string","minLength":1,"maxLength":100},"description":{"type":"string","maxLength":1000}});
    let item = json!({
        "categoryId":id,"key":{"type":"string","pattern":"^[a-z0-9-]+$","maxLength":80},
        "name":{"type":"string","minLength":1,"maxLength":100},"sound":{"type":"string","maxLength":500},
        "image":{"type":"string","description":"URL returned by upload_media"},
        "audioUrl":{"type":"string","description":"URL returned by upload_media"},
        "color":{"type":"string","pattern":"^#[0-9a-fA-F]{6}$"}
    });
    let definitions = [
        ("list_categories", "List categories and item counts.", json!({}), vec![], true, false),
        ("create_category", "Create a category. Its spoken name is generated automatically.", category.clone(), vec!["name"], false, false),
        ("update_category", "Replace category name and description. Preserve fields using list_categories.", category, vec!["id","name"], false, false),
        ("delete_category", "Delete a category and ALL its items. Obtain user confirmation first.", json!({"id":id,"confirm":{"const":true,"type":"boolean"}}), vec!["id","confirm"], false, true),
        ("list_items", "List items, optionally filtered by categoryId.", json!({"categoryId":id}), vec![], true, false),
        ("create_item", "Create an item. Upload its image and recorded sound first. Spoken prompts are generated automatically.", item.clone(), vec!["categoryId","key","name","image","audioUrl"], false, false),
        ("update_item", "Replace an item. Use list_items and preserve unchanged fields.", item, vec!["id","categoryId","key","name","image","audioUrl"], false, false),
        ("delete_item", "Delete an item. Obtain user confirmation first.", json!({"id":id,"confirm":{"const":true,"type":"boolean"}}), vec!["id","confirm"], false, true),
        ("upload_media", "Upload base64 file contents: PNG, JPEG, WebP, MP3, WAV, OGG, M4A. Maximum decoded size 20 MB. Returns URL and kind; no local path or URL fetching.", json!({"dataBase64":{"type":"string","minLength":1,"maxLength":MAX_BASE64_LEN}}), vec!["dataBase64"], false, false),
    ];
    definitions.into_iter().map(|(name, description, mut properties, required, read_only, destructive)| {
        if name.starts_with("update_") { properties["id"] = id.clone(); }
        serde_json::from_value(json!({
            "name":name,"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read_only,"destructiveHint":destructive,"openWorldHint":false}
        })).expect("valid tool schema")
    }).collect()
}

/// Base64 length that decodes to at most `admin::MAX_UPLOAD_BYTES`.
const MAX_BASE64_LEN: usize = admin::MAX_UPLOAD_BYTES.div_ceil(3) * 4;

fn record_id(args: &Value) -> Result<i64, String> {
    args["id"].as_i64().filter(|id| *id > 0).ok_or_else(|| "A positive id is required".into())
}
fn confirmed(args: &Value) -> Result<(), String> {
    if args["confirm"] == true { Ok(()) } else { Err("Deletion requires confirm=true after user approval".into()) }
}
fn input<T: DeserializeOwned>(args: Value) -> Result<T, String> {
    serde_json::from_value(args).map_err(|e| e.to_string())
}
fn decode_upload(args: &Value) -> Result<Vec<u8>, String> {
    let encoded = args["dataBase64"].as_str().ok_or("dataBase64 is required")?;
    let too_large = || "Maximum upload size is 20 MB".to_string();
    if encoded.len() > MAX_BASE64_LEN { return Err(too_large()); }
    let decoded = STANDARD.decode(encoded).map_err(|_| "Invalid base64 data")?;
    if decoded.len() > admin::MAX_UPLOAD_BYTES { return Err(too_large()); }
    Ok(decoded)
}

impl CatalogMcp {
    /// Runs a tool. Requests are already authorized by the `/mcp` middleware.
    async fn execute(&self, name: &str, args: Value) -> Result<Value, String> {
        let state = &self.0;
        let outcome = match name {
            "list_categories" => admin::list_categories(state).await,
            "list_items" => {
                let category_id = match args.get("categoryId") {
                    None => None,
                    Some(value) => Some(value.as_i64().filter(|id| *id > 0).ok_or("Invalid categoryId")?),
                };
                admin::list_items(state, category_id).await
            }
            "create_category" => admin::save_category(state, None, input(args)?).await,
            "update_category" => admin::save_category(state, Some(record_id(&args)?), input(args)?).await,
            "delete_category" => { confirmed(&args)?; admin::delete_category(state, record_id(&args)?).await }
            "create_item" => admin::save_item(state, None, input(args)?).await,
            "update_item" => admin::save_item(state, Some(record_id(&args)?), input(args)?).await,
            "delete_item" => { confirmed(&args)?; admin::delete_item(state, record_id(&args)?).await }
            "upload_media" => admin::upload(state, Bytes::from(decode_upload(&args)?)).await,
            _ => return Err("Unknown tool".into()),
        };
        outcome.map_err(admin::message)
    }
}

impl ServerHandler for CatalogMcp {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder().enable_tools().build();
        config.instructions = Some("Manage Peekaboo Pals categories, items, images and recorded sounds. Ask before destructive operations. Upload file contents with upload_media, then use its URLs in item tools. Spoken prompts for items and categories are generated automatically.".into());
        config
    }

    fn get_tool(&self, name: &str) -> Option<Tool> { tools().into_iter().find(|tool| tool.name == name) }

    async fn list_tools(&self, _: Option<PaginatedRequestParams>, _: RequestContext<RoleServer>) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult { tools: tools(), ..Default::default() })
    }

    async fn call_tool(&self, request: CallToolRequestParams, _: RequestContext<RoleServer>) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let outcome = self.execute(&request.name, args).await;
        let response = match outcome {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(error)]),
        };
        Ok(response.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn tools_share_database_and_require_delete_confirmation() {
        let server = CatalogMcp(AppState { db: Database::in_memory(), uploads: "/nonexistent".into(), token: "test-token-12345678".into(), speech: None, public_url: None });
        assert_eq!(tools().len(), 9);
        let created = server.execute("create_category", json!({"name":"Vehicles"})).await.unwrap();
        let id = created["id"].as_i64().unwrap();
        assert!(server.execute("delete_category", json!({"id":id})).await.is_err());
        server.execute("update_category", json!({"id":id,"name":"Transport"})).await.unwrap();
        assert!(server.execute("list_categories", json!({})).await.unwrap().as_array().unwrap().iter().any(|row| row["name"] == "Transport"));
        server.execute("delete_category", json!({"id":id,"confirm":true})).await.unwrap();
        assert!(server.execute("upload_media", json!({"dataBase64":"not-base64"})).await.is_err());
    }
}
