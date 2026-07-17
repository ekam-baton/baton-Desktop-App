use std::sync::Arc;
use axum::{
    extract::{State, Query},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use axum::response::sse::{Event, Sse};
use std::convert::Infallible;

use crate::auth::validate_access_token;
use crate::mcp_models::{McpRequest, McpResponse};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct McpQuery {
    pub target: Option<String>,
}

/// Validate the target agent name: only lowercase alphanumeric and hyphens, max 64 chars.
fn validate_target(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn get_or_spawn_client(
    state: &Arc<AppState>,
    target: &str,
    client_id: &str,
) -> Option<Arc<crate::adapters::stdio::StdioMcpClient>> {
    let session_key = format!("{}:{}", target, client_id);
    let clients = &state.mcp_clients;
    
    if let Some(c) = clients.get(&session_key) {
        return Some(c.clone());
    }
    
    if let Some((cmd, args)) = state.config.mcp_agents.get(target) {
        let new_client = Arc::new(crate::adapters::stdio::StdioMcpClient::new(cmd.clone(), args.clone()));
        clients.insert(session_key, new_client.clone());
        Some(new_client)
    } else {
        None
    }
}

pub async fn initialize_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    let claims = match validate_access_token(&headers, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    if payload.jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))
        ).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    let client = match get_or_spawn_client(&state, &raw_target, &claims.sub) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(McpResponse::error(payload.id, -32601, "Agent not found".into(), None))).into_response(),
    };

    match client.send_request(&payload).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => {
            tracing::warn!("initialize agent error (fallback to local init): {}", e);
            // If the agent doesn't support initialize or fails, fallback to connector response
            let capabilities = json!({
                "tools": { "listChanged": false }
            });
            (StatusCode::OK, Json(McpResponse::success(payload.id, json!({
                "protocolVersion": "2024-11-05",
                "capabilities": capabilities,
                "serverInfo": {
                    "name": "baton-connector",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })))).into_response()
        }
    }
}

pub async fn notifications_initialized_handler(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if let Err(e) = validate_access_token(&headers, &state.config.jwt_secret) {
        return e.into_response();
    }
    StatusCode::OK.into_response()
}

pub async fn tools_list_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    let claims = match validate_access_token(&headers, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    if payload.jsonrpc != "2.0" {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))).into_response();
    }

    if !payload.is_method_allowed() {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::method_not_found(payload.id))).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    let client = match get_or_spawn_client(&state, &raw_target, &claims.sub) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, Json(McpResponse::error(payload.id, -32601, "Agent not found".into(), None))).into_response(),
    };

    match client.send_request(&payload).await {
        Ok(mut res) => {
            // Inject built-in tools into the agent's tool list
            if let Some(res_map) = res.as_object_mut() {
                if let Some(result_val) = res_map.get_mut("result") {
                    if let Some(result_map) = result_val.as_object_mut() {
                        if let Some(tools_arr) = result_map.get_mut("tools").and_then(|t| t.as_array_mut()) {
                    tools_arr.push(json!({
                        "name": "kb_add_document",
                        "description": "Adds a document to your isolated knowledge base for later RAG retrieval.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string", "description": "The title of the document." },
                                "content": { "type": "string", "description": "The plain text content to store and embed." }
                            },
                            "required": ["title", "content"]
                        }
                    }));
                    tools_arr.push(json!({
                        "name": "kb_search",
                        "description": "Searches your isolated knowledge base using semantic vector search.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "The search query to match against document embeddings." }
                            },
                            "required": ["query"]
                        }
                    }));
                }
            }
        }
    }
    (StatusCode::OK, Json(res)).into_response()
},
        Err(e) => {
            tracing::error!("tools/list agent error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(McpResponse::error(payload.id, -32000, "Agent error".into(), None))).into_response()
        }
    }
}

