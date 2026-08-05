# 代理转发与 Token 计算文档

---

## 代理转发核心

代理转发的核心逻辑在 [proxy_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/proxy_service.rs) 中实现，入口为 `chat_handler.rs`。

### 转发流程

```
handle_chat_request (chat_handler.rs)
    │
    ├─ validate_request_body 校验请求体
    │   ├─ Responses：input 为非空 String 或非空 Array
    │   └─ OpenAI / Anthropic：messages 为非空数组
    ├─ schedule_all_available 获取可用模型（SchedulerCache：Redis + moka，Redis 不可用降级 moka）
    ├─ supports_protocol 按协议过滤（openai / anthropic / responses），过滤后为空 → 503
    ├─ merge_preferred_first 请求指定的 model 排到最前
    ├─ extract_prompt_text + estimate_prompt_tokens（tiktoken cl100k_base）估算 prompt
    ├─ filter_by_context_window 过滤窗口不足的模型（任一映射 context_length >= 估算值即保留）
    │
    └─ proxy_chat_completion_inner
        ├─ reorder_by_affinity 亲和排序（CacheAffinity：优先复用上次成功目标）
        └─ 三重循环（model → map → credential）
            ├─ cred.quota_exhausted = true → 跳过并记录原因
            ├─ circuit_breaker.is_allowed = false → 跳过并记录原因
            └─ forward_to_provider
                ├─ join_url: {base_url.trim_end('/')}{protocol.path()}
                ├─ ssrf_checker.validate_url_safe → 校验失败返回 Fail（不重试）
                ├─ body["model"] = map.model_id
                ├─ is_stream → body["stream"]=true；OpenAI 协议加 stream_options={"include_usage":true}
                ├─ 认证头：OpenAI/Responses: Bearer；Anthropic: x-api-key + anthropic-version 2023-06-01
                ├─ 超时 timeout = map.timeout
                └─ 结果分类：
                    ├─ 2xx → Success
                    ├─ 401 / 402 / 403 → CredentialFail
                    ├─ 429 且 is_quota_error → CredentialFail；否则 → Retry(retry_after)
                    ├─ 500 / 502 / 503 / 504 → Retry(retry_after)
                    ├─ 其他状态码 → Fail
                    └─ 网络错误 / 超时 → Retry(None)
                ├─ Success → handle_success（按 is_stream 分流）
                ├─ Retry → sleep retry_delay(retry_after) 后重试一次（同一凭证）
                │    ├─ 二次 Success → handle_success
                │    ├─ 二次 Retry / Fail → record_failure（更新熔断）+ error_details → continue
                │    └─ 二次 CredentialFail → record_credential_fail（record_failure + mark_quota_exhausted + warn + error_details）→ continue
                ├─ CredentialFail → record_credential_fail → continue
                └─ Fail → 记录 failed 日志 → 返回协议风格 400（"Upstream provider error: ..."）
        └─ 全部组合失败 → 记录 failed 日志 → 返回 503，聚合 error_details
```

### 调度数据结构

调度使用「模型 → 映射 → 凭证」三级嵌套结构，定义在 [model_scheduler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/model_scheduler.rs)：

```rust
pub struct ModelScheduleInfo {
    pub model_name: String,               // 模型对外名称
    pub model_config_id: i32,             // 模型配置 ID
    pub priority: i32,                    // 模型优先级（越小越先）
    pub maps: Vec<ModelProviderMap>,      // 该模型的映射列表（按 priority ASC）
}

pub struct ModelProviderMap {
    pub provider_model_map_id: i32,       // 映射 ID
    pub provider_config_id: i32,          // 供应商配置 ID
    pub provider_name: String,            // 供应商名称
    pub base_url: String,                 // 供应商基础 URL
    pub model_id: String,                 // 上游 API 实际使用的 model 值
    pub priority: i32,                    // 映射优先级（越小越先）
    pub timeout: u64,                     // 超时秒数（映射未设置时回退模型配置值）
    pub protocols: String,                // 支持的协议，逗号分隔（openai,anthropic,responses）
    pub context_length: i32,              // 上下文窗口（映射未设置时回退模型配置值）
    pub credentials: Vec<CredentialInfo>, // 该供应商下的活跃凭证（按 priority ASC）
}

pub struct CredentialInfo {
    pub provider_credential_id: i32,      // 凭证 ID
    pub api_key: String,                  // 解密后的 API Key
    pub priority: i32,                    // 凭证优先级（越小越先）
    pub quota_exhausted: bool,            // 配额是否已耗尽（耗尽则调度跳过）
}
```

`schedule_all_available` 按 `model_config.priority ASC` 装配模型，模型内 `maps` 按 `map.priority ASC`，映射内 `credentials` 按 `priority ASC`。三级均要求对应记录活跃；任一模型无可用映射时整体不可用（不进入结果列表）。

### 协议适配

`Protocol` 三协议枚举定义在 [proxy_types.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/proxy_types.rs)：

