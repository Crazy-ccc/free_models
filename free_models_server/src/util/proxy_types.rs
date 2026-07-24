use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use log::info;
use serde_json::Value;

use crate::response;

/// 日志状态常量，避免硬编码字符串
pub mod log_status {
    pub const SUCCESS: &str = "success";
    pub const FAILED: &str = "failed";
}

/// 支持的协议类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAI,
    Anthropic,
}

impl Protocol {
    /// 返回上游 API 路径（不含 base_url）
    pub fn path(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "/chat/completions",
            Protocol::Anthropic => "/messages",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "openai",
            Protocol::Anthropic => "anthropic",
        }
    }

    pub fn internal_error(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => response::internal_error(message),
            Protocol::Anthropic => response::anthropic_internal_error(message),
        }
    }

    pub fn service_unavailable(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => response::service_unavailable(message),
            Protocol::Anthropic => response::anthropic_service_unavailable(message),
        }
    }

    pub fn bad_request(&self, message: &str) -> HttpResponse {
        match self {
            Protocol::OpenAI => response::openai_error(
                StatusCode::BAD_REQUEST,
                message,
                "invalid_request_error",
            ),
            Protocol::Anthropic => response::anthropic_bad_request(message),
        }
    }

    pub fn extract_usage(&self, usage: &Value) -> UsageInfo {
        fn get_i64(v: &Value, key: &str) -> i64 {
            v.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
        }

        match self {
            Protocol::OpenAI => {
                let prompt = get_i64(usage, "prompt_tokens") as i32;
                let completion = get_i64(usage, "completion_tokens") as i32;
                let total = get_i64(usage, "total_tokens") as i32;
                let cached = usage
                    .get("prompt_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(get_i64(usage, "prompt_cache_hit_tokens")) as i32;
                UsageInfo {
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    total_tokens: total,
                    cache_hit_tokens: cached,
                    cache_miss_tokens: prompt - cached,
                }
            }
            Protocol::Anthropic => {
                let input = get_i64(usage, "input_tokens") as i32;
                let output = get_i64(usage, "output_tokens") as i32;
                let cache_read = get_i64(usage, "cache_read_input_tokens") as i32;
                UsageInfo {
                    prompt_tokens: input,
                    completion_tokens: output,
                    total_tokens: input + output,
                    cache_hit_tokens: cache_read,
                    cache_miss_tokens: input,
                }
            }
        }
    }
}

/// 从上游响应的 usage 对象中提取 token 指标，兼容 OpenAI 和 Anthropic 两种格式
pub struct UsageInfo {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub cache_miss_tokens: i32,
}
