use axum::{
    routing::{get, post},
    Router, Json, http::{StatusCode, HeaderMap},
    extract::State,
};
use std::sync::Arc;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, SearchResult, Feedback, LlmConfigRequest, LlmConfigResponse, HandoffSave};
use crate::store::MarkdownStore;
use crate::vault::watcher::WatchIgnoreList;
use serde_json::{json, Value};

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
        .with_state(state)
}

fn check_auth(headers: &HeaderMap, state: &ApiState) -> bool {
    if let Some(token_header) = headers.get("X-Mythrax-Token") {
        if let Ok(token_str) = token_header.to_str() {
            return token_str == state.auth_token;
        }
    }
    false
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
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let query = payload.get("query").and_then(|v| v.as_str()).ok_or(StatusCode::BAD_REQUEST)?;
    let scope = payload.get("scope").and_then(|v| v.as_str());
    let deep_insight = payload.get("deep_insight").and_then(|v| v.as_bool()).unwrap_or(false);
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let offset = payload.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    match state.backend.search(query, scope, deep_insight, limit, offset).await {
        Ok(res) => Ok(Json(res)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
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
    match harvester.harvest_skills(&*state.backend, &*state.store).await {
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
    
    let res_scope = compactor.compact_scope(&*state.backend, &*state.store, scope).await;
    let res_global = compactor.compact_global(&*state.backend, &*state.store).await;

    match (res_scope, res_global) {
        (Ok(_), Ok(_)) => Ok(Json(json!({ "status": "success" }))),
        _ => Err(StatusCode::INTERNAL_SERVER_ERROR),
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
        let response = app
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
    }
}
