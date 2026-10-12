use axum::{body::Bytes, extract::{Path, State}, http::{HeaderMap, HeaderValue}, Json};
use base64::{engine::general_purpose::STANDARD, Engine};
use rmcp::{
    model::{CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool},
    service::RequestContext, ErrorData, RoleServer, ServerHandler,
};
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
        ("create_category", "Create a category.", category.clone(), vec!["name"], false, false),
        ("update_category", "Replace category name and description. Preserve fields using list_categories.", category, vec!["id","name"], false, false),
        ("delete_category", "Delete a category and ALL its items. Obtain user confirmation first.", json!({"id":id,"confirm":{"const":true,"type":"boolean"}}), vec!["id","confirm"], false, true),
        ("list_items", "List items, optionally filtered by categoryId.", json!({"categoryId":id}), vec![], true, false),
        ("create_item", "Create an item. Upload its image and recorded sound first.", item.clone(), vec!["categoryId","key","name","image","audioUrl"], false, false),
        ("update_item", "Replace an item. Use list_items and preserve unchanged fields.", item, vec!["id","categoryId","key","name","image","audioUrl"], false, false),
        ("delete_item", "Delete an item. Obtain user confirmation first.", json!({"id":id,"confirm":{"const":true,"type":"boolean"}}), vec!["id","confirm"], false, true),
        ("upload_media", "Upload base64 file contents: PNG, JPEG, WebP, MP3, WAV, OGG, M4A. Maximum decoded size 20 MB. Returns URL and kind; no local path or URL fetching.", json!({"dataBase64":{"type":"string","minLength":1,"maxLength":27962028}}), vec!["dataBase64"], false, false),
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

fn record_id(args: &Value) -> Result<i64, String> {
    args["id"].as_i64().filter(|id| *id > 0).ok_or_else(|| "A positive id is required".into())
}
fn confirmed(args: &Value) -> Result<(), String> {
    if args["confirm"] == true { Ok(()) } else { Err("Deletion requires confirm=true after user approval".into()) }
}
fn result(value: Result<Json<Value>, admin::ApiError>) -> Result<Value, String> {
    value.map(|Json(value)| value).map_err(|(_, Json(value))| value["error"].as_str().unwrap_or("Operation failed").to_string())
}

impl CatalogMcp {
    async fn execute(&self, name: &str, args: Value) -> Result<Value, String> {
        let state = State(self.0.clone());
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(&format!("Bearer {}", self.0.token)).map_err(|_| "Invalid admin token")?);
        match name {
            "list_categories" => result(admin::categories(state).await),
            "list_items" => {
                let id = match args.get("categoryId") {
                    None => None,
                    Some(value) => Some(value.as_i64().filter(|id| *id > 0).ok_or("Invalid categoryId")?),
                };
                self.0.db.items(id).await.map(|items| json!(items))
            }
            "create_category" => result(admin::create_category(state, headers, Json(serde_json::from_value(args).map_err(|e| e.to_string())?)).await),
            "update_category" => result(admin::update_category(state, Path(record_id(&args)?), headers, Json(serde_json::from_value(args).map_err(|e| e.to_string())?)).await),
            "delete_category" => { confirmed(&args)?; result(admin::delete_category(state, Path(record_id(&args)?), headers).await) }
            "create_item" => result(admin::create_item(state, headers, Json(serde_json::from_value(args).map_err(|e| e.to_string())?)).await),
            "update_item" => result(admin::update_item(state, Path(record_id(&args)?), headers, Json(serde_json::from_value(args).map_err(|e| e.to_string())?)).await),
            "delete_item" => { confirmed(&args)?; result(admin::delete_item(state, Path(record_id(&args)?), headers).await) }
            "upload_media" => {
                let encoded = args["dataBase64"].as_str().ok_or("dataBase64 is required")?;
                if encoded.len() > 27962028 { return Err("Maximum upload size is 20 MB".into()); }
                let decoded = STANDARD.decode(encoded).map_err(|_| "Invalid base64 data")?;
                if decoded.len() > 20 * 1024 * 1024 { return Err("Maximum upload size is 20 MB".into()); }
                result(admin::upload(state, headers, Bytes::from(decoded)).await)
            }
            _ => Err("Unknown tool".into()),
        }
    }
}

impl ServerHandler for CatalogMcp {
    fn get_info(&self) -> ServerConfig {
        let mut config = ServerConfig::default();
        config.capabilities = ServerCapabilities::builder().enable_tools().build();
        config.instructions = Some("Manage Peekaboo Pals categories, items, images and recorded sounds. Ask before destructive operations. Upload file contents with upload_media, then use its URLs in item tools.".into());
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
        let server = CatalogMcp(AppState { db: Database::in_memory(), uploads: "/nonexistent".into(), token: "test-token-12345678".into() });
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
