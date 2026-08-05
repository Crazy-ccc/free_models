# 数据库表结构文档

系统使用 MySQL 数据库，共 7 张业务表（provider_config、model_config、provider_model_map、api_key、admin_key、provider_credential、usage_log），另有 1 张日汇总表 usage_log_daily，全部通过 Sea-ORM 2 进行访问。以下为各表的完整字段说明。

---

## provider_config（供应商配置）

存储上游 LLM 供应商的基本信息。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| name | VARCHAR(255) | NOT NULL | 供应商名称 |
| base_url | VARCHAR(512) | NOT NULL | 供应商 API 基础 URL |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**用途：** 定义上游供应商接入点，如 OpenAI (`https://api.openai.com/v1`)、Anthropic (`https://api.anthropic.com/v1`) 或其他兼容代理。

**关联关系：**
- 一个 provider 有多个 provider_model_map（模型映射）
- 一个 provider 有多个 provider_credential（凭证）

---

## model_config（模型配置）

存储模型的基本定义。模型与供应商的关联关系在 `provider_model_map` 表中描述。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| name | VARCHAR(255) | NOT NULL | 模型对外名称，用于 API 请求中的 `model` 字段匹配 |
| priority | INT | NOT NULL DEFAULT 0 | 全局默认优先级 |
| is_active | BOOLEAN | NOT NULL DEFAULT TRUE | 是否启用 |
| timeout | INT | NOT NULL DEFAULT 30 | 默认请求超时时间（秒） |
| context_length | INT | NOT NULL DEFAULT 256000 | 默认上下文窗口大小（token 数） |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**注意：** `model_config` 表中不存储与供应商相关的字段（provider_id、protocols、status 等），这些信息已移到 `provider_model_map` 表中。

**上下文窗口校验：** 转发请求前会估算 prompt token 数，与 `context_length` 对比，过滤掉窗口不足的模型。

---

## provider_model_map（供应商模型映射）

描述模型与供应商的多对多关系，是当前架构中最核心的表之一。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| model_id | INT | NOT NULL, FOREIGN KEY → model_config(id) ON DELETE CASCADE | 关联模型 ID |
| provider_id | INT | NOT NULL, FOREIGN KEY → provider_config(id) ON DELETE CASCADE | 关联供应商 ID |
| provider_model_id | VARCHAR(255) | NOT NULL | 模型在目标供应商处的映射 ID（上游 API 的 model 值） |
| is_active | BOOLEAN | NOT NULL DEFAULT TRUE | 是否启用 |
| priority | INT | NOT NULL DEFAULT 0 | 在该供应商处的优先级，数值越小越优先转发 |
| context_length | INT | NULL | 可选，不为空时覆盖 model_config.context_length |
| protocols | VARCHAR(255) | NOT NULL DEFAULT 'openai' | 支持的协议（`openai` / `anthropic` / `responses`），逗号分隔 |
| status | VARCHAR(16) | NOT NULL DEFAULT 'available' | 状态：`available` / `unavailable` / `deprecated` |
| timeout | INT | NULL | 可选超时（秒），不为空时覆盖 model_config.timeout |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**索引：**
| 索引名 | 字段 | 用途 |
|--------|------|------|
| uk_model_provider | model_id, provider_id | UNIQUE 约束，防止重复映射 |
| idx_model_id | model_id | 按模型查询供应商映射 |
| idx_provider_id | provider_id | 按供应商查询模型映射 |
| idx_is_active | is_active | 筛选活跃映射 |
| idx_status | status | 按状态筛选 |

**用途：**
- 实现模型与供应商的多对多关系：一个模型可被多个供应商提供，一个供应商可提供多个模型
- `provider_model_id` 支持模型名与上游 API 实际 model 值的分离
- 各供应商可单独配置优先级、协议、上下文窗口和超时

---

## api_key（API 密钥）

