use axum::{
    routing::{get, post},
    Router, Json, http::{StatusCode, HeaderMap},
    extract::State,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, Feedback, LlmConfigRequest, LlmConfigResponse, HandoffSave, SearchResponse, GetMemoryNodesRequest, GetMemoryNodesResponse, ForgedSectionBatch};
use crate::store::MarkdownStore;
use crate::vault::watcher::WatchIgnoreList;
use serde_json::{json, Value};
use bytes::Bytes;

#[derive(Clone)]
pub struct ApiState {
    pub backend: Arc<dyn StorageBackend>,
    pub auth_token: String,
    pub store: Arc<MarkdownStore>,
    pub ignore_list: Arc<WatchIgnoreList>,
    pub dream_tx: Option<tokio::sync::mpsc::Sender<()>>,
    pub shutdown_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/v1/episodes", post(save_episode_handler))
        .route("/v1/episodes/batch", post(save_episodes_batch_handler))
        .route("/v1/search", post(search_handler))
        .route("/v1/feedback", post(feedback_handler))
        .route("/v1/config/llm", get(get_llm_config_handler).post(post_llm_config_handler))
        .route("/v1/handoffs", post(save_handoff_handler))
        .route("/v1/wisdom/harvest", post(harvest_handler))
        .route("/v1/dream", post(dream_handler))
        .route("/v1/nodes", post(get_memory_nodes_handler))
        .route("/v1/forge/save", post(save_forged_assets_handler))
        .route("/v1/mcp/tools", get(get_mcp_tools_handler))
        .route("/v1/mcp/call", post(call_mcp_tool_handler))
        .route("/v1/mcp/tools/call_batch", post(call_mcp_tool_batch_handler))
        .route("/v1/mcp/resources", get(resources_list_handler))
        .route("/v1/mcp/resources/read", post(resources_read_handler))
        .route("/v1/chat/completions", post(completions_proxy_handler))
        .route("/api/*path", post(ollama_proxy_handler).get(ollama_proxy_handler))
        .route("/v1/hooks/precompact", post(precompact_handler))
        .route("/v1/hooks/stop", post(stop_handler))
        .route("/v1/daemon/stop", post(stop_daemon_endpoint_handler))
        .with_state(state)
}

fn check_auth(headers: &HeaderMap, state: &ApiState) -> bool {
    let token_from_header = headers.get("X-Mythrax-Token")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
        
    let token_from_bearer = headers.get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| {
            if h.starts_with("Bearer ") {
                Some(h["Bearer ".len()..].to_string())
            } else {
                None
            }
        });

    if let Some(token_str) = token_from_header.or(token_from_bearer) {
        crate::auth::verify_token_constant_time(&token_str, &state.auth_token)
    } else {
        false
    }
}

