# 模型代理全链路梳理

本文档梳理 `free_models_server` 从请求进入到响应返回的完整代理链路，覆盖鉴权、校验、调度、转发、错误处理与用量记录各环节。所有描述与当前源码一致（主要涉及 `chat_handler.rs`、`proxy_service.rs`、`model_scheduler.rs`、`penalty.rs`、`cache_affinity.rs`、`stream_usage_scanner.rs`、`proxy_types.rs`、`usage_log_collector.rs`、`proxy_ssrf.rs`）。

## 全链路总览

```
客户端请求 (Bearer Token)
    │
    ▼
GET /v1/models                    POST /v1/chat/completions | /v1/messages | /v1/responses
    │                                      │
    │                                      ├─ resolve_api_key      ① 鉴权：Bearer → api_key 表
    │                                      ├─ validate_request_body ② 校验（Responses 查 input，其余查 messages）
    │                                      ├─ schedule_all_available ③ 调度：Redis+moka 缓存 → 批量查活跃配置 → 解密 key
    │                                      ├─ supports_protocol    ④ 协议过滤
    │                                      ├─ merge_preferred_first ⑤ 请求 model 置前
    │                                      ├─ estimate_prompt_tokens ⑥ token 估算
    │                                      ├─ filter_by_context_window ⑥ 上下文窗口过滤
    │                                      │
    │                                      ▼
    │                          proxy_chat_completion_inner
    │                                      │
    │                                      ├─ reorder_by_affinity  ⑦ 亲和排序（复用上次成功目标）
    │                                      ▼
    │                        ┌──────────────┴───────────────┐
    │                        ▼                             ▼
    │              circuit_breaker.is_allowed    cred.quota_exhausted
    │                        │                    （跳过并记录原因）
    │                        ▼
    │                   forward_to_provider  ⑧ 转发：join_url + SSRF 校验 + 协议头 + 超时
    │                        │
    │                        ▼
    │              ┌─ Success ──────────────► handle_success  ⑩ 流式/非流式透传
    │              │                            │
    │              ├─ Retry ── retry_delay ──► 重试一次 ──► 失败则 record_failure，换下一凭证
    │              ├─ CredentialFail ──► record_failure + mark_quota_exhausted，换下一凭证
    │              └─ Fail ──► 记录失败日志，返回 400
    │
    └─ 全部失败 → 返回 503，聚合各错误原因
```

各环节均通过 `spawn_usage_log` 向后台队列写入 `usage_log`（⑪），配额耗尽状态持久化到凭证表（⑫）。

---

## ① 请求入口与鉴权

**职责**：识别协议入口、解析 Bearer Token 并映射为 API Key 上下文。

**入口函数**（`free_models_server/src/handler/chat_handler.rs`）：

| 端点 | 函数 | 协议 |
|------|------|------|
| `GET /v1/models` | `list_models` | 返回启用模型列表（OpenAI 格式），不依赖 API Key |
| `POST /v1/chat/completions` | `chat_completions` | `Protocol::OpenAI` |
| `POST /v1/messages` | `anthropic_messages` | `Protocol::Anthropic` |
| `POST /v1/responses` | `responses_messages` | `Protocol::Responses` |

**鉴权流程**：`dispatch_chat` → `resolve_api_key` → `extract_bearer_token`（读取 `Authorization` 头，`strip_prefix("Bearer ")`）→ `ApiKeyStoreSeaorm::find_by_key_value` 查询 `api_key` 表。

**关键决策点**：
- 鉴权失败**不拦截请求**，仅返回 `(None, None)` 上下文并记 warn；API Key 缺失/无效只影响日志归属与亲和排序（无 `api_key_id` 时亲和性不生效）。
- 返回的 `ApiKeyContext { id, name }` 贯穿转发过程，用于 `usage_log` 记录与 `CacheAffinity` 键。

---

## ② 请求校验

**职责**：按协议校验请求体必填字段，返回协议风格的 400 错误。

**入口**：`validate_request_body(&body, protocol)`（`chat_handler.rs`）。

| 协议 | 校验字段 | 规则 |
|------|---------|------|
| Responses | `input` | 为**非空字符串**或**非空数组** |
| OpenAI / Anthropic | `messages` | 为非空数组 |