存储调用代理服务的客户端密钥。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| key_value | VARCHAR(512) | NOT NULL, UNIQUE | API Key 值，格式为 `fm-` + 64 位字母数字 |
| name | VARCHAR(255) | NOT NULL | 密钥名称/备注 |
| is_active | BOOLEAN | NOT NULL DEFAULT TRUE | 是否启用 |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**用途：** 客户端调用 `/v1/chat/completions`、`/v1/messages`、`/v1/responses` 时使用 Bearer Token 鉴权。

**自动生成：** 创建时不传 `key_value` 时，系统自动生成 `fm-` 开头 + 64 位随机字母数字。

**缓存：** 所有 `is_active = true` 的密钥在服务启动时全量加载到 Redis Set 和内存中，鉴权时通过缓存 O(1) 查证。

---

## admin_key（管理员密钥）

存储用于 Admin API 签名鉴权的 SSH Ed25519 公钥。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| name | VARCHAR(255) | NOT NULL DEFAULT '' | 密钥名称/备注 |
| public_key | TEXT | NOT NULL | OpenSSH 格式的 Ed25519 公钥 |
| fingerprint | VARCHAR(64) | NULL UNIQUE | 公钥的 SHA256 指纹（`SHA256:xxxx` 格式） |
| is_active | BOOLEAN | NOT NULL DEFAULT TRUE | 是否启用 |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**用途：** 管理员通过 Ed25519 签名访问 Admin API 时，服务端根据 `X-Admin-Fingerprint` 头查询此表获取公钥进行验签（`AdminKeyStoreSeaorm::find_active_by_fingerprint`，仅匹配 `is_active = true` 的行）。

**指纹格式：** Ed25519 raw 公钥（32 字节）→ SHA256 哈希 → Base64 无填充编码 → 添加 `SHA256:` 前缀，与请求头 `X-Admin-Fingerprint` 一一对应。

---

## provider_credential（供应商凭证）

存储每个供应商的 API 凭证，一个供应商可有多组凭证用于负载均衡或故障切换。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| provider_id | INT | NOT NULL, FOREIGN KEY → provider_config(id) | 关联供应商 ID |
| name | VARCHAR(255) | NOT NULL DEFAULT '' | 凭证名称/备注 |
| api_key | VARCHAR(512) | NOT NULL | AES-256-GCM 加密后的 API Key |
| account | VARCHAR(255) | NULL | 账号（部分供应商需要，明文存储） |
| encrypted_password | VARCHAR(512) | NULL | AES-256-GCM 加密后的密码 |
| priority | INT | NOT NULL DEFAULT 0 | 优先级，数值越小越优先使用 |
| is_active | BOOLEAN | NOT NULL DEFAULT TRUE | 是否启用 |
| quota_exhausted | BOOLEAN | NOT NULL DEFAULT FALSE | 是否因配额耗尽被标记（006 迁移新增），为 true 时调度会跳过该凭证 |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**索引：** `idx_provider_id`（provider_id）、`idx_is_active`（is_active）

**用途：** 将凭证从供应商中剥离，允许多组凭证隶属同一供应商。转发请求时按 `priority` 升序选取第一个活跃凭证。

**加密说明：**
- `api_key` 和 `encrypted_password` 使用 AES-256-GCM 加密
- 加密密钥来自环境变量 `ENCRYPTION_KEY`（64 位 hex = 32 字节）
- 每次加密生成随机 12 字节 nonce，以 base64(nonce + ciphertext) 格式存储
- Admin API 返回时自动解密，`api_key` 以掩码（前 4 位 + `****`）返回，`password` 一律返回 `****`

**配额持久化（quota_exhausted）：**
- 上游返回凭证/配额类错误（如 401 / 429 配额不足）时，`proxy_service` 会调用 `mark_quota_exhausted` 将该凭证标记为 `quota_exhausted = true` 并持久化到数据库，服务重启后依然生效
- 调度转发时（`proxy_service` 内层循环）会跳过 `quota_exhausted = true` 的凭证，不发起请求
- 重置方式：
  - 调用 `POST /admin/provider_credentials/{id}/reset_status`（同时调用熔断器 `reset_credential` 清除惩罚记录）
  - 更新该凭证的 `api_key` 时自动清除（`ProviderCredentialStoreSeaorm::update` 中 `new_api_key` 非空即重置 `quota_exhausted = false`）
  - 凭证连通性测试成功（`/admin/test_credential`）时也会自动清除