pub async fn tools_call_handler(
    headers: HeaderMap,
    Query(query): Query<McpQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<McpRequest>,
) -> impl IntoResponse {
    let claims = match validate_access_token(&headers, &state.config.jwt_secret) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };

    if payload.jsonrpc != "2.0" {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32600, "Invalid JSON-RPC version".into(), None))).into_response();
    }

    if !payload.is_method_allowed() {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::method_not_found(payload.id))).into_response();
    }

    let raw_target = query.target.unwrap_or_else(|| "openclaw".to_string()).to_lowercase();
    if !validate_target(&raw_target) {
        return (StatusCode::BAD_REQUEST, Json(McpResponse::error(payload.id, -32602, "Invalid target name".into(), None))).into_response();
    }

    // Intercept built-in KB tools
    if payload.method == "tools/call" {
        if let Some(params) = &payload.params {
            if let Ok(tool_params) = serde_json::from_value::<crate::mcp_models::McpToolCallParams>(params.clone()) {
                if tool_params.name == "kb_add_document" || tool_params.name == "kb_search" {
                    return handle_kb_tool(state, payload, tool_params, claims.sub).await;
                }
            }
        }
    }

    let client = match get_or_spawn_client(&state, &raw_target, &claims.sub) {
        Some(c) => c,
        None => {
            let err = McpResponse::error(payload.id.clone(), -32601, "Agent not found".into(), None);
            return (StatusCode::NOT_FOUND, Json(err)).into_response();
        }
    };

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<String, String>>(10);

    tokio::spawn(async move {
        match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            client.send_request(&payload)
        ).await {
            Ok(Ok(res)) => {
                match serde_json::to_string(&res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(e) => {
                        tracing::error!("tools/call serialization error: {}", e);
                        let _ = tx.send(Err("Serialization error".to_string())).await;
                    }
                }
            }
            Ok(Err(e)) => {
                // Do NOT leak internal agent error details to the caller.
                tracing::error!("tools/call agent error: {}", e);
                let err_res = McpResponse::error(payload.id.clone(), -32000, "Agent error".into(), None);
                match serde_json::to_string(&err_res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(_) => { let _ = tx.send(Err("Agent error".to_string())).await; }
                }
            }
            Err(_timeout) => {
                tracing::error!("tools/call agent timed out for method={}", payload.method);
                let err_res = McpResponse::error(payload.id.clone(), -32001, "Agent timeout".into(), None);
                match serde_json::to_string(&err_res) {
                    Ok(s) => { let _ = tx.send(Ok(s)).await; }
                    Err(_) => { let _ = tx.send(Err("Timeout".to_string())).await; }
                }
            }
        }
    });

    let stream = async_stream::stream! {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(text) => yield Ok::<Event, Infallible>(Event::default().data(text)),
                Err(e) => yield Ok::<Event, Infallible>(Event::default().event("error").data(e)),
            }
        }
        // Send the [DONE] marker expected by HttpSseMcpTransport.kt
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    };

    Sse::new(stream).into_response()
}

async fn handle_kb_tool(
    state: Arc<AppState>,
    payload: McpRequest,
    tool_params: crate::mcp_models::McpToolCallParams,
    client_id: String,
) -> axum::response::Response {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<String, String>>(10);
    
    // Use client_id as the namespace to guarantee strict isolation
    let namespace = client_id.clone();
    
    tokio::spawn(async move {
        match tool_params.name.as_str() {
            "kb_add_document" => {
                // Parse arguments: { "title": "...", "content": "..." }
                let title = tool_params.arguments.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
                let content = tool_params.arguments.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
                
                if content.is_empty() {
                    let err = McpResponse::error(payload.id.clone(), -32602, "Missing content".into(), None);
                    let _ = tx.send(Ok(serde_json::to_string(&err).unwrap())).await;
                    return;
                }

                // Chunk the document (e.g. 200 words per chunk)
                let chunks = crate::knowledge_base::KnowledgeBase::chunk_text(&content, 200);
                
                let mut added = 0;
                for (i, chunk_content) in chunks.iter().enumerate() {
                    let chunk_title = format!("{} (Part {})", title, i + 1);
                    if let Ok(_) = state.knowledge_base.add_chunk(&namespace, &chunk_title, chunk_content).await {
                        added += 1;
                    }
                }
                
                let res = McpResponse::success(payload.id.clone(), json!({
                    "content": [
                        { "type": "text", "text": format!("Successfully indexed {} chunks into Knowledge Base.", added) }
                    ]
                }));
                let _ = tx.send(Ok(serde_json::to_string(&res).unwrap())).await;
            }
            "kb_search" => {
                // Parse arguments: { "query": "..." }
                let query = tool_params.arguments.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
                
                if query.is_empty() {
                    let err = McpResponse::error(payload.id.clone(), -32602, "Missing query".into(), None);
                    let _ = tx.send(Ok(serde_json::to_string(&err).unwrap())).await;
                    return;
                }

                // Search top 5 most relevant chunks in this isolated namespace
                match state.knowledge_base.search(&namespace, &query, 5).await {
                    Ok(results) => {
                        let mut result_text = String::new();
                        for (i, chunk) in results.iter().enumerate() {
                            result_text.push_str(&format!("[{}] {}\n{}\n\n", i+1, chunk.document_title, chunk.content));
                        }
                        if result_text.is_empty() {
                            result_text = "No relevant documents found in your knowledge base.".to_string();
                        }
                        
                        let res = McpResponse::success(payload.id.clone(), json!({
                            "content": [
                                { "type": "text", "text": result_text }
                            ]
                        }));
                        let _ = tx.send(Ok(serde_json::to_string(&res).unwrap())).await;
                    }
                    Err(e) => {
                        let err = McpResponse::error(payload.id.clone(), -32000, format!("Search failed: {}", e), None);
                        let _ = tx.send(Ok(serde_json::to_string(&err).unwrap())).await;
                    }
                }
            }
            _ => {
                let err = McpResponse::method_not_found(payload.id.clone());
                let _ = tx.send(Ok(serde_json::to_string(&err).unwrap())).await;
            }
        }
    });

    let stream = async_stream::stream! {
        while let Some(res) = rx.recv().await {
            match res {
                Ok(text) => yield Ok::<Event, Infallible>(Event::default().data(text)),
                Err(e) => yield Ok::<Event, Infallible>(Event::default().event("error").data(e)),
            }
        }
        yield Ok::<Event, Infallible>(Event::default().data("[DONE]"));
    };

    Sse::new(stream).into_response()
}
