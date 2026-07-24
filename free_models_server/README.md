# free_models_server

统一的模型代理服务，对外暴露类 OpenAI 与 Anthropic 的聊天补全接口，支持多供应商按优先级和可用性自动切换。

## 核心特性

- **双协议支持**：OpenAI (`/v1/chat/completions`) 与 Anthropic (`/v1/messages`)
- **按模型配置协议**：`model_config.protocols` 字段标识模型支持的协议，请求时自动匹配
- **优先级与故障切换**：按 `priority` 排序，失败自动降级，带惩罚机制
- **多级缓存**：基于 Redis 的模型/供应商/API Key 缓存，支持 Redis 不可用时自动降级到内存缓存，TTL 统一配置
- **Admin 管理接口**：SSH Ed25519 公钥签名鉴权，提供 providers / models / api_keys 的完整 CRUD
- **优雅关闭**：支持 SIGTERM / Ctrl-C，30 秒超时强制退出

技术栈：Rust / actix-web 4 / Sea-ORM 1（MySQL）/ reqwest 0.12 / Redis

## 目录结构

```
free_models_server/
├── Cargo.toml
├── Cargo.lock
├── Dockerfile                  # 多阶段 rust:alpine 构建
├── .env                        # 运行配置（DATABASE_URL / RUST_LOG 等）
├── migrations/                 # 手动执行的 SQL 迁移
│   ├── 001_init.sql
│   ├── 002_add_model_config_columns.sql
│   └── 003_add_admin_key_and_usage.sql
└── src/
    ├── main.rs                 # 入口：AppState、路由、中间件注册、优雅关闭
    ├── config/mod.rs           # Config（环境变量读取）
    ├── error.rs                # 统一错误响应（OpenAI 格式 + Anthropic 格式）
    ├── db/                     # Sea-ORM 实体与连接
    │   ├── entities/           # provider_config / model_config / api_key / admin_key / usage_log
    │   ├── redis.rs            # 统一 Redis 连接管理与降级封装
    │   └── mod.rs              # init_db（连接池配置）
    ├── middleware/
    │   ├── auth.rs             # AuthMiddleware（Bearer 鉴权，协议感知错误格式）
    │   └── admin_auth.rs       # AdminAuthMiddleware（SSH Ed25519 公钥签名鉴权）
    ├── handler/
    │   ├── chat_handler.rs     # list_models / chat_completions / anthropic_messages / health_check
    │   └── admin_handler.rs    # Admin CRUD handlers，所有 DB 操作委托到 service 层
    ├── service/
    │   ├── admin_key_service.rs # 管理端 SSH 公钥 CRUD + 自动填充 Fingerprint
    │   ├── api_key_service.rs   # API Key CRUD，自动生成 fm- 前缀随机密钥
    │   ├── model_service.rs     # 模型查询、优先级、缓存、合并、协议检查
    │   ├── model_service_ext.rs # 扩展 Model CRUD + 活跃数统计（含 provider join 过滤）
    │   ├── provider_service.rs  # 供应商 CRUD
    │   ├── proxy_service.rs     # 供应商转发、多协议、超时、降级、SSE 优雅关闭
    │   ├── tokenizer.rs         # token 估算（tiktoken-rs BPE）
    │   └── usage_log_service.rs # 用量持久化
    └── util/
        ├── crud.rs             # 泛型 count_total / count_active 辅助函数
        ├── api_key_cache.rs    # API Key 全量缓存（Redis Set + 内存 fallback）
        └── penalty.rs          # PriorityPenalty（失败惩罚）
```

## 环境变量

| 变量名 | 默认值 | 说明 |
|--------|--------|------|
| `DATABASE_URL` | — | MySQL 连接串，**必须设置** |
| `SERVER_HOST` | `0.0.0.0` | 服务监听地址 |
| `SERVER_PORT` | `8080` | 服务监听端口 |
| `DB_MAX_CONNECTIONS` | `100` | 数据库连接池最大连接数 |
| `RUST_LOG` | `info` | 日志级别 |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis 连接地址 |
| `REDIS_ENABLED` | `true` | 是否启用 Redis，false 时全部降级到内存缓存 |
| `REDIS_CACHE_TTL_MODEL_SEC` | `30` | 模型缓存 TTL（秒） |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | `600` | 供应商缓存 TTL（秒） |
| `REDIS_CACHE_TTL_PENALTY_SEC` | `1800` | 失败惩罚缓存 TTL（秒） |
| `REDIS_CACHE_TTL_API_KEY_SEC` | `3600` | API Key 缓存 TTL（秒） |
| `MODEL_CACHE_TTL_SEC` | `30` | ~~（已废弃，使用 `REDIS_CACHE_TTL_MODEL_SEC` 替代）~~ |
| `PROVIDER_CACHE_TTL_SEC` | `600` | ~~（已废弃，使用 `REDIS_CACHE_TTL_PROVIDER_SEC` 替代）~~ |
| `PENALTY_TTL_SEC` | `1800` | ~~（已废弃，使用 `REDIS_CACHE_TTL_PENALTY_SEC` 替代）~~ |