---

## usage_log（用量日志）

记录每次模型调用的详细使用数据。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | BIGINT | AUTO_INCREMENT PRIMARY KEY | 主键（使用 BIGINT 应对大量日志） |
| api_key_id | INT | NULL | 调用方 API Key ID |
| api_key_name | VARCHAR(255) | NULL | 调用方 API Key 名称 |
| model_config_id | INT | NULL | 模型配置 ID |
| provider_config_id | INT | NULL | 供应商 ID |
| provider_credential_id | INT | NULL | 使用的凭证 ID |
| model_name | VARCHAR(255) | NOT NULL | 模型名称 |
| provider_name | VARCHAR(255) | NOT NULL | 供应商名称 |
| protocol | VARCHAR(32) | NOT NULL DEFAULT 'openai' | 协议类型（`openai` / `anthropic` / `responses`） |
| status | VARCHAR(16) | NOT NULL DEFAULT 'success' | 请求状态（`success` / `failed`） |
| error_message | TEXT | NULL | 错误信息（失败时记录） |
| prompt_tokens | INT | NOT NULL DEFAULT 0 | prompt token 数 |
| completion_tokens | INT | NOT NULL DEFAULT 0 | 生成 token 数 |
| total_tokens | INT | NOT NULL DEFAULT 0 | 总 token 数 |
| cache_hit_tokens | INT | NOT NULL DEFAULT 0 | 缓存命中的 token 数（各协议的解析来源见下方表格） |
| cache_miss_tokens | INT | NOT NULL DEFAULT 0 | 缓存未命中的 token 数 |
| duration_ms | INT | NOT NULL DEFAULT 0 | 请求耗时（毫秒） |
| is_stream | BOOLEAN | NOT NULL DEFAULT FALSE | 是否为流式请求 |
| request_timestamp | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 请求时间戳 |

**索引：**
| 索引名 | 字段 | 用途 |
|--------|------|------|
| idx_api_key_id | api_key_id | 按 API Key 查询用量 |
| idx_model_config_id | model_config_id | 按模型查询用量 |
| idx_provider_config_id | provider_config_id | 按供应商查询用量 |
| idx_provider_credential_id | provider_credential_id | 按凭证查询用量 |
| idx_status | status | 按状态筛选 |
| idx_model_provider | model_name, provider_name | 按模型+供应商联合查询 |
| idx_request_timestamp | request_timestamp | 按时间范围查询 |

**写入时机：**
- **非流式请求：** 从上游响应 JSON 的 `usage` 字段解析后立即写入
- **流式请求：** 在 SSE 流中检测到 `usage` 字段时写入（OpenAI 需设置 `stream_options: {"include_usage": true}`）
- **全部失败：** 所有供应商均失败时记录一条状态为 `failed` 的日志

**Protocol 用量解析差异：**

| 字段 | OpenAI | Anthropic | Responses |
|------|--------|-----------|-----------|
| prompt_tokens | `usage.prompt_tokens` | `usage.input_tokens` | `usage.input_tokens` |
| completion_tokens | `usage.completion_tokens` | `usage.output_tokens` | `usage.output_tokens` |
| total_tokens | `usage.total_tokens` | `input_tokens + output_tokens` | `usage.total_tokens` |
| cache_hit_tokens | `usage.prompt_tokens_details.cached_tokens`（fallback `usage.prompt_cache_hit_tokens`） | `usage.cache_read_input_tokens` | `usage.input_tokens_details.cached_tokens` |
| cache_miss_tokens | `prompt_tokens - cached_tokens` | `input_tokens` | `input_tokens - cached_tokens` |

---

## usage_log_daily（用量日汇总）

