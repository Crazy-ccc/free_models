use actix_web::HttpResponse;
use serde_json::json;

use crate::db::StoreError;

#[derive(Clone, Copy)]
enum ErrorFormat {
    Flat,
    OpenAI,
    Anthropic,
}

fn error_response(
    format: ErrorFormat,
    status: actix_web::http::StatusCode,
    message: &str,
    error_type: &str,
) -> HttpResponse {
    let body = match format {
        ErrorFormat::Flat => json!({ "error": message }),
        ErrorFormat::OpenAI => json!({ "error": { "message": message, "type": error_type } }),
        ErrorFormat::Anthropic => {
            json!({ "type": "error", "error": { "type": error_type, "message": message } })
        }
    };
    HttpResponse::build(status).json(body)
}

pub fn openai_error(
    status: actix_web::http::StatusCode,
    message: &str,
    error_type: &str,
) -> HttpResponse {
    error_response(ErrorFormat::OpenAI, status, message, error_type)
}

pub fn unauthorized() -> HttpResponse {
    error_response(
        ErrorFormat::OpenAI,
        actix_web::http::StatusCode::UNAUTHORIZED,
        "Invalid API key",
        "invalid_request_error",
    )
}

pub fn internal_error(message: &str) -> HttpResponse {
    error_response(
        ErrorFormat::OpenAI,
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        message,
        "server_error",
    )
}

pub fn service_unavailable(message: &str) -> HttpResponse {
    error_response(
        ErrorFormat::OpenAI,
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
        message,
        "server_error",
    )
}

// ========== Anthropic 格式错误响应 ==========

#[allow(dead_code)]
pub fn anthropic_error(
    status: actix_web::http::StatusCode,
    error_type: &str,
    message: &str,
) -> HttpResponse {
    error_response(ErrorFormat::Anthropic, status, message, error_type)
}

pub fn anthropic_unauthorized() -> HttpResponse {
    error_response(
        ErrorFormat::Anthropic,
        actix_web::http::StatusCode::UNAUTHORIZED,
        "Invalid API key",
        "authentication_error",
    )
}

pub fn anthropic_bad_request(message: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Anthropic,
        actix_web::http::StatusCode::BAD_REQUEST,
        message,
        "invalid_request_error",
    )
}

pub fn anthropic_internal_error(message: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Anthropic,
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        message,
        "api_error",
    )
}

pub fn anthropic_service_unavailable(message: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Anthropic,
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
        message,
        "api_error",
    )
}

/// 统一的数据库错误 500 响应
pub fn db_error() -> HttpResponse {
    error_response(
        ErrorFormat::Flat,
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        "Database error",
        "",
    )
}

/// 将 StoreError 映射为 HTTP 响应
pub fn from_store_error(e: StoreError) -> HttpResponse {
    match e {
        StoreError::NotFound(msg) => error_response(
            ErrorFormat::Flat,
            actix_web::http::StatusCode::NOT_FOUND,
            &msg,
            "",
        ),
        StoreError::Database(msg) => error_response(
            ErrorFormat::Flat,
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            &msg,
            "",
        ),
    }
}

/// 扁平结构错误响应（与 db_error/from_store_error 一致），用于 admin 接口
pub fn not_found(msg: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Flat,
        actix_web::http::StatusCode::NOT_FOUND,
        msg,
        "",
    )
}

pub fn bad_request(msg: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Flat,
        actix_web::http::StatusCode::BAD_REQUEST,
        msg,
        "",
    )
}

pub fn bad_gateway(msg: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Flat,
        actix_web::http::StatusCode::BAD_GATEWAY,
        msg,
        "",
    )
}

pub fn internal(msg: &str) -> HttpResponse {
    error_response(
        ErrorFormat::Flat,
        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        msg,
        "",
    )
}
