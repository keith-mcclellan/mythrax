use axum::{
    routing::{get, post},
    Router, Json, http::{StatusCode, HeaderMap},
    extract::State,
};
use std::sync::Arc;
use crate::db::StorageBackend;
use crate::contracts::{EpisodeSave, SearchResult, Feedback, LlmConfigRequest, LlmConfigResponse};
use serde_json::{json, Value};

pub struct ApiState {
    pub backend: Arc<dyn StorageBackend>,
    pub auth_token: String,
}

pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/v1/episodes", post(save_episode_handler))
        .route("/v1/search", post(search_handler))
        .route("/v1/feedback", post(feedback_handler))
        .route("/v1/config/llm", get(get_llm_config_handler).post(post_llm_config_handler))
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

    match state.backend.save_episode(&payload).await {
        Ok(id) => Ok(Json(json!({ "status": "success", "id": id }))),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
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
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    match state.backend.search(query, scope, limit).await {
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

    // Default configuration (placeholder for database metadata read)
    Ok(Json(LlmConfigResponse {
        active_provider: "cloud".to_string(),
        cloud_provider: "gemini".to_string(),
        model: "gemini-1.5-flash".to_string(),
        is_override: false,
        expires_at: None,
    }))
}

async fn post_llm_config_handler(
    State(state): State<Arc<ApiState>>,
    headers: HeaderMap,
    Json(payload): Json<LlmConfigRequest>,
) -> Result<Json<Value>, StatusCode> {
    if !check_auth(&headers, &state) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Dynamic config write logic (placeholder for database write)
    let expires = if payload.duration.as_deref() != Some("permanent") {
        Some("2026-06-20T23:59:59Z".to_string())
    } else {
        None
    };

    Ok(Json(json!({
        "status": "success",
        "active_provider": payload.provider,
        "expires_at": expires
    })))
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

        let state = Arc::new(ApiState {
            backend,
            auth_token: "secret-token".to_string(),
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