```rust
pub enum Protocol { OpenAI, Anthropic, Responses }

impl Protocol {
    pub fn path(&self) -> &'static str {
        match self {
            Protocol::OpenAI => "/chat/completions",
            Protocol::Anthropic => "/messages",
            Protocol::Responses => "/responses",
        }
    }
}
```

对应公共端点：`/v1/chat/completions`、`/v1/messages`、`/v1/responses`。

鉴权头（协议差异化，见 `forward_to_provider`）：

```rust
request = match protocol {
    Protocol::OpenAI => request.header("Authorization", format!("Bearer {}", cred.api_key)),
    Protocol::Anthropic => request
        .header("x-api-key", &cred.api_key)
        .header("anthropic-version", "2023-06-01"),
    Protocol::Responses => request.header("Authorization", format!("Bearer {}", cred.api_key)),
};
```

错误响应格式同样按协议区分：OpenAI / Responses 使用 OpenAI 错误格式，Anthropic 使用 Anthropic 错误格式（`Protocol::error_for_protocol`）。

### 流式响应处理

**SSE 流式转发** 使用 `tokio::sync::mpsc` 通道（容量 64）在后台任务中消费上游流式响应（`handle_stream_response`）：

```rust
let (tx, rx) = tokio::sync::mpsc::channel::<Result<web::Bytes, actix_web::Error>>(64);

actix_web::rt::spawn(async move {
    let mut upstream_stream = resp.bytes_stream();
    let mut scanner = SseScanner::new();          // 用量扫描
    let mut error_detector = SseScanner::new();   // 错误扫描

    while let Some(item) = upstream_stream.next().await {
        match item {
            Ok(bytes) => {
                // scan_usage_json：检测 usage（Responses 额外查 response.usage）
                if let Some(info) = scanner.push(&bytes, |json| scan_usage_json(json, protocol)) {
                    log_stream_usage(...);        // 记录用量日志
                }
                // scan_error_json：检测流内嵌 error
                if let Some((err_msg, is_quota)) = error_detector.push(&bytes, scan_error_json) {
                    handle_embedded_error(...).await;  // record_failure + 配额则 mark_quota_exhausted + failed 日志
                    break;                             // 中断转发，不再透传后续数据
                }
                if tx.send(Ok(bytes)).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    // 流结束时 finish() 兜底扫描用量与错误
});
```

要点：
- 后台任务使用两个 `SseScanner` 实例（tail 缓冲 + 按 `\n\n` 切分完整事件，只解析 `data: ` 行）分别扫描用量与错误，扫描命中后置 `done` 标记。
- 检测到内嵌错误 → 调用 `handle_embedded_error`（熔断 `record_failure`；若为配额错误则 `mark_quota_exhausted` 持久化 + failed 日志）并 **break 中断**，后续数据不再透传。
- **不伪造 `[DONE]` 结束帧**：SSE 数据原样透传，通道关闭即结束响应流，`[DONE]` 由上游数据自然携带。
- 响应头：`content-type: text/event-stream` + `append_upstream_headers` 透传上游头。hop-by-hop 头被过滤：`content-length`、`transfer-encoding`、`connection`、`keep-alive`、`proxy-authenticate`、`proxy-authorization`、`te`、`trailers`、`upgrade`、`x-accel-buffering`、`x-accel-limit-rate`、`x-accel-redirect`、`x-forwarded-for`、`x-forwarded-proto`（`content-type` 特判处理）。
- 进入流式处理前先 `record_success_metrics`：熔断 `record_success` + 亲和 `record_success`（有 api_key_id 时）。

**非流式响应**（`handle_non_stream_response`）：复制上游状态码 + `append_upstream_headers` 透传头（其返回值决定是否补 `application/json`）→ 读完整 body → `find_embedded_error`（复用 `extract_error_message`）命中则 `handle_embedded_error(is_stream=false)` 并原样透传 body；无错误则 `record_success_metrics` + 从 body 的 `usage` 字段按协议提取用量，记录成功日志，body 原样透传。

### 熔断与重试

`ForwardOutcome` 四态（`proxy_service.rs`）：

```rust
enum ForwardOutcome {
    Success(reqwest::Response),   // 2xx
    Retry(Option<u64>),           // 可重试，携带 retry-after 秒数
    Fail(String),                 // 不可重试（SSRF 校验失败 / 其他 4xx、5xx）
    CredentialFail(String),       // 凭证级失败（配额 / 401/402/403）
}
```

**重试策略**（`proxy_chat_completion_inner`）：
- `Retry`：等待 `retry_delay(retry_after)` 后**重试一次**（同一凭证）。`retry_after` 有值 → 秒数封顶 15 秒（`min(15)`）；无值 → `500 + random(0..=400)` 毫秒。
- 二次仍 `Retry`/`Fail` → `circuit_breaker.record_failure` + 记入 `error_details` → 换下一凭证（failed 用量日志在全部组合失败时统一写入）。
- `CredentialFail`（首次或重试后）→ `record_credential_fail`：`record_failure` + 异步 `mark_quota_exhausted`（持久化配额耗尽）+ warn + error_details → 换下一凭证。
- `Fail`：不重试，记录 failed 日志并立即返回协议风格 400。
- 全部组合失败 → 返回 503，`error_details` 聚合各失败原因（含配额耗尽 / 熔断跳过的原因）。