async fn save_episode_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<EpisodeSave>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match crate::vault::watcher::save_episode_bidirectional(&payload, state.backend.as_ref(), &state.store, &state.ignore_list).await {
        Ok(id) => {
            if let Some(ref tx) = state.dream_tx {
                let _ = tx.send(()).await;
            }
            Ok(Json(json!({ "status": "success", "id": id })))
        }
        Err(e) => {
            tracing::error!("API failed to save episode bidirectional: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn save_episodes_batch_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Vec<EpisodeSave>>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Some(backend) = state.backend.as_any().downcast_ref::<crate::db::SurrealBackend>() {
        match backend.save_episodes_batch(&payload).await {
            Ok(_) => {
                if let Some(ref tx) = state.dream_tx {
                    let _ = tx.send(()).await;
                }
                Ok(Json(json!({ "status": "success" })))
            }
            Err(e) => {
                tracing::error!("API failed to save episodes batch: {:?}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        Err(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

async fn search_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<SearchResponse>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let query = payload.get("query").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let scope = payload.get("scope").and_then(|v| v.as_str());
    let deep_insight = payload.get("deep_insight").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let offset = payload.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let threshold = payload.get("threshold").and_then(|v| v.as_f64()).map(|t| t as f32).unwrap_or(0.55);

    let token_budget = payload.get("token_budget").and_then(|v| v.as_u64()).map(|t| t as usize);
    let allow_downward = payload.get("allow_downward").and_then(|v| v.as_bool()).unwrap_or(false);
    let include_episodes = payload.get("include_episodes").and_then(|v| v.as_bool()).unwrap_or(false);
    let include_artifacts = payload.get("include_artifacts").and_then(|v| v.as_bool()).unwrap_or(false);
    let session_id = payload.get("session_id").and_then(|v| v.as_str());
    let include_archived = payload.get("include_archived").and_then(|v| v.as_bool()).unwrap_or(true);
    let temporal_anchor = payload.get("temporal_anchor").and_then(|v| v.as_str());

    match state.backend.search(crate::contracts::SearchParams::from_positional(
        query,
        scope,
        deep_insight,
        limit,
        offset,
        threshold,
        token_budget,
        allow_downward,
        include_episodes,
        include_artifacts,
        session_id,
        include_archived,
        temporal_anchor,
    )).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => {
            tracing::error!("Search failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn feedback_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Feedback>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.record_feedback(&payload.id, payload.success).await {
        Ok(_) => Ok(Json(json!({ "status": "success" }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_llm_config_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Json<LlmConfigResponse>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.get_llm_config().await {
        Ok(config) => Ok(Json(config)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn post_llm_config_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<LlmConfigRequest>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.update_llm_config(&payload).await {
        Ok(_) => {
            let expires: Option<String> = None;
            Ok(Json(json!({
                "status": "success",
                "active_provider": payload.provider,
                "expires_at": expires
            })))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn save_handoff_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<HandoffSave>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.save_handoff(&payload).await {
        Ok(id) => Ok(Json(json!({ "status": "success", "id": id }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn harvest_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let harvester = crate::cognitive::harvest::Harvester::new();
    match harvester.harvest_skills(&*state.backend, &state.store).await {
        Ok(_) => Ok(Json(json!({ "status": "success" }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn dream_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let scope = payload.get("scope").and_then(|v| v.as_str()).unwrap_or("general");
    let compactor = crate::cognitive::compactor::Compactor::new();
    
    let embedder = if let Some(backend) = state.backend.as_any().downcast_ref::<crate::db::backend::SurrealBackend>() {
        backend.embedder.clone()
    } else {
        None
    };
    
    let res_scope = compactor.compact_scope(&*state.backend, &state.store, scope, embedder).await;
    let res_global = compactor.compact_global(&*state.backend, &state.store).await;

    match (res_scope, res_global) {
        (Ok(_), Ok(_)) => Ok(Json(json!({ "status": "success" }))),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn get_memory_nodes_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<GetMemoryNodesRequest>,
) -> Result<Json<GetMemoryNodesResponse>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.get_memory_nodes(&payload.node_ids).await {
        Ok(res) => Ok(Json(res)),
        Err(e) => {
            tracing::error!("get_memory_nodes failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn save_forged_assets_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<ForgedSectionBatch>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.backend.save_forged_section(&payload).await {
        Ok(_) => {
            if let Some(ref tx) = state.dream_tx {
                let _ = tx.send(()).await;
            }
            Ok(Json(json!({ "status": "success" })))
        }
        Err(e) => {
            tracing::error!("API failed to save forged section: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_mcp_tools_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(crate::mcp_routes::get_mcp_tools_schema()))
}

async fn call_mcp_tool_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let name = match payload.get("name").and_then(|v| v.as_str()) {
        Some(n) if !n.is_empty() => n,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let args = payload
        .get("arguments")
        .or_else(|| payload.get("args"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    match crate::mcp_routes::call_mcp_tool(&state, name, args).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => {
            tracing::error!("MCP tool call failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn call_mcp_tool_batch_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let calls_val = if payload.is_array() {
        &payload
    } else {
        payload.get("calls").ok_or(StatusCode::BAD_REQUEST)?
    };

    let calls_arr = calls_val.as_array().ok_or(StatusCode::BAD_REQUEST)?;
    let mut futures = Vec::new();
    for call in calls_arr {
        let name = call.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let arguments = call.get("arguments").or_else(|| call.get("args")).cloned().unwrap_or(Value::Null);
        let state_ref = state.clone();
        futures.push(async move {
            match crate::mcp_routes::call_mcp_tool(&state_ref, &name, arguments).await {
                Ok(res) => json!({ "status": "success", "result": res }),
                Err(e) => json!({ "status": "error", "message": e.to_string() }),
            }
        });
    }

    let results = futures_util::future::join_all(futures).await;
    Ok(Json(Value::Array(results)))
}

async fn resources_list_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(Json(json!({
        "resources": [
            {
                "uri": "htr://tree",
                "name": "Active Hypothesis Tree",
                "description": "Structured JSON representation of the active hypothesis tree.",
                "mimeType": "application/json"
            }
        ]
    })))
}

async fn resources_read_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let uri = payload.get("uri").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    if uri == "htr://tree" {
        let sql = "SELECT * FROM hypothesis_node;";
        let surreal_backend = state.backend.as_any().downcast_ref::<crate::db::SurrealBackend>()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut response = match surreal_backend.db.query(sql).await {
            Ok(r) => r,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };
        let nodes: Vec<crate::contracts::HypothesisNode> = match response.take(0) {
            Ok(n) => n,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };
        let tree_str = match serde_json::to_string_pretty(&nodes) {
            Ok(s) => s,
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        };
        Ok(Json(json!({
            "contents": [
                {
                    "uri": "htr://tree",
                    "mimeType": "application/json",
                    "text": tree_str
                }
            ]
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn completions_proxy_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let external_url = std::env::var("MYTHRAX_COMPLETIONS_URL")
        .ok()
        .or_else(|| {
            #[cfg(not(feature = "mlx"))]
            {
                Some("http://127.0.0.1:8080/v1/chat/completions".to_string())
            }
            #[cfg(feature = "mlx")]
            {
                None
            }
        });

    if let Some(url) = external_url {
        let client = reqwest::Client::new();
        let req = client.post(&url).json(&payload);
        let is_stream = payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
        
        if is_stream {
            match req.send().await {
                Ok(resp) => {
                    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
                    let mut header_map = HeaderMap::new();
                    header_map.insert(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("text/event-stream"));
                    
                    use futures_util::StreamExt;
                    let stream = resp.bytes_stream().map(|r| r.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));
                    (status, header_map, axum::body::Body::from_stream(stream)).into_response()
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to proxy request: {}", e)).into_response()
                }
            }
        } else {
            match req.send().await {
                Ok(resp) => {
                    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
                    match resp.json::<Value>().await {
                        Ok(json_val) => (status, Json(json_val)).into_response(),
                        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse response: {}", e)).into_response(),
                    }
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to proxy request: {}", e)).into_response()
                }
            }
        }
    } else {
        #[cfg(feature = "mlx")]
        {
            let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or("mlx-community/Qwen3.6-35B-A3B-4bit");
            let messages = payload.get("messages").and_then(|v| v.as_array());
            
            let mut system_instruction = None;
            let mut prompt = String::new();
            if let Some(msgs) = messages {
                for msg in msgs {
                    let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                    if role == "system" {
                        system_instruction = Some(content);
                    } else if role == "user" {
                        if !prompt.is_empty() {
                            prompt.push_str("\n\n");
                        }
                        prompt.push_str(content);
                    } else if role == "assistant" {
                        if !prompt.is_empty() {
                            prompt.push_str("\n\n");
                        }
                        prompt.push_str(content);
                    }
                }
            }

            let client_llm = crate::llm::LLMClient::new();
            match client_llm.completion_explicit(
                state.backend.as_ref(),
                "local",
                "local",
                model,
                system_instruction,
                &prompt,
                false,
            ).await {
                Ok(response_text) => {
                    let is_stream = payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
                    if is_stream {
                        let chunk = serde_json::json!({
                            "choices": [{
                                "delta": { "content": response_text },
                                "index": 0,
                                "finish_reason": "stop"
                            }]
                        });
                        let sse_data = format!("data: {}\n\ndata: [DONE]\n\n", serde_json::to_string(&chunk).unwrap());
                        let mut header_map = HeaderMap::new();
                        header_map.insert(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("text/event-stream"));
                        (StatusCode::OK, header_map, sse_data).into_response()
                    } else {
                        let res_json = serde_json::json!({
                            "choices": [{
                                "message": { "role": "assistant", "content": response_text },
                                "index": 0,
                                "finish_reason": "stop"
                            }]
                        });
                        (StatusCode::OK, Json(res_json)).into_response()
                    }
                }
                Err(e) => {
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("In-process generation failed: {:?}", e)).into_response()
                }
            }
        }
        #[cfg(not(feature = "mlx"))]
        {
            (StatusCode::SERVICE_UNAVAILABLE, "In-process MLX engine is disabled and no completions proxy URL is configured.").into_response()
        }
    }
}

async fn ollama_proxy_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    axum::extract::Path(path): axum::extract::Path<String>,
    payload: Option<Bytes>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:8080/api/{}", path);
    
    let mut req = client.post(&url);
    if payload.is_none() {
        req = client.get(&url);
    } else if let Some(bytes) = payload {
        req = req.body(bytes);
    }
    
    match req.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
            let mut header_map = HeaderMap::new();
            if let Some(ct) = resp.headers().get(axum::http::header::CONTENT_TYPE) {
                header_map.insert(axum::http::header::CONTENT_TYPE, ct.clone());
            }
            (status, header_map, axum::body::Body::from_stream(resp.bytes_stream())).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to proxy Ollama request: {}", e)).into_response()
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SurrealBackend;
    use axum::http::Request;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_api_routes() {
        let backend = Arc::new(SurrealBackend::new_in_memory().await.unwrap());
        backend.init().await.unwrap();

        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(MarkdownStore::new(temp.path()).unwrap());
        let ignore_list = Arc::new(WatchIgnoreList::new());

        let state = Arc::new(ApiState {
            backend,
            auth_token: "secret-token".to_string(),
            store,
            ignore_list,
            dream_tx: None,
            shutdown_tx: None,
        });

        let app = create_router(state);

        // Test UNAUTHORIZED request
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/config/llm")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Test AUTHORIZED request
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/config/llm")
                    .header("X-Mythrax-Token", "secret-token")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /v1/nodes (Authorized)
        let request_body = serde_json::json!({
            "node_ids": ["episode:test-uuid"]
        });
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/nodes")
                    .header("X-Mythrax-Token", "secret-token")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test GET /v1/mcp/tools (Authorized)
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/mcp/tools")
                    .header("X-Mythrax-Token", "secret-token")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test GET /v1/mcp/tools (Unauthorized)
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/mcp/tools")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Test POST /v1/mcp/call (Authorized, read action get_vault_root)
        let call_payload = serde_json::json!({
            "name": "read",
            "arguments": {
                "action": "get_vault_root"
            }
        });
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/mcp/call")
                    .header("X-Mythrax-Token", "secret-token")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&call_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Test POST /v1/mcp/call (Unauthorized)
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/mcp/call")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&call_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Test POST /v1/mcp/call (Bad Request - missing name)
        let bad_payload = serde_json::json!({
            "arguments": {}
        });
        let response = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/mcp/call")
                    .header("X-Mythrax-Token", "secret-token")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&bad_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[derive(serde::Deserialize)]
struct HookQuery {
    host: Option<String>,
}

async fn precompact_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<HookQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let host = query.host.unwrap_or_else(|| "gemini".to_string());
    let (sanitized_session, _, normalized_path) = match crate::hooks::adapters::adapt_payload(body, &host) {
        Ok(tup) => tup,
        Err(e) => {
            tracing::error!("Failed to adapt precompact payload for host '{}': {:?}", host, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let mcp_args = json!({
        "action": "precompact",
        "session_id": sanitized_session,
        "transcript_path": normalized_path,
    });

    match crate::mcp_routes::call_mcp_tool(&state, "manage", mcp_args).await {
        Ok(val) => Ok(Json(val)),
        Err(e) => {
            tracing::error!("Precompact hook tool call failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn stop_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<HookQuery>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let host = query.host.unwrap_or_else(|| "gemini".to_string());
    let (sanitized_session, stop_hook_active, normalized_path) = match crate::hooks::adapters::adapt_payload(body, &host) {
        Ok(tup) => tup,
        Err(e) => {
            tracing::error!("Failed to adapt stop payload for host '{}': {:?}", host, e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match crate::hooks::stop::mine_if_due(
        &sanitized_session,
        &normalized_path,
        stop_hook_active,
        &state.backend,
        &state.store,
        &state.ignore_list,
    ).await {
        Ok(count_opt) => Ok(Json(json!({
            "status": "success",
            "episodes_saved": count_opt,
            "block": count_opt.is_some()
        }))),
        Err(e) => {
            tracing::error!("Stop hook failed: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn stop_daemon_endpoint_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let pid_path_clone = std::path::PathBuf::from(home).join(".mythrax/daemon.pid");
    
    if let Some(tx) = &state.shutdown_tx {
        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            tracing::info!("Graceful shutdown requested via API. Sending shutdown signal...");
            let _ = tx.send(()).await;
        });
    } else {
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            tracing::info!("Graceful shutdown requested via API (no shutdown_tx). Exiting...");
            let _ = std::fs::remove_file(pid_path_clone);
            std::process::exit(0);
        });
    }

    Ok(Json(json!({ "status": "stopping" })))
}
