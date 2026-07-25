# 代理转发与 Token 计算文档

---

## 代理转发核心

代理转发的核心逻辑在 [proxy_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/proxy_service.rs) 中实现，入口为 `chat_handler.rs`。

### 转发流程

```
handle_chat_request (chat_handler.rs)
    │
    ├─ 校验 messages 字段非空
    ├─ 获取可用模型列表（ModelCache / DB）
    ├─ 按协议筛选（openai / anthropic）
    ├─ 指定模型排到最前（merge_preferred_first）
    ├─ 惩罚排序（sort_penalized_last）
    ├─ Token 估算 + 上下文窗口过滤
    │
    └─ proxy_chat_completion_inner
        └─ 遍历模型列表
            ├─ forward_to_provider
            │   ├─ 构建 URL: {base_url.trim_end('/')}{protocol.path()}
            │   ├─ 替换 model: body["model"] = model_info.model_id
            │   ├─ 设置 stream_options (OpenAI SSE)
            │   ├─ 设置认证头:
            │   │   ├─ OpenAI: Bearer {api_key}
            │   │   └─ Anthropic: x-api-key + anthropic-version
            │   ├─ 发送请求（带超时）
            │   └─ 判断结果
            │       ├─ 2xx → ForwardOutcome::Success
            │       └─ 4xx/5xx/超时/网络错 → ForwardOutcome::Retry
            │
            ├─ Success → handle_stream_response / handle_non_stream_response
            ├─ Retry → penalty.penalize → continue
            └─ 全部失败 → 503 + 记录 failed 用量日志
```

### ModelProviderInfo

转发会话中使用的完整模型+供应商+凭证信息：

```rust
pub struct ModelProviderInfo {
    pub model_name: String,        // 模型对外名称
    pub model_id: String,          // 上游 API 实际 model 值
    pub model_config_id: i32,      // 模型配置 ID
    pub provider_config_id: i32,   // 供应商 ID
    pub provider_name: String,     // 供应商名称
    pub base_url: String,          // 供应商基础 URL
    pub api_key: String,           // 解密后的 API Key
    pub provider_credential_id: i32, // 使用的凭证 ID
    pub priority: i32,             // 优先级
    pub timeout: u64,              // 超时秒数
    pub protocols: String,         // 支持的协议
    pub context_length: i32,       // 上下文窗口
}
```

### 协议适配

```rust
impl Protocol {
    pub fn path(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "/chat/completions",
            Protocol::Anthropic => "/messages",
        }
    }

    // 认证头设置
    request = match protocol {
        Protocol::OpenAI =>
            request.header("Authorization", format!("Bearer {}", model_info.api_key)),
        Protocol::Anthropic =>
            request.header("x-api-key", &model_info.api_key)
                   .header("anthropic-version", "2023-06-01"),
    };
}
```

### 流式响应处理

**SSE 流式转发** 使用 `tokio::sync::mpsc` 通道在后台任务中消费上游流式响应：

```rust
// 后台任务：消费上游 stream → 检测 usage → 写入日志 → 转发到通道
actix_web::rt::spawn(async move {
    let mut upstream_stream = resp.bytes_stream();
    let mut buffer = Vec::new();
    let mut logged_usage = false;

    while let Some(item) = upstream_stream.next().await {
        match item {
            Ok(bytes) => {
                buffer.extend_from_slice(&bytes);
                // 检测 usage 字段
                if !logged_usage {
                    if let Some(info) = extract_usage_from_buffer(&buffer, protocol) {
                        spawn_usage_log(...);
                        logged_usage = true;
                    }
                }
                let _ = tx.send(Ok(bytes));
            }
            Err(_) => {
                let _ = tx.send(Ok(sse_close_event_for(protocol)));
                break;
            }
        }
    }
});
```

**非流式响应** 直接等待完整响应后解析 `usage` 字段写入日志。

### 故障切换与惩罚

当上游请求失败时，自动标记惩罚并尝试下一个模型：

```rust
ForwardOutcome::Retry => {
    penalty.penalize(&model_info.model_name, &model_info.provider_name).await;
    continue;
}
```

被惩罚的 `(model, provider)` 组合会在 TTL 内被排序到列表末尾（详见 [caching_strategy.md](./caching_strategy.md)）。

### 关闭帧

| 协议 | 流结束时 | 流中断时 |
|------|---------|---------|
| OpenAI | `data: [DONE]\n\n` | `data: [DONE]\n\n` |
| Anthropic | 由上游发送 | `event: error\ndata: {"type":"error","error":{"type":"api_error","message":"upstream stream interrupted"}}\n\n` |

---

## Token 计算

Token 计算模块在 [tokenizer.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/tokenizer.rs) 中实现，使用 `tiktoken-rs` 库的 `cl100k_base` BPE 编码器。

### 核心函数

```rust
use std::sync::OnceLock;

fn tiktoken_bpe() -> &'static tiktoken_rs::CoreBPE {
    static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("Failed to initialize cl100k_base tokenizer")
    })
}

pub fn estimate_prompt_tokens(text: &str) -> usize {
    tiktoken_bpe().encode_with_special_tokens(text).len()
}
```

使用 `OnceLock` 实现单例初始化，仅在首次调用时创建 `CoreBPE` 实例。

### 用途

1. **上下文窗口校验：** 转发请求前，估算 prompt 的 token 数，与模型的 `context_length` 对比，过滤掉窗口不足的模型
2. **用量日志记录：** 请求完成后，从上游响应中读取实际的 usage 数据写入 `usage_log` 表

### 验证超长 Prompt

```rust
// 在 chat_handler.rs 中
let estimated_prompt = prompt_text.iter()
    .map(|s| tokenizer::estimate_prompt_tokens(s))
    .sum::<usize>() as i32;

let models: Vec<_> = models.into_iter()
    .filter(|m| estimated_prompt <= m.context_length)
    .collect();

if models.is_empty() {
    return protocol.bad_request(
        &format!("Prompt too long (estimated {} tokens), exceeds all available models' context window", estimated_prompt)
    );
}
```

### 用量日志写入

用量日志的解析在 `proxy_service.rs` 的 `Protocol::extract_usage` 方法中：

| 指标 | OpenAI 字段路径 | Anthropic 字段路径 |
|------|----------------|-------------------|
| prompt_tokens | `usage.prompt_tokens` | `usage.input_tokens` |
| completion_tokens | `usage.completion_tokens` | `usage.output_tokens` |
| total_tokens | `usage.total_tokens` | `input_tokens + output_tokens` |
| cache_hit_tokens | `usage.prompt_tokens_details.cached_tokens.prompt_cache_hit_tokens` | `usage.cache_read_input_tokens` |
| cache_miss_tokens | `prompt_tokens - cached_tokens` | `input_tokens` |

日志写入使用 `actix_web::rt::spawn` 在后台异步执行，不阻塞主请求流程。

### 代码位置

| 功能 | 文件 |
|------|------|
| Token 估算 | [tokenizer.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/tokenizer.rs) |
| 用量提取 | [proxy_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/proxy_service.rs)（`Protocol::extract_usage`） |
| 上下文校验 | [chat_handler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/handler/chat_handler.rs) |
| 用量日志写入 | [usage_log_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/usage_log_service.rs) |