每日凌晨自动汇总前一天的用量数据，用于快速查询统计，避免扫描全量 `usage_log` 表。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | BIGINT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| stat_date | DATE | NOT NULL | 统计日期 |
| api_key_id | INT | NULL | API Key ID |
| api_key_name | VARCHAR(255) | NULL | API Key 名称 |
| provider_config_id | INT | NULL | 供应商 ID |
| provider_credential_id | INT | NULL | 使用的凭证 ID（005 迁移新增） |
| provider_name | VARCHAR(255) | NOT NULL | 供应商名称 |
| model_config_id | INT | NULL | 模型配置 ID |
| model_name | VARCHAR(255) | NOT NULL | 模型名称 |
| requests | INT | NOT NULL DEFAULT 0 | 请求次数 |
| prompt_tokens | BIGINT | NOT NULL DEFAULT 0 | Prompt token 总计 |
| completion_tokens | BIGINT | NOT NULL DEFAULT 0 | Completion token 总计 |
| total_tokens | BIGINT | NOT NULL DEFAULT 0 | 总 token 数 |
| cache_hit_tokens | BIGINT | NOT NULL DEFAULT 0 | 缓存命中 token 总计 |
| cache_miss_tokens | BIGINT | NOT NULL DEFAULT 0 | 缓存未命中 token 总计 |
| avg_duration_ms | INT | NOT NULL DEFAULT 0 | 平均耗时 |
| min_duration_ms | INT | NOT NULL DEFAULT 0 | 最小耗时 |
| max_duration_ms | INT | NOT NULL DEFAULT 0 | 最大耗时 |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |

**索引：**
| 索引名 | 字段 | 用途 |
|--------|------|------|
| uk_daily | stat_date, api_key_id, provider_config_id, model_config_id | UNIQUE 约束，每日去重 |
| idx_stat_date | stat_date | 按日期查询 |
| idx_provider_config_id | provider_config_id | 按供应商查询 |
| idx_provider_credential_id | provider_credential_id | 按凭证查询（005 迁移新增） |
| idx_model_config_id | model_config_id | 按模型查询 |
| idx_api_key_id | api_key_id | 按 API Key 查询 |

**归档逻辑：** 由后台定时任务每日凌晨 00:05（UTC）执行 `UsageLogStoreSeaorm::archive_yesterday`（`src/db/impls/usage_log.rs`，由 `task.rs` 的 `spawn_daily_archive` 调度，失败时指数退避重试，上限 1 小时）。归档时从 `usage_log` 表按 `(stat_date, api_key_id, api_key_name, provider_config_id, provider_credential_id, provider_name, model_config_id, model_name)` 分组汇总后插入 `usage_log_daily`，随后删除对应日期的原始明细。统计查询（`/admin/usage_log/stats`）将 `usage_log_daily`（历史）+ `usage_log`（当日）合并聚合返回。

---

## 迁移文件清单

迁移为 `migrations/` 目录下的手动 SQL 文件，需按顺序执行（`mysql -u root -p free_models < migrations/NNN_xxx.sql`）：

| 文件 | 内容 |
|------|------|
| 001_init.sql | 创建 `provider_config`、`model_config`（含 provider_id、model_id、protocols、status、priority 字段）、`api_key` 三张表 |
| 002_add_model_config_columns.sql | `model_config` 新增 `model_id`、`timeout`、`protocols` 字段 |
| 003_add_admin_key_and_usage.sql | 创建 `admin_key`、`usage_log`、`provider_credential` 三张表；`model_config` 新增 `context_length` 字段 |
| 004_create_usage_log_daily.sql | 创建 `usage_log_daily` 日汇总表 |
| 005_create_provider_model_map.sql | 创建 `provider_model_map` 映射表；将 `model_config` 中的 provider_id / model_id / protocols / status / context_length / timeout 迁移到映射表；从 `model_config` 删除 provider_id、protocols、status、model_id 并新增 `is_active`；`usage_log_daily` 新增 `provider_credential_id` 字段及索引 |
| 006_add_credential_status_fields.sql | `provider_credential` 新增 `quota_exhausted BOOLEAN NOT NULL DEFAULT FALSE` 字段 |
