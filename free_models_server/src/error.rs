use actix_web::HttpResponse;
use serde_json::json;

pub fn openai_error(
    status: actix_web::http::StatusCode,
    message: &str,
    error_type: &str,
) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "error": {
            "message": message,
            "type": error_type
        }
    }))
}

pub fn unauthorized() -> HttpResponse {
    openai_error(
        actix_web::http::StatusCode::UNAUTHORIZED,
        "Invalid API key",
        "invalid_request_error",
    )
}

pub fn internal_error(message: &str) -> HttpResponse {
    openai_error(
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        message,
        "server_error",
    )
}

pub fn service_unavailable(message: &str) -> HttpResponse {
    openai_error(
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
        message,
        "server_error",
    )
}

// ========== Anthropic 格式错误响应 ==========

pub fn anthropic_error(
    status: actix_web::http::StatusCode,
    error_type: &str,
    message: &str,
) -> HttpResponse {
    HttpResponse::build(status).json(json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message
        }
    }))
}

pub fn anthropic_unauthorized() -> HttpResponse {
    anthropic_error(
        actix_web::http::StatusCode::UNAUTHORIZED,
        "authentication_error",
        "Invalid API key",
    )
}

pub fn anthropic_bad_request(message: &str) -> HttpResponse {
    anthropic_error(
        actix_web::http::StatusCode::BAD_REQUEST,
        "invalid_request_error",
        message,
    )
}

pub fn anthropic_internal_error(message: &str) -> HttpResponse {
    anthropic_error(
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        "api_error",
        message,
    )
}

pub fn anthropic_service_unavailable(message: &str) -> HttpResponse {
    anthropic_error(
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
        "api_error",
        message,
    )
}

/// 统一的数据库错误 500 响应
pub fn db_error() -> HttpResponse {
    HttpResponse::InternalServerError().json(json!({
        "error": "Database error"
    }))
}