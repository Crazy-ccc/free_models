# free_models_token

统一的免费模型代理服务，对外暴露类 OpenAI 与 Anthropic 接口，按优先级和可用性将请求转发到多个上游供应商。

项目采用双目录结构：后端代理服务（Rust） + Tauri 桌面管理端。

```
free_models_token/
├── free_models_server/    # 后端代理服务
│   ├── src/               # Rust 源码
│   ├── migrations/        # 数据库迁移 SQL
│   ├── Cargo.toml
│   └── Dockerfile
└── free_models_manager/   # 桌面管理端
    ├── src/               # React + TS 前端源码
    ├── src-tauri/         # Tauri Rust 后端
    ├── package.json
    └── vite.config.ts
```

---

## free_models_server

后端代理服务。Rust / actix-web 4 / Sea-ORM 1 (MySQL) / reqwest 0.12 / Redis / tiktoken-rs。

**详细文档**：[free_models_server/README.md](./free_models_server/README.md)

### 核心能力

- **双协议** — OpenAI `/v1/chat/completions` 与 Anthropic `/v1/messages`
- **优先级与故障切换** — 按 `priority` 排序，失败自动降级 + 惩罚机制
- **多级缓存** — Redis + 内存 fallback，模型/供应商/API Key 全量缓存
- **Token 计数** — `tiktoken-rs` cl100k_base BPE 估算 prompt token，做上下文窗口校验
- **用量日志** — 每次请求记录完整用量到 `usage_log` 表（token / 耗时 / 协议 / 状态）
- **凭证管理** — 一个供应商支持多组凭证（api_key / account / encrypted_password），按优先级排序
- **Admin 接口** — SSH Ed25519 公钥签名鉴权，完整 CRUD

### 快速启动

```bash
cd free_models_server
cp .env.example .env           # 编辑 DATABASE_URL

mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql

cargo run --release
```

### Docker

```bash
docker build -t free_models_server .
docker run --rm -p 8080:8080 --env-file .env free_models_server
```

### 公共路由

| 路由 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/v1/models` | GET | 列出可用模型 |
| `/v1/chat/completions` | POST | 聊天补全（OpenAI 协议） |
| `/v1/messages` | POST | 聊天补全（Anthropic 协议） |

### Admin 路由（需 Ed25519 签名鉴权）

所有 `/admin/*` 请求头：`X-Admin-Fingerprint` + `X-Admin-Timestamp` + `X-Admin-Signature`（签名内容 `METHOD:PATH:TIMESTAMP`）。

| 路由 | 方法 | 说明 |
|------|------|------|
| `/admin/service/status` | GET | 服务状态统计 |
| `/admin/cache/refresh` | POST | 刷新所有缓存 |
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

## free_models_manager

基于 Tauri v2 的本地桌面管理应用。技术栈：Rust / Tauri v2 / React / TypeScript / LESS / Vite。

### 功能模块

| 模块 | 功能 |
|------|------|
| 概览 | 服务健康状态、资源统计（彩色数字）、惩罚中的模型列表、刷新缓存 |
| 供应商 | CRUD、一键导入上游模型、凭证管理（多凭证/启停/测试连接） |
| 模型 | CRUD（协议标签/优先级/超时/状态），原生表格+内联筛选（供应商/协议/状态/排序） |
| API Keys | CRUD，自动生成 `fm-` 前缀 64 位密钥，脱敏显示，一键复制 |
| 设置 | 后端地址绑定、SSH 密钥对路径配置、Fingerprint 展示、保存密钥 |

### 鉴权流程

1. 在设置页配置 Ed25519 私钥/公钥文件路径，加载后获得 Fingerprint
2. 将公钥插入服务端 `admin_key` 表
3. 每个 Admin 请求自动携带签名头完成鉴权

### 开发启动

```bash
cd free_models_manager
pnpm install
pnpm tauri dev          # 开发模式
pnpm tauri build        # 生产构建
```

---

## 数据库表结构

### provider_config — 供应商

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `name` | VARCHAR(255) | 名称 |
| `base_url` | VARCHAR(255) | 上游 API Base URL |
| `api_key` | TEXT | 默认 API Key |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### model_config — 模型

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `provider_id` | INT (FK) | `provider_config.id` |
| `name` | VARCHAR(255) | 对外暴露的模型名 |
| `model_id` | VARCHAR(255) | 上游 API model 字段 |
| `timeout` | INT | 超时秒数 |
| `protocols` | VARCHAR(255) | 支持协议（`,` 分隔，如 `"openai,anthropic"`） |
| `context_length` | INT | 上下文窗口上限（token） |
| `priority` | INT | 优先级（越小越高） |
| `status` | VARCHAR(16) | 状态：`available` / `unavailable` / `deprecated` |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### api_key — 客户端 Key

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `key_value` | VARCHAR(255) UNIQUE | `fm-` 前缀 + 64 位随机串 |
| `name` | VARCHAR(255) | 名称 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### admin_key — 管理端 SSH 公钥

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `name` | VARCHAR(255) | 名称 |
| `public_key` | TEXT | OpenSSH Ed25519 公钥原文 |
| `fingerprint` | VARCHAR(64) UNIQUE | SHA256 fingerprint，自动填充 |
| `is_active` | TINYINT(1) | 是否启用 |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

### provider_credential — 供应商凭证

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT (PK) | 自增主键 |
| `provider_id` | INT (FK) | `provider_config.id` |
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
| `id` | BIGINT (PK) | 自增主键 |
| `api_key_id` | INT NULL | 关联 API Key |
| `api_key_name` | VARCHAR(255) | Key 名称快照 |
| `model_config_id` | INT NULL | 关联模型 ID |
| `provider_config_id` | INT NULL | 关联供应商 ID |
| `provider_credential_id` | INT NULL | 关联凭证 ID |
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

## 环境变量（Server）

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `DATABASE_URL` | — | MySQL 连接串，**必填** |
| `SERVER_HOST` | `0.0.0.0` | 监听地址 |
| `SERVER_PORT` | `8080` | 监听端口 |
| `DB_MAX_CONNECTIONS` | `100` | 连接池上限 |
| `RUST_LOG` | `info` | 日志级别 |
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis 地址 |
| `REDIS_ENABLED` | `true` | 启用 Redis |
| `REDIS_CACHE_TTL_MODEL_SEC` | `30` | 模型缓存 TTL |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | `600` | 供应商缓存 TTL |
| `REDIS_CACHE_TTL_PENALTY_SEC` | `1800` | 惩罚缓存 TTL |
| `REDIS_CACHE_TTL_API_KEY_SEC` | `3600` | API Key 缓存 TTL |
| `ENCRYPTION_KEY` | — | AES-256 密钥（32 字节 hex） |