**关键决策点**：错误响应由 `protocol.bad_request(...)` 生成——OpenAI/Responses 走 `openai_error`（含 `invalid_request_error`），Anthropic 走 `anthropic_bad_request`，保证错误格式与协议匹配。

---

## ③ 模型调度

**职责**：从数据库装配「模型 → 映射 → 凭证」三层可用集合，供后续遍历尝试。

**入口**：`schedule_all_available(&database, &encryption_key, &scheduler_cache)`（`model_scheduler.rs`）。

**缓存**（`SchedulerCache`）：Redis 键 `app:free_models:scheduler:all` + moka 本地二级缓存，TTL 可配置；Redis 不可用时透明降级到 moka。命中直接返回。

**装配流程**：
1. 批量查询：活跃 `model_config`（`list_all_active_ordered`，按 priority ASC）、全部映射 `provider_model_map`（`list_filtered`）、涉及的 `provider_config`（`list_by_ids`）、这些供应商下的活跃凭证 `provider_credential`（`list_active_by_provider_ids`）。
2. 按 `model_id` 分组映射，逐模型调用 `build_model_info`：
   - `resolve_providers`：跳过无对应 provider 的映射；
   - `resolve_credentials`：按 provider 分组凭证；
   - 解密每个凭证的 API Key（`encryption::decrypt`，AES-256-GCM）；
   - 组装 `ModelScheduleInfo` / `ModelProviderMap` / `CredentialInfo`。

**关键决策点**：
- 三层排序均按 priority ASC（模型 < 映射 < 凭证），priority 小者先尝试。
- 任一模型无可用映射 → 返回 `None`（该模型整体不可用，不进入结果列表）。
- `timeout`、`context_length` 取映射值，为空时回退到模型配置值。
- 解密失败按 `StoreError` 处理。

---

## ④ 协议过滤与优先级合并

**职责**：按请求协议过滤模型，并把请求指定的模型排到最前。

**入口**：`supports_protocol`（`model_scheduler.rs`）、`merge_preferred_first`（`chat_handler.rs`）。

- `ModelScheduleInfo::supports_protocol`：任一映射的 `protocols`（逗号分隔，trim 后）包含协议字符串即支持。协议字符串：`openai` / `anthropic` / `responses`。
- 过滤后为空 → `service_unavailable("No available models")`。
- `merge_preferred_first`：`partition` 出 `model_name == 请求 model` 的项置于列表头部，其余保持原顺序。

---

## ⑤ Token 估算与上下文窗口过滤

**职责**：转发前估算 prompt token 数，过滤掉窗口不足的模型。

**入口**：`extract_prompt_text`、`filter_by_context_window`（`chat_handler.rs`）、`estimate_prompt_tokens`（`tokenizer.rs`，tiktoken-rs `cl100k_base`，`OnceLock` 单例）。

**提取规则**：
- Responses：`input` 为字符串直接取；为数组时取每项 `text`（回退 `content`）字段。
- OpenAI / Anthropic：`messages[].content` 字符串。

**过滤**：`estimated_prompt = Σ estimate_prompt_tokens(text)`，模型保留条件为**任一映射**的 `context_length >= estimated_prompt`（模型级判断）。全部被过滤 → `bad_request("Prompt too long (estimated N tokens)...")`。

---

## ⑥ 亲和排序

**职责**：优先复用该 API Key 此前成功转发的目标（provider + 凭证），减少切换开销。

**入口**：`reorder_by_affinity(models, api_key_id, &cache_affinity)`（`proxy_service.rs`）。

**数据结构**：`CacheAffinity`（`cache_affinity.rs`）——moka 同步缓存，键 `(api_key_id, model_name)`，值为上次成功的 `CacheEntry { provider_name, provider_credential_id }`。

**排序逻辑**：
- 无 `api_key_id` 或空列表 → 原样返回。
- **命中亲和记录**：每个模型的 `maps` 重排为「含命中凭证的映射 → 同 provider 其余映射 → 其余映射」。
- **未命中**：对每个映射的 `credentials` 按 `count_by_credential`（当前有多少 API Key 关联到该凭证）升序排序，实现凭证级负载均衡。

---

## ⑦ 熔断检查

**职责**：对「模型 × 供应商 × 凭证」维度的连续失败进行熔断，避免把流量持续打向故障目标。

**入口**：`CircuitBreaker`（`penalty.rs`），方法 `is_allowed` / `record_failure` / `record_success` / `reset_credential` / `list_blocked_models`。

