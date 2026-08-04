use serde_json::Value;

use crate::util::proxy_types::{Protocol, UsageInfo};

pub struct StreamUsageScanner {
    tail: Vec<u8>,
    pub done: bool,
    protocol: Protocol,
}

fn find_usage_in_json(json: &Value, protocol: Protocol) -> Option<UsageInfo> {
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

fn scan_usage(buf: &[u8], protocol: Protocol) -> Option<UsageInfo> {
    let s = std::str::from_utf8(buf).ok()?;
    for event in s.split("\n\n") {
        for line in event.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(json) = serde_json::from_str::<Value>(data)
            {
                if let Some(info) = find_usage_in_json(&json, protocol) {
                    return Some(info);
                }
            }
        }
    }
    None
}

impl StreamUsageScanner {
    pub fn new(protocol: Protocol) -> Self {
        Self {
            tail: Vec::new(),
            done: false,
            protocol,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Option<UsageInfo> {
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
        let info = scan_usage(&self.tail[..=last_nn], self.protocol);
        if info.is_some() {
            self.done = true;
        }
        self.tail.drain(..=last_nn);
        info
    }

    pub fn finish(&mut self) -> Option<UsageInfo> {
        if self.done {
            return None;
        }
        let info = scan_usage(&self.tail, self.protocol);
        if info.is_some() {
            self.done = true;
        }
        self.tail.clear();
        info
    }
}