**熔断器**（[penalty.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/penalty.rs) 的 `CircuitBreaker`）按「模型 × 供应商 × 凭证」维度（key：`model|provider|credential_id`）维护状态机：

```
Closed ──(failure_count ≥ threshold，默认 3)──► Open
  ▲                                              │
  └──── record_success 复位 ◄── HalfOpen ◄──(open_ttl 到期)┘
                                  │
                   半开时仅放行一次探测（half_open_allowed 置 false）
```

- `is_allowed`：Open 未到期 → 拒绝（三重循环中跳过并记录原因）；Open 到期 → 转 HalfOpen 放行一次探测；HalfOpen 已探测 → 拒绝。
- `record_failure`：失败计数 +1，达到阈值置 Open 并记录 `open_ttl` 过期时间。
- `record_success`：清零计数并复位 Closed。
- `reset_credential(credential_id)`：清除某凭证的全部熔断条目（管理端"重置凭证状态"使用）。
- `list_blocked_models`：列出当前熔断中的目标及剩余秒数（服务状态接口使用）。
- 实现注意：moka get-then-insert 非原子，并发下 failure_count 可能少计、HalfOpen 可能多发探测（代码注释明确该行为可接受）。

### 响应透明化

**不伪造结束帧**：流式响应由 `handle_stream_response` 将上游 SSE 数据原样透传，服务端不生成、不追加 `data: [DONE]`，也不构造 Anthropic 错误关闭帧；流是否结束完全由上游数据流与通道关闭决定。

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

1. **上下文窗口校验：** 转发请求前，估算 prompt 的 token 数，与模型的 `context_length` 对比，过滤掉窗口不足的模型（模型级判断：任一映射窗口足够即保留该模型）
2. **用量日志记录：** 请求完成后，从上游响应中读取实际的 usage 数据，通过 `UsageLogCollector` 异步写入

### 验证超长 Prompt

```rust
// 在 chat_handler.rs 中
let prompt_texts = extract_prompt_text(&body, is_responses);
let estimated_prompt = prompt_texts.iter()
    .map(|s| tokenizer::estimate_prompt_tokens(s))
    .sum::<usize>() as i32;

let models = filter_by_context_window(models, estimated_prompt);

if models.is_empty() {
    return protocol.bad_request(
        &format!("Prompt too long (estimated {} tokens), exceeds all available models' context window", estimated_prompt)
    );
}
```

prompt 提取规则（`extract_prompt_text`）：Responses 取 `input`（字符串直接取；数组取每项 `text`，回退 `content`）；OpenAI / Anthropic 取 `messages[].content`。

### 用量日志写入

用量日志的解析在 [proxy_types.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/proxy_types.rs) 的 `Protocol::extract_usage` 方法中：

| 指标 | OpenAI 字段路径 | Anthropic 字段路径 | Responses 字段路径 |
|------|----------------|-------------------|-------------------|
| prompt_tokens | `usage.prompt_tokens` | `usage.input_tokens` | `usage.input_tokens` |
| completion_tokens | `usage.completion_tokens` | `usage.output_tokens` | `usage.output_tokens` |
| total_tokens | `usage.total_tokens` | `input_tokens + output_tokens` | `usage.total_tokens` |
| cache_hit_tokens | `usage.prompt_tokens_details.cached_tokens`（回退 `usage.prompt_cache_hit_tokens`） | `usage.cache_read_input_tokens` | `usage.input_tokens_details.cached_tokens` |
| cache_miss_tokens | `prompt_tokens − cache_hit_tokens` | `input_tokens` | `input_tokens − cache_hit_tokens` |

日志写入使用 [usage_log_collector.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/usage_log_collector.rs) 的 `spawn_usage_log` 在后台执行，不阻塞主请求流程：
- 流式请求：SSE 流中检测到 `usage` 字段时提交（`SseScanner` 扫描，流结束 `finish()` 兜底）
- 非流式请求：从上游响应解析后提交
- 收集器（`UsageLogCollector`，mpsc 容量 256）批量写入：**满 10 条**或**每 30 秒** flush，优雅关停时最后 flush；通道满则丢记录并 warn

### 代码位置

| 功能 | 文件 |
|------|------|
| Token 估算 | [tokenizer.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/tokenizer.rs) |
| 模型调度 | [model_scheduler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/model_scheduler.rs) |
| 转发核心 | [proxy_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/proxy_service.rs) |
| SSE 扫描 | [stream_usage_scanner.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/stream_usage_scanner.rs) |
| 协议与用量类型 | [proxy_types.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/proxy_types.rs) |
| 熔断状态机 | [penalty.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/penalty.rs) |
| 上下文校验 | [chat_handler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/handler/chat_handler.rs) |
| 用量日志收集 | [usage_log_collector.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/usage_log_collector.rs) |
