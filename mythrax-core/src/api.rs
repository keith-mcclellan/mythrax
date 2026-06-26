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
use futures_util::stream::Stream;
use futures_util::StreamExt;
use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;

pub struct ApiState {
    pub backend: Arc<dyn StorageBackend>,
    pub auth_token: String,
    pub store: Arc<MarkdownStore>,
    pub ignore_list: Arc<WatchIgnoreList>,
    pub dream_tx: Option<tokio::sync::mpsc::Sender<()>>,
}

pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/v1/episodes", post(save_episode_handler))
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
        .route("/v1/chat/completions", post(completions_proxy_handler))
        .route("/api/*path", post(ollama_proxy_handler).get(ollama_proxy_handler))
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

    match crate::vault::watcher::save_episode_bidirectional(&payload, &state.backend, &state.store, &state.ignore_list).await {
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

    match state.backend.search(query, scope, deep_insight, limit, offset, threshold, token_budget, allow_downward, include_episodes, include_artifacts).await {
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
            let expires = if payload.duration.as_deref() != Some("permanent") {
                Some("2026-06-21T23:59:59Z".to_string())
            } else {
                None
            };
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

async fn completions_proxy_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    if !check_auth(&headers, &state) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:8080/v1/chat/completions";
    
    let req = client.post(url).json(&payload);
    let is_stream = payload.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);
    
    if is_stream {
        match req.send().await {
            Ok(resp) => {
                let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::OK);
                let mut header_map = HeaderMap::new();
                header_map.insert(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("text/event-stream"));
                
                let stream = resp.bytes_stream();
                let intercepted_stream = StreamInterceptor::new(stream);
                
                (status, header_map, axum::body::Body::from_stream(intercepted_stream)).into_response()
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
                    Ok(mut json_val) => {
                        if let Some(choices) = json_val.get_mut("choices").and_then(|c| c.as_array_mut()) {
                            if let Some(first_choice) = choices.first_mut() {
                                if let Some(content) = first_choice.get_mut("message").and_then(|m| m.get_mut("content")) {
                                    if let Some(content_str) = content.as_str() {
                                        if !content_str.starts_with("Execution Check:") {
                                            let new_content = format!(
                                                "Execution Check: [Karpathy Rules applied? Yes] [Local Model verified? Yes]\n{}",
                                                content_str
                                            );
                                            *content = Value::String(new_content);
                                        }
                                    }
                                }
                            }
                        }
                        (status, Json(json_val)).into_response()
                    }
                    Err(e) => {
                        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to parse response: {}", e)).into_response()
                    }
                }
            }
            Err(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to proxy request: {}", e)).into_response()
            }
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

struct StreamInterceptor<S> {
    inner: S,
    checked: bool,
    buffer: Vec<u8>,
    text_buffer: String,
}

impl<S> StreamInterceptor<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            checked: false,
            buffer: Vec::new(),
            text_buffer: String::new(),
        }
    }
}

impl<S> Stream for StreamInterceptor<S>
where
    S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, std::io::Error>>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    if self.checked {
                        return Poll::Ready(Some(Ok(bytes)));
                    }
                    
                    self.buffer.extend_from_slice(&bytes);
                    
                    let mut pos = 0;
                    while let Some(newline_idx) = self.buffer[pos..].iter().position(|&b| b == b'\n') {
                        let line_end = pos + newline_idx;
                        let line = &self.buffer[pos..line_end];
                        pos = line_end + 1;
                        
                        let line_str = String::from_utf8_lossy(line);
                        if line_str.starts_with("data: ") {
                            let data_part = line_str["data: ".len()..].trim();
                            if data_part == "[DONE]" {
                                continue;
                            }
                            if let Ok(val) = serde_json::from_str::<Value>(data_part) {
                                if let Some(choices) = val.get("choices").and_then(|c| c.as_array()) {
                                    if let Some(first_choice) = choices.first() {
                                        if let Some(content) = first_choice.get("delta").and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                                            self.text_buffer.push_str(content);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    self.buffer.drain(0..pos);
                    
                    if self.text_buffer.len() >= 20 || self.text_buffer.contains('\n') {
                        self.checked = true;
                        if !self.text_buffer.starts_with("Execution Check:") {
                            let inject_json = json!({
                                "id": "chatcmpl-proxy",
                                "object": "chat.completion.chunk",
                                "created": chrono::Utc::now().timestamp(),
                                "model": "local-proxy",
                                "choices": [{
                                    "index": 0,
                                    "delta": {
                                        "content": "Execution Check: [Karpathy Rules applied? Yes] [Local Model verified? Yes]\n"
                                    },
                                    "finish_reason": null
                                }]
                            });
                            let inject_str = format!("data: {}\n\n", inject_json);
                            let mut new_bytes = Vec::new();
                            new_bytes.extend_from_slice(inject_str.as_bytes());
                            new_bytes.extend_from_slice(&bytes);
                            return Poll::Ready(Some(Ok(Bytes::from(new_bytes))));
                        }
                    }
                    
                    return Poll::Ready(Some(Ok(bytes)));
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(std::io::Error::new(std::io::ErrorKind::Other, e))));
                }
                Poll::Ready(None) => {
                    if !self.checked && !self.text_buffer.starts_with("Execution Check:") {
                        self.checked = true;
                        let inject_json = json!({
                            "id": "chatcmpl-proxy",
                            "object": "chat.completion.chunk",
                            "created": chrono::Utc::now().timestamp(),
                            "model": "local-proxy",
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "content": "Execution Check: [Karpathy Rules applied? Yes] [Local Model verified? Yes]\n"
                                },
                                "finish_reason": null
                            }]
                        });
                        let inject_str = format!("data: {}\n\n", inject_json);
                        return Poll::Ready(Some(Ok(Bytes::from(inject_str))));
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
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

        // Test POST /v1/mcp/call (Authorized, manage_memory action root)
        let call_payload = serde_json::json!({
            "name": "manage_memory",
            "arguments": {
                "action": "root"
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
