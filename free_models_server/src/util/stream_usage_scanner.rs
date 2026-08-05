use serde_json::Value;

use crate::util::proxy_types::{Protocol, UsageInfo};

pub(crate) fn is_quota_error(error_text: &str) -> bool {
    let lower = error_text.to_lowercase();
    [
        "insufficient_quota",
        "insufficient_credits",
        "insufficient_balance",
        "out of credits",
        "exceeded your current quota",
        "billing",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

/// 通用 SSE 字节流扫描器：维护尾部缓冲，按最后一个 `\n\n` 切分完整事件，
/// 对每个 `data: ` 行解析出的 JSON 对象调用 `scan`。
pub struct SseScanner {
    tail: Vec<u8>,
    pub done: bool,
}

impl SseScanner {
    pub fn new() -> Self {
        Self {
            tail: Vec::new(),
            done: false,
        }
    }

    pub fn push<T>(&mut self, bytes: &[u8], scan: impl Fn(&Value) -> Option<T>) -> Option<T> {
        if self.done {
            return None;
        }
        self.tail.extend_from_slice(bytes);
        let Some(last_nn) = self
            .tail
            .windows(2)
            .rposition(|w| w == b"\n\n")
            .map(|pos| pos + 1)
        else {
            return None;
        };
        let result = scan_sse_buf(&self.tail[..=last_nn], &scan);
        if result.is_some() {
            self.done = true;
        }
        self.tail.drain(..=last_nn);
        result
    }

    pub fn finish<T>(&mut self, scan: impl Fn(&Value) -> Option<T>) -> Option<T> {
        if self.done {
            return None;
        }
        let result = scan_sse_buf(&self.tail, &scan);
        if result.is_some() {
            self.done = true;
        }
        self.tail.clear();
        result
    }
}

fn scan_sse_buf<T>(buf: &[u8], scan: &impl Fn(&Value) -> Option<T>) -> Option<T> {
    let s = std::str::from_utf8(buf).ok()?;
    for event in s.split("\n\n") {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(json) = serde_json::from_str::<Value>(data)
            {
                if let Some(result) = scan(&json) {
                    return Some(result);
                }
            }
        }
    }
    None
}

/// 从单个 SSE JSON 对象中提取用量信息（原 find_usage_in_json 逻辑）。
pub(crate) fn scan_usage_json(json: &Value, protocol: Protocol) -> Option<UsageInfo> {
    if let Some(usage) = json.get("usage") {
        if !usage.is_null() {
            return Some(protocol.extract_usage(usage));
        }
    }
    if protocol == Protocol::Responses {
        if let Some(response) = json.get("response") {
            if let Some(usage) = response.get("usage") {
                if !usage.is_null() {
                    return Some(protocol.extract_usage(usage));
                }
            }
        }
    }
    None
}

/// 从单个 SSE JSON 对象中提取错误信息与配额标记。
pub(crate) fn scan_error_json(json: &Value) -> Option<(String, bool)> {
    if let Some(error) = json.get("error") {
        if !error.is_null() {
            let message = extract_error_message(error);
            let is_quota = is_quota_error(&message);
            return Some((message, is_quota));
        }
    }
    None
}

pub(crate) fn extract_error_message(error: &Value) -> String {
    if let Some(obj) = error.as_object() {
        if let Some(message) = obj.get("message") {
            if !message.is_null() {
                return if let Some(s) = message.as_str() {
                    s.to_string()
                } else {
                    message.to_string()
                };
            }
        }
        if let Some(error_type) = obj.get("type") {
            if !error_type.is_null() {
                return if let Some(s) = error_type.as_str() {
                    s.to_string()
                } else {
                    error_type.to_string()
                };
            }
        }
        if let Some(code) = obj.get("code") {
            if !code.is_null() {
                return if let Some(s) = code.as_str() {
                    s.to_string()
                } else {
                    code.to_string()
                };
            }
        }
        return error.to_string();
    }
    if let Some(s) = error.as_str() {
        return s.to_string();
    }
    error.to_string()
}
