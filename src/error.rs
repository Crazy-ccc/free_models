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

#[allow(dead_code)]
pub fn bad_request(message: &str) -> HttpResponse {
    openai_error(
        actix_web::http::StatusCode::BAD_REQUEST,
        message,
        "invalid_request_error",
    )
}

#[allow(dead_code)]
pub fn not_found(message: &str) -> HttpResponse {
    openai_error(
        actix_web::http::StatusCode::NOT_FOUND,
        message,
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