**状态机**：

```
Closed ──(failure_count ≥ threshold，threshold 默认 3)──► Open
  ▲                                                          │
  └──── record_success 复位 ◄──── HalfOpen ◄──(open_ttl 到期)┘
                                   │
                          半开时允许单探测（half_open_allowed 置 false）
```

- **key**：`format!("{}|{}|{}", model_name, provider_name, credential_id)`（moka 缓存，容量与 TTL 可配）。
- 三重循环内先查 `is_allowed`：Open 且未到 TTL → 跳过（记入 `error_details`）；Open 到期 → 转 HalfOpen 放行一次探测；HalfOpen 已探测 → 拒绝。
- 成功后 `record_success` 归零并复位 Closed。
- 实现注意：moka get-then-insert 非原子，并发下 failure_count 可能少计、HalfOpen 可能多发探测（代码注释明确该行为可接受）。

---

## ⑧ 转发

**职责**：向选定供应商发出实际请求，携带对应凭证与协议头。

**入口**：`forward_to_provider(client, map, cred, body, is_stream, protocol, ssrf_checker)`（`proxy_service.rs`）。

**关键决策点**：
- **URL 拼接**：`join_url(base_url, protocol.path())`——`base_url.trim_end_matches('/')` + 路径；三协议路径为 `/chat/completions`、`/messages`、`/responses`。
- **SSRF 防护**：`ssrf_checker.validate_url_safe(&url)`（`proxy_ssrf.rs`）——仅允许 http/https，DNS 解析后拦截回环/私网/链路本地/未指定/广播地址，DNS 解析失败 fail-closed；按 host 做 moka 缓存；校验失败对代理转发直接返回 `ForwardOutcome::Fail`（不泄露内网 IP）。
- **请求体改写**：`body["model"]` 替换为上游 `map.model_id`；流式时强制 `stream=true`，OpenAI 额外注入 `stream_options: {"include_usage": true}`（保证 usage 随 SSE 返回）。
- **鉴权头（协议差异化）**：
  - OpenAI / Responses：`Authorization: Bearer {api_key}`
  - Anthropic：`x-api-key: {api_key}` + `anthropic-version: 2023-06-01`
- 统一 `Content-Type: application/json`，超时取 `map.timeout`（秒）。

---

## ⑨ 错误分类与重试

**职责**：对上游响应/网络错误分级，决定重试、降级或终止。

**数据结构**：`ForwardOutcome` 四态——`Success(reqwest::Response)`、`Retry(Option<u64>)`（可选 retry-after 秒数）、`Fail(String)`、`CredentialFail(String)`。

**分类规则**（HTTP 状态码）：

| 状态 | 分类 |
|------|------|
| 2xx | `Success` |
| 401 / 402 / 403 | `CredentialFail` |
| 429 | 命中 `is_quota_error`（insufficient_quota / insufficient_credits / insufficient_balance / out of credits / exceeded your current quota / billing）→ `CredentialFail`，否则 `Retry` |
| 500 / 502 / 503 / 504 | `Retry` |
| 其余 4xx/5xx | `Fail` |
| 网络错误 / 超时 | `Retry(None)` |

**重试策略**（`proxy_chat_completion_inner` 三重循环 model → map → cred）：
- `Retry`：`retry_delay` 等待——解析 `retry-after` 头（秒数或 RFC2822 日期，**封顶 15 秒**）；无则 500–900ms 随机退避。随后**再试一次**（同一凭证），二次仍 Retry/Fail → `record_failure` 更新熔断 + 换下一凭证；二次 CredentialFail → `record_credential_fail`（`record_failure` + `mark_quota_exhausted`）+ 换下一凭证。
- `CredentialFail`（首次）：`record_credential_fail`（同左），换下一凭证。
- `Fail`：不重试，记录 failed 日志并立即返回 `bad_request("Upstream provider error: ...")`。
- 全部组合失败：返回 `service_unavailable`，`error_details` 聚合各失败原因（含配额耗尽/熔断跳过）。

---

## ⑩ 成功响应处理

**职责**：将上游 2xx 响应透传给客户端，并提取用量。

**入口**：`handle_success` → 按 `is_stream` 分流 `handle_stream_response` / `handle_non_stream_response`。

### 流式（SSE）