## 数据库迁移

手动执行 SQL 文件：

```bash
# 创建数据库后依次执行
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql
```

## 路由

### 公共接口（无需鉴权）

| 路由 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 服务健康检查 |
| `/v1/models` | GET | 列出可用模型（含 OpenAI 协议过滤） |
| `/v1/chat/completions` | POST | 聊天补全（OpenAI 协议） |
| `/v1/messages` | POST | 聊天补全（Anthropic 协议） |

### Admin 管理接口（SSH Ed25519 签名鉴权）

所有 `/admin/*` 接口受 `AdminAuthMiddleware` 保护，需在请求头中携带：

- `X-Admin-Fingerprint` — 公钥 SHA256 fingerprint
- `X-Admin-Timestamp` — 当前 Unix 时间戳（±300 秒防重放）
- `X-Admin-Signature` — 对 `METHOD:PATH:TIMESTAMP` 的 Ed25519 签名（Base64）

| 路由 | 方法 | 说明 |
|------|------|------|
| `/admin/service/status` | GET | 服务状态 + 各类资源激活/总数统计 |
| `/admin/cache/refresh` | POST | 刷新所有缓存（模型/供应商/API Key/惩罚） |
| `/admin/providers` | GET / POST | 列出 / 创建供应商 |
| `/admin/providers/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除供应商 |
| `/admin/models` | GET / POST | 列出 / 创建模型 |
| `/admin/models/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除模型 |
| `/admin/api_keys` | GET / POST | 列出 / 创建 API Key |
| `/admin/api_keys/{id}` | GET / PUT / DELETE | 获取 / 更新 / 删除 API Key |

## 鉴权机制

### 客户端接口鉴权（`/v1/*`）

全局 `AuthMiddleware` 对所有请求生效（除白名单路径外）。请求头中需携带 `Authorization: Bearer <api_key>`，系统在 `api_key` 表中验证 Key 的存在性和激活状态。

白名单路径：`/health`

### Admin 接口鉴权（`/admin/*`）

`AdminAuthMiddleware` 对 `/admin/*` 路径生效，使用 SSH Ed25519 公钥签名鉴权：

1. 从 `X-Admin-Fingerprint` 读取 fingerprint
2. 按 fingerprint 查询 `admin_key` 表，要求 `is_active = true`
3. 验证 `X-Admin-Timestamp` 与当前时间差不超过 300 秒（防重放）
4. 解析 OpenSSH `ssh-ed25519` 格式公钥，提取 raw 32 字节公钥
5. 构造签名内容 `METHOD:PATH:TIMESTAMP`，用 Ed25519 验证 `X-Admin-Signature`
6. 任一验证失败返回 401 Unauthorized

### 添加管理公钥

将管理端的 SSH Ed25519 公钥插入 `admin_key` 表：

```sql
INSERT INTO admin_key (name, public_key, fingerprint, is_active)
VALUES ('manager-01', 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...', 'SHA256:xxxx...', true);
```

## 使用示例

### 环境准备

```bash
# 1. 创建 .env
cat > .env << 'EOF'
DATABASE_URL=mysql://root:password@localhost/free_models
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
DB_MAX_CONNECTIONS=100
RUST_LOG=info

# Redis 配置
REDIS_URL=redis://127.0.0.1:6379
REDIS_ENABLED=true

# 缓存 TTL（秒）
REDIS_CACHE_TTL_MODEL_SEC=30
REDIS_CACHE_TTL_PROVIDER_SEC=600
REDIS_CACHE_TTL_PENALTY_SEC=1800
REDIS_CACHE_TTL_API_KEY_SEC=3600
EOF

# 2. 创建数据库并执行迁移
mysql -u root -p -e "CREATE DATABASE IF NOT EXISTS free_models CHARACTER SET utf8mb4;"
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql
```

### 运行服务

```bash
# 开发模式
cargo run

# 生产模式
cargo build --release
./target/release/free_models_server
```

### Docker 构建与运行

```bash
# 构建
docker build -t free_models_server .

# 运行
docker run --rm -p 8080:8080 --env-file .env free_models_server
```

### 调用聊天接口（OpenAI 协议）

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

### 调用聊天接口（Anthropic 协议）

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 1024
  }'
```

### 列出可用模型

```bash
curl http://localhost:8080/v1/models \
  -H "Authorization: Bearer YOUR_API_KEY"
