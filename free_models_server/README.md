# free_models_server

统一的模型代理服务，对外暴露类 OpenAI 与 Anthropic 的聊天补全接口，支持多供应商按优先级和可用性自动切换。

技术栈：Rust / actix-web 4 / Sea-ORM 1（MySQL）/ reqwest 0.12 / Redis / tiktoken-rs

---

## 核心特性

- **双协议** — OpenAI `/v1/chat/completions` 与 Anthropic `/v1/messages`
- **优先级与故障切换** — 按 `priority` 排序，失败自动降级，带惩罚机制
- **多级缓存** — Redis + 内存 fallback，TTL 统一配置
- **Token 计数** — `tiktoken-rs` cl100k_base BPE，请求前估算 prompt token，上下文窗口校验
- **用量日志** — 每次请求记录完整用量到 `usage_log` 表（token / 耗时 / 协议 / 状态 / 缓存命中）
- **凭证管理** — 一个供应商支持多组凭证，api_key / account / password，AES-256-GCM 加密存储
- **凭证测试** — 通过 `/admin/test_credential` 测试凭证连接有效性
- **Admin 接口** — SSH Ed25519 公钥签名鉴权，完整 CRUD
- **优雅关闭** — SIGTERM / Ctrl-C，30 秒超时强制退出

---

## 目录结构

```
free_models_server/
├── Cargo.toml
├── Cargo.lock
├── Dockerfile                  # 多阶段 rust:alpine 构建
├── .env                        # 运行配置
├── migrations/
│   ├── 001_init.sql
│   ├── 002_add_model_config_columns.sql
│   └── 003_add_admin_key_and_usage.sql
└── src/
    ├── main.rs                 # 入口：AppState、路由、中间件、优雅关闭
    ├── config/mod.rs           # 环境变量读取
    ├── error.rs                # 统一错误响应（OpenAI + Anthropic 格式）
    ├── db/
    │   ├── entities/           # provider_config / model_config / api_key / admin_key / usage_log / provider_credential
    │   ├── redis.rs            # Redis 管理 + 降级封装
    │   └── mod.rs              # 连接池初始化
    ├── middleware/
    │   ├── auth.rs             # Bearer Token 鉴权
    │   └── admin_auth.rs       # SSH Ed25519 签名鉴权
    ├── handler/
    │   ├── chat_handler.rs     # /v1/models / /v1/chat/completions / /v1/messages / /health
    │   └── admin_handler.rs    # Admin CRUD（DB 操作委托到 service 层）
    ├── service/
    │   ├── admin_key_service.rs           # SSH 公钥管理 + 自动填充 fingerprint
    │   ├── api_key_service.rs             # API Key CRUD + 自动生成密钥
    │   ├── model_service.rs               # 模型查询、优先级、缓存、合并、协议检查
    │   ├── model_service_ext.rs           # Model CRUD + 活跃数统计（含 provider join）
    │   ├── provider_service.rs            # 供应商 CRUD
    │   ├── provider_credential_service.rs # 凭证 CRUD
    │   ├── proxy_service.rs              # 上游转发、降级、SSE、用量日志
    │   └── usage_log_service.rs          # 用量持久化
    └── util/
        ├── crud.rs             # 泛型 count_total / count_active
        ├── api_key_cache.rs    # Redis Set + 内存 fallback
        ├── encryption.rs       # AES-256-GCM 加密/解密
        ├── penalty.rs          # PriorityPenalty 失败惩罚
        └── tokenizer.rs        # cl100k_base BPE token 估算
```

---

## 环境变量

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DATABASE_URL` | — | MySQL 连接串，**必填** |
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `8080` | 监听端口 |
| `DB_MAX_CONNECTIONS` | `100` | 连接池上限 |
| `RUST_LOG` | `info` | 日志级别 |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis 地址 |
| `REDIS_ENABLED` | `true` | 启用 Redis（false 时全部降级到内存） |
| `REDIS_CACHE_TTL_MODEL_SEC` | `30` | 模型缓存 TTL |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | `600` | 供应商缓存 TTL |
| `REDIS_CACHE_TTL_PENALTY_SEC` | `1800` | 惩罚缓存 TTL |
| `REDIS_CACHE_TTL_API_KEY_SEC` | `3600` | API Key 缓存 TTL |
| `ENCRYPTION_KEY` | — | AES-256 密钥（32 字节 hex），用于凭证加密 |

---

## 路由

### 公共接口（无需鉴权）

| 路由 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/v1/models` | GET | 列出可用模型（OpenAI 格式） |
| `/v1/chat/completions` | POST | 聊天补全（OpenAI 协议） |
| `/v1/messages` | POST | 聊天补全（Anthropic 协议） |