- 先 `record_success_metrics`：熔断 `record_success` + 亲和 `record_success`（有 api_key_id 时）。
- 响应头：`content-type: text/event-stream` + `append_upstream_headers` 透传（跳过多跳头 hop-by-hop：`content-length`、`transfer-encoding`、`connection` 等，`content-type` 特判）。
- 经 `mpsc` 通道（容量 64）+ 后台任务消费 `bytes_stream`：
  - `SseScanner`（用量）：按 `\n\n` 切分事件，扫描 `data:` JSON 中 `usage` 字段（Responses 额外查 `response.usage`），提取到后标记 `done` 并写日志；流结束时 `finish()` 兜底扫描。
  - `SseScanner`（错误，同一骨架）：检测流内 `error` 字段 → `handle_embedded_error`（`record_failure` + 配额则 `mark_quota_exhausted` + failed 日志），检测到即**中断转发**（break，不再透传后续数据）。
  - 通道关闭即结束响应流。

### 非流式

- 复制上游状态码 + `append_upstream_headers` 透传头。
- 读完整 body 后 `find_embedded_error`：body 内 `error` 非 null → `extract_error_message`（依次取 `message` / `type` / `code`）。命中则调用 `handle_embedded_error`（`record_failure` + 配额则 `mark_quota_exhausted` + failed 日志），并**原样透传 body** 返回。
- 无内嵌错误：`record_success_metrics`；从 body JSON 的 `usage` 字段按协议提取用量（`Protocol::extract_usage`），写 success 日志；上游无 `content-type` 时补 `application/json`，body 原样透传。

---

## ⑪ 用量日志收集

**职责**：将每次转发（成功/失败）的用量与元信息异步批量落库，不阻塞请求。

**入口**：`spawn_usage_log(model_info, map, cred, api_key_ctx, &log_ctx)`（`usage_log_collector.rs`）。

**链路**：`spawn_usage_log` 构建 `UsageLogInsert`（api_key_id/name、model_config_id、provider_config_id、provider_credential_id、model/provider 名、protocol、status、error_message、token 五项、duration_ms、is_stream）→ 全局 `USAGE_LOG_SENDER`（`OnceLock<mpsc::Sender>`）`try_send` 入队（通道满则丢记录并 warn）→ `UsageLogCollector::run_collector` 批量写入：**满 10 条**或**每 30 秒** flush，优雅关停时最后 flush。

**Token 按协议提取**（`Protocol::extract_usage`）：

| 指标 | OpenAI | Anthropic | Responses |
|------|--------|-----------|-----------|
| prompt | `usage.prompt_tokens` | `usage.input_tokens` | `usage.input_tokens` |
| completion | `usage.completion_tokens` | `usage.output_tokens` | `usage.output_tokens` |
| total | `usage.total_tokens` | input + output | `usage.total_tokens` |
| cache_hit | `prompt_tokens_details.cached_tokens`（回退 `prompt_cache_hit_tokens`） | `cache_read_input_tokens` | `input_tokens_details.cached_tokens` |
| cache_miss | prompt − cached | input | input − cached |

---

## ⑫ 配额持久化

**职责**：识别凭证额度耗尽并持久化，之后调度自动跳过，直至管理端重置。

**写入**（`ProviderCredentialStoreSeaorm::mark_quota_exhausted`，置 `quota_exhausted = true`），三个触发点：
- 流式响应内嵌配额错误（`handle_stream_embedded_error`）；
- 非流式 body 内嵌配额错误（`handle_non_stream_response`）；
- 凭证级失败 `CredentialFail`（含 429 配额、401/402/403，重试后仍失败——`record_credential_fail`）。

**跳过**：`schedule_all_available` 装配的 `CredentialInfo.quota_exhausted` 为 true 时，三重循环直接 `continue`（记入 `error_details`「quota exhausted」）。

**重置**：
- Admin 端点 `reset_provider_credential_status`（`admin/provider_credential.rs`）：`clear_quota_exhausted` + `priority_penalty.reset_credential(id)`（清除该凭证全部熔断条目）；
- 凭证测试成功时自动 `clear_quota_exhausted`（`admin/test_credential.rs`）。

**注意**：调度结果缓存在 Redis + moka（`SchedulerCache`），配额标记后缓存内的 `quota_exhausted` 会在 TTL 内滞后；管理端可调用 `scheduler_cache.clear()`（`admin/stats.rs`）主动刷新。
