use actix_web::http::StatusCode;
use actix_web::HttpResponse;
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
    Responses,
}

impl Protocol {
    /// 返回上游 API 路径（不含 base_url）
    pub fn path(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "/chat/completions",
            Protocol::Anthropic => "/messages",
            Protocol::Responses => "/responses",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "openai",
            Protocol::Anthropic => "anthropic",
            Protocol::Responses => "responses",
        }
    }

    pub fn internal_error(&self, message: &str) -> HttpResponse {
        self.error_for_protocol(response::internal_error, response::anthropic_internal_error, message)
    }

    pub fn service_unavailable(&self, message: &str) -> HttpResponse {
        self.error_for_protocol(response::service_unavailable, response::anthropic_service_unavailable, message)
    }

    pub fn bad_request(&self, message: &str) -> HttpResponse {
        self.error_for_protocol(
            |msg| response::openai_error(StatusCode::BAD_REQUEST, msg, "invalid_request_error"),
            response::anthropic_bad_request,
            message,
        )
    }

    fn error_for_protocol(
        &self,
        openai_fn: fn(&str) -> HttpResponse,
        anthropic_fn: fn(&str) -> HttpResponse,
        msg: &str,
    ) -> HttpResponse {
        match self {
            Protocol::OpenAI | Protocol::Responses => openai_fn(msg),
            Protocol::Anthropic => anthropic_fn(msg),
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
            Protocol::Responses => {
                let input = get_i64(usage, "input_tokens") as i32;
                let output = get_i64(usage, "output_tokens") as i32;
                let total = get_i64(usage, "total_tokens") as i32;
                let cached = usage
                    .get("input_tokens_details")
                    .and_then(|d| d.get("cached_tokens"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as i32;
                UsageInfo {
                    prompt_tokens: input,
                    completion_tokens: output,
                    total_tokens: total,
                    cache_hit_tokens: cached,
                    cache_miss_tokens: input - cached,
                }
            }
        }
    }
}

/// 从上游响应的 usage 对象中提取 token 指标，兼容 OpenAI 和 Anthropic 两种格式
#[derive(Default, Clone)]
pub struct UsageInfo {
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_hit_tokens: i32,
    pub cache_miss_tokens: i32,
}

/// 当前请求所用的 API Key 上下文信息，用于日志记录。
#[derive(Clone)]
pub struct ApiKeyContext {
    pub id: Option<i32>,
    pub name: Option<String>,
}

/// 转发请求的元信息。`Protocol` 已 derive `Copy`，因此本结构也 derive `Copy`。
#[derive(Clone, Copy)]
pub struct ForwardMeta {
    pub protocol: Protocol,
    pub is_stream: bool,
    pub start_time: std::time::Instant,
}

/// 一次转发请求结束后的日志上下文，汇集协议、耗时、用量、状态与错误信息。
#[derive(Clone)]
pub struct LogContext {
    pub protocol: Protocol,
    pub duration_ms: i32,
    pub is_stream: bool,
    pub info: UsageInfo,
    pub status: String,
    pub error_message: Option<String>,
}