### Admin 接口（需 Ed25519 签名鉴权）

请求头：`X-Admin-Fingerprint` + `X-Admin-Timestamp` + `X-Admin-Signature`

| 路由 | 方法 | 说明 |
|------|------|------|
| `/admin/service/status` | GET | 服务状态 + 资源统计 |
| `/admin/cache/refresh` | POST | 刷新全部缓存 |
| `/admin/providers` | GET / POST | 列出 / 创建 |
| `/admin/providers/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除 |
| `/admin/models` | GET / POST | 列出 / 创建 |
| `/admin/models/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除 |
| `/admin/api_keys` | GET / POST | 列出 / 创建 |
| `/admin/api_keys/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除 |
| `/admin/provider_credentials` | GET / POST | 列出 / 创建 |
| `/admin/provider_credentials/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除 |
| `/admin/test_credential` | POST | 测试凭证连接 |

---

## 鉴权

### 客户端鉴权（`/v1/*`）

全局 `AuthMiddleware`（白名单：`/health`）。请求头 `Authorization: Bearer <api_key>`，查询 `api_key` 表验证存在性和激活状态。

### Admin 鉴权（`/admin/*`）

`AdminAuthMiddleware`，SSH Ed25519 公钥签名：

1. 读取 `X-Admin-Fingerprint` 查 `admin_key` 表（`is_active = true`）
2. 验证 `X-Admin-Timestamp` 在 ±300 秒内（防重放）
3. 构造 `METHOD:PATH:TIMESTAMP`，用公钥验证 Ed25519 签名
4. 失败返回 401

```sql
INSERT INTO admin_key (name, public_key, fingerprint, is_active)
VALUES ('manager-01', 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...', 'SHA256:xxxx...', true);
```

---

## 使用示例

### 环境准备

```bash
mysql -u root -p -e "CREATE DATABASE IF NOT EXISTS free_models CHARACTER SET utf8mb4;"
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql

cat > .env << 'EOF'
DATABASE_URL=mysql://root:password@localhost/free_models
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
DB_MAX_CONNECTIONS=100
RUST_LOG=info
REDIS_URL=redis://127.0.0.1:6379
REDIS_ENABLED=true
REDIS_CACHE_TTL_MODEL_SEC=30
REDIS_CACHE_TTL_PROVIDER_SEC=600
REDIS_CACHE_TTL_PENALTY_SEC=1800
REDIS_CACHE_TTL_API_KEY_SEC=3600
ENCRYPTION_KEY=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
EOF
```

### 运行

```bash
cargo run              # 开发
cargo build --release && ./target/release/free_models_server  # 生产
docker build -t free_models_server . && docker run --rm -p 8080:8080 --env-file .env free_models_server
```

### 调用

```bash
# 列出模型
curl http://localhost:8080/v1/models -H "Authorization: Bearer <API_KEY>"

# 聊天（OpenAI 协议）
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","messages":[{"role":"user","content":"Hello"}]}'

# 聊天（Anthropic 协议）
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet","messages":[{"role":"user","content":"Hello"}],"max_tokens":1024}'
```

---

## 数据库表结构