```

### 刷新缓存（Admin 接口）

```bash
curl -X POST http://localhost:8080/admin/cache/refresh \
  -H "X-Admin-Fingerprint: YOUR_FINGERPRINT" \
  -H "X-Admin-Timestamp: $(date +%s)" \
  -H "X-Admin-Signature: YOUR_SIGNATURE"
```

## 数据库表结构

### provider_config — 上游供应商配置

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `name` | VARCHAR(255) | 供应商名称 |
| `base_url` | VARCHAR(255) | 上游 API Base URL |
| `api_key` | TEXT | 上游 API Key |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 最后更新时间 |

### model_config — 模型配置

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `provider_id` | INT (FK) | 关联 `provider_config.id` |
| `name` | VARCHAR(255) | 对外暴露的模型名称 |
| `model_id` | VARCHAR(255) | 上游 API 实际使用的 model 字段值 |
| `timeout` | INT | 请求超时秒数 |
| `protocols` | VARCHAR(255) | 支持的协议（逗号分隔，如 `"openai,anthropic"`） |
| `context_length` | INT | 上下文窗口上限（token），用于请求前校验 |
| `priority` | INT | 优先级（数字越小优先级越高） |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 最后更新时间 |

### api_key — 客户端 API Key

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `key_value` | VARCHAR(255) (UNIQUE) | API Key 值（`fm-` 前缀 + 64 位随机字符串） |
| `name` | VARCHAR(255) | 名称标识 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 最后更新时间 |

### admin_key — 管理端 SSH 公钥

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `name` | VARCHAR(255) | 名称标识 |
| `public_key` | TEXT | SSH Ed25519 公钥原文 |
| `fingerprint` | VARCHAR(255) (UNIQUE) | 公钥 SHA256 fingerprint，自动计算填充 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 最后更新时间 |

### usage_log — 用量记录

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | BIGINT (PK) | 自增主键 |
| `model_name` | VARCHAR(255) | 使用的模型名称 |
| `provider_name` | VARCHAR(255) | 使用的供应商名称 |
| `prompt_tokens` | INT | 提示词 token 数 |
| `completion_tokens` | INT | 补全 token 数 |
| `total_tokens` | INT | token 总数 |
| `request_timestamp` | DATETIME | 请求时间 |

## 设计说明

### 数据库连接池

通过 `sea_orm::Database::connect()` 初始化，默认最大连接数 100，支持环境变量 `DB_MAX_CONNECTIONS` 覆盖。

### 缓存策略

所有缓存基于 Redis 实现，Redis 不可用时自动降级到进程内内存缓存，保证服务高可用：

- **模型缓存**：`ModelCache`，Redis key 前缀 `model_cache:`，TTL 30 秒，JSON 序列化存储
- **供应商缓存**：`ProviderCache`，Redis key 前缀 `provider_cache:`，TTL 600 秒，JSON 序列化存储
- **API Key 缓存**：`ApiKeyCache`，Redis Set `api_keys:active`，启动时全量加载，鉴权时用 `SISMEMBER` 快速判断
- **失败惩罚**：`PriorityPenalty`，Redis key 前缀 `penalty:`，本地带 1 秒二级缓存减少高频网络往返
- **缓存刷新**：调用 `POST /admin/cache/refresh` 会同时清理 Redis 中的 model_cache:*、provider_cache:*、penalty:* 键，并重新加载 API Key Set

### 惩罚机制

`PriorityPenalty` 为每个失败的（provider, model）组合记录惩罚时间戳。惩罚有效期内该组合优先级降低，TTL 默认 1800 秒。调用成功或手动刷新缓存时清除对应惩罚。

### 协议检查

`model_config.protocols` 字段为逗号分隔字符串，如 `"openai"` 或 `"openai,anthropic"`。收到请求时：
- `/v1/chat/completions` 路径要求模型支持 `openai` 协议
- `/v1/messages` 路径要求模型支持 `anthropic` 协议
- 不支持的模型在候选阶段被过滤

### 模型 ID 映射

`model_config.name` 为对外暴露的模型名，`model_config.model_id` 为实际调用上游供应商 API 时使用的 model 字段值。新增模型时如 model_id 为空，系统使用 name 作为 fallback。

### 服务层架构

`admin_handler.rs` 中的数据库访问全部委托到 `service/` 下的模块：

- `provider_service.rs` — Provider CRUD
- `model_service_ext.rs` — Model CRUD + 活跃数统计
- `api_key_service.rs` — API Key CRUD + 自动密钥生成
- `admin_key_service.rs` — 管理端 SSH 公钥管理

通用统计函数抽离到 `util/crud.rs` 的泛型辅助函数 (`count_total` / `count_active`)。
