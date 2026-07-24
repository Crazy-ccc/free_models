# 数据库表结构文档

系统使用 MySQL 数据库，共 6 张表，通过 Sea-ORM 2 进行访问。以下为各表的完整字段说明。

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
- 一个 provider 有多个 model_config（模型）
- 一个 provider 有多个 provider_credential（凭证）

---

## model_config（模型配置）

存储每个供应商下可用的模型定义。

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| id | INT | AUTO_INCREMENT PRIMARY KEY | 主键 |
| provider_id | INT | NOT NULL, FOREIGN KEY → provider_config(id) | 所属供应商 ID |
| name | VARCHAR(255) | NOT NULL | 模型对外名称，用于 API 请求中的 `model` 字段匹配 |
| model_id | VARCHAR(255) | NOT NULL DEFAULT '' | 上游 API 实际使用的 model 字段值（可不同于 name） |
| priority | INT | NOT NULL DEFAULT 0 | 优先级，数值越小越优先转发 |
| status | VARCHAR(16) | NOT NULL DEFAULT 'available' | 状态：`available`（可用）/ `disabled`（禁用） |
| timeout | INT | NOT NULL DEFAULT 30 | 请求超时时间（秒） |
| protocols | VARCHAR(255) | NOT NULL DEFAULT 'openai' | 支持的协议，逗号分隔（如 `openai,anthropic`） |
| context_length | INT | NOT NULL DEFAULT 256000 | 上下文窗口大小（token 数） |
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**用途：** 定义可供转发的模型列表。`name` 和 `model_id` 分离的设计允许将上游的某个模型 ID 映射为不同的对外名称。

**上下文窗口校验：** 转发请求前会估算 prompt token 数，与 `context_length` 对比，过滤掉窗口不足的模型。

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

**用途：** 客户端调用 `/v1/chat/completions` 和 `/v1/messages` 时使用 Bearer Token 鉴权。

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

**用途：** 管理员通过 Ed25519 签名访问 Admin API 时，服务端根据 `X-Admin-Fingerprint` 头查询此表获取公钥进行验签。

**指纹自动填充：** 服务启动时自动扫描 `fingerprint IS NULL` 的行，解析公钥计算 SHA256 指纹并回填。

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
| created_time | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP | 创建时间 |
| last_updated | DATETIME | NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP | 最后更新时间 |

**索引：** `idx_provider_id`（provider_id）、`idx_is_active`（is_active）

**用途：** 将凭证从供应商中剥离，允许多组凭证隶属同一供应商。转发请求时按 `priority` 升序选取第一个活跃凭证。

**加密说明：**
- `api_key` 和 `encrypted_password` 使用 AES-256-GCM 加密
- 加密密钥来自环境变量 `ENCRYPTION_KEY`（64 位 hex = 32 字节）
- 每次加密生成随机 12 字节 nonce，以 base64(nonce + ciphertext) 格式存储
- Admin API 返回时自动解密为明文

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
| protocol | VARCHAR(32) | NOT NULL DEFAULT 'openai' | 协议类型（`openai` / `anthropic`） |
| status | VARCHAR(16) | NOT NULL DEFAULT 'success' | 请求状态（`success` / `failed`） |
| error_message | TEXT | NULL | 错误信息（失败时记录） |
| prompt_tokens | INT | NOT NULL DEFAULT 0 | prompt token 数 |
| completion_tokens | INT | NOT NULL DEFAULT 0 | 生成 token 数 |
| total_tokens | INT | NOT NULL DEFAULT 0 | 总 token 数 |
| cache_hit_tokens | INT | NOT NULL DEFAULT 0 | 缓存命中的 token 数（仅 OpenAI） |
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

| 字段 | OpenAI | Anthropic |
|------|--------|-----------|
| prompt_tokens | `usage.prompt_tokens` | `usage.input_tokens` |
| completion_tokens | `usage.completion_tokens` | `usage.output_tokens` |
| total_tokens | `usage.total_tokens` | `input_tokens + output_tokens` |
| cache_hit_tokens | `usage.prompt_tokens_details.cached_tokens` | `usage.cache_read_input_tokens` |
| cache_miss_tokens | `prompt_tokens - cached_tokens` | `input_tokens` |