### provider_config — 供应商

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增 |
| `name` | VARCHAR(255) | 名称 |
| `base_url` | VARCHAR(255) | 上游 API Base URL |
| `api_key` | TEXT | 默认 API Key |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### model_config — 模型

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增 |
| `provider_id` | INT (FK) | 关联供应商 |
| `name` | VARCHAR(255) | 对外暴露名 |
| `model_id` | VARCHAR(255) | 上游 API model 字段 |
| `timeout` | INT | 超时秒数 |
| `protocols` | VARCHAR(255) | 支持协议（`openai,anthropic`） |
| `context_length` | INT | 上下文窗口上限（token） |
| `priority` | INT | 优先级（越小越高） |
| `status` | VARCHAR(16) | `available` / `unavailable` / `deprecated` |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### api_key — 客户端 Key

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增 |
| `key_value` | VARCHAR(255) (UNIQUE) | `fm-` 前缀 + 64 位随机串 |
| `name` | VARCHAR(255) | 名称 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### admin_key — SSH 公钥

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增 |
| `name` | VARCHAR(255) | 名称 |
| `public_key` | TEXT | OpenSSH Ed25519 公钥 |
| `fingerprint` | VARCHAR(64) (UNIQUE) | SHA256 fingerprint |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### provider_credential — 凭证

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增 |
| `provider_id` | INT (FK) | 关联供应商 |
| `name` | VARCHAR(255) | 名称 |
| `api_key` | VARCHAR(512) | 凭证 Key（AES-GCM 加密） |
| `account` | VARCHAR(255) | 账号（可选） |
| `encrypted_password` | VARCHAR(512) | 密码（AES-GCM 加密，可选） |
| `priority` | INT | 优先级 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### usage_log — 用量日志

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT (PK) | 自增 |
| `api_key_id` | INT NULL | 关联 API Key |
| `api_key_name` | VARCHAR(255) | Key 名称快照 |
| `model_config_id` | INT NULL | 关联模型 |
| `provider_config_id` | INT NULL | 关联供应商 |
| `provider_credential_id` | INT NULL | 关联凭证 |
| `model_name` | VARCHAR(255) | 模型名称 |
| `provider_name` | VARCHAR(255) | 供应商名称 |
| `protocol` | VARCHAR(32) | `openai` / `anthropic` |
| `status` | VARCHAR(16) | `success` / `failed` |
| `error_message` | TEXT NULL | 错误信息 |
| `prompt_tokens` | INT | 提示词 token |
| `completion_tokens` | INT | 补全 token |
| `total_tokens` | INT | 总 token |
| `cache_hit_tokens` | INT | 缓存命中 |
| `cache_miss_tokens` | INT | 缓存未命中 |
| `duration_ms` | INT | 耗时（毫秒） |
| `is_stream` | BOOLEAN | 是否 SSE 流式 |
| `request_timestamp` | DATETIME | 请求时间 |

---

## 设计说明

### 缓存策略

Redis 不可用时自动降级到进程内内存缓存：

- **模型** — `ModelCache`，key 前缀 `model_cache:`，TTL 30s
- **供应商** — `ProviderCache`，key 前缀 `provider_cache:`，TTL 600s
- **API Key** — `ApiKeyCache`，Redis Set `api_keys:active`，启动时全量加载
- **惩罚** — `PriorityPenalty`，key 前缀 `penalty:`，本地 1s 二级缓存
- **刷新** — `POST /admin/cache/refresh` 清理全部缓存键并重新加载

### 惩罚机制

为每个失败的（provider, model）组合标记惩罚时间戳，有效期内该组合优先级降低。TTL 默认 1800 秒。调用成功或手动刷新缓存时清除。

### Token 计数

```rust
pub fn estimate_prompt_tokens(text: &str) -> usize {
    tiktoken_bpe().encode_with_special_tokens(text).len()
}
```

- 请求转发前估算 prompt token，与 `context_length` 对比做上下文校验
- 非流式请求：从上游响应解析 usage 字段写入 `usage_log`
- 流式请求：仅记录请求信息

### 协议检查

`protocols` 字段为逗号分隔字符串。`/v1/chat/completions` 要求 `openai` 协议，`/v1/messages` 要求 `anthropic` 协议。不匹配的模型在候选阶段被过滤。

### 模型 ID 映射

`name` → 对外暴露名，`model_id` → 上游 API 使用的 model 值。`model_id` 为空时回退到 `name`。

### 凭证加密

`password` 字段使用 AES-256-GCM 加密，密钥由 `ENCRYPTION_KEY` 环境变量提供（32 字节 hex）。读取时自动解密。

### 服务层架构

所有 Admin handler 的数据库访问委托到 `service/` 层：

| 模块 | 职责 |
|------|------|
| `provider_service` | Provider CRUD |
| `model_service_ext` | Model CRUD + 统计 |
| `api_key_service` | API Key CRUD + 自动生成 |
| `admin_key_service` | SSH 公钥管理 + fingerprint |
| `provider_credential_service` | 凭证 CRUD |

通用统计函数（`count_total` / `count_active`）抽离到 `util/crud.rs`。
