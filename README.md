# free_models_token

统一的免费模型代理服务，对外暴露类 OpenAI 与 Anthropic 接口，按优先级和可用性将请求转发到多个上游 LLM 供应商。

项目采用双目录结构：后端代理服务（Rust） + Tauri 桌面管理端。

```
                     ┌─────────────────────┐
                     │   客户端 (OpenAI SDK 等) │
                     └────────┬────────────┘
                              │ Bearer Token
                              ▼
┌─────────────────────────────────────────────┐
│       free_models_server (Rust)              │
│  协议转换 · 优先级调度 · 故障切换 · 用量日志    │
│   ┌─────────────────────────────────────┐   │
│   │  模块化数据访问层 (src/db)            │   │
│   │  entities + impls + types            │   │
│   └─────────────────────────────────────┘   │
└────────┬───────────────────────┬────────────┘
         │ HTTP 转发              │ Admin API (Ed25519)
         ▼                       ▼
   上游 LLM 供应商         free_models_manager
   (OpenAI/Anthropic 等)   (Tauri 桌面管理端)
```

## 项目结构

```
free_models_token/
├── README.md                     # 本文件：项目概览
├── docx/                         # 详细技术文档
├── free_models_server/           # 后端代理服务 (Rust，单 crate)
│   ├── README.md                 # 快速启动与配置
│   └── src/                      # 业务逻辑 + Web 层 + 数据访问层
└── free_models_manager/          # 桌面管理端 (Tauri + React)
    └── README.md                 # 快速启动与功能概览
```

### 单 crate 模块化数据访问层

`free_models_server` 为单 crate，内部通过 `src/db/` 模块实现数据访问：

- **`src/db/entities/`** — SeaORM 实体定义（对应 7 张表）
- **`src/db/impls/`** — 各 Store 的 SeaORM 实现，通过 `Database` 结构体聚合
- **`src/db/types.rs`** — DTO 类型（`UsageLogInsert`、`UsageLogStatItem` 等）
- **`src/cache.rs`** — `RedisManager` 实现，实现 `CacheStore` trait，Redis 不可用时透明降级到内存

业务代码直接调用 `Database` 上的具体 store 结构体（如 `state.database.provider_configs.list()`），不引入 trait 抽象层。

## 核心能力

- **三协议** — OpenAI `/v1/chat/completions`、Anthropic `/v1/messages`、OpenAI Responses `/v1/responses`
- **优先级与故障切换** — 按 `priority` 排序，失败自动降级 + 熔断惩罚机制
- **多级缓存** — Redis + moka 内存缓存，Redis 宕机透明降级
- **Token 计数** — `tiktoken-rs` cl100k_base BPE，请求前估算 prompt token 做上下文窗口校验
- **用量日志** — 每次请求记录完整用量到 `usage_log` 表（含 cache hit/miss tokens），支持每日自动归档
- **凭证管理** — 一个供应商支持多组凭证（api_key / account / password），AES-256-GCM 加密存储
- **Admin 接口** — SSH Ed25519 公钥签名鉴权，按职责拆分为 8 个子模块的完整 CRUD

## 快速启动

### 后端服务

```bash
cd free_models_server
cp .env.example .env           # 编辑 DATABASE_URL

# 建库 + 迁移
mysql -u root -p -e "CREATE DATABASE IF NOT EXISTS free_models CHARACTER SET utf8mb4;"
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql
mysql -u root -p free_models < migrations/004_create_usage_log_daily.sql
mysql -u root -p free_models < migrations/005_create_provider_model_map.sql

cargo run --release
```

### 桌面管理端

```bash
cd free_models_manager
pnpm install
pnpm tauri dev            # 开发模式
pnpm tauri build          # 生产构建
```

## 详细文档

| 文档 | 内容 |
|------|------|
| [架构总览](docx/architecture.md) | 系统架构图、三层 crate 结构、技术栈、核心流程 |
| [API 端点](docx/api_endpoints.md) | 公共 & Admin API 完整参考 |
| [数据库表结构](docx/database_schema.md) | 6 张表的字段、约束、索引、关系 |
| [认证鉴权](docx/authentication.md) | API Key Bearer + Ed25519 签名鉴权流程 |
| [缓存策略](docx/caching_strategy.md) | Redis + 内存多级缓存、TTL、刷新机制 |
| [代理转发与 Token 计算](docx/proxy_and_token.md) | 故障切换、流式转发、tiktoken |
| [管理员界面](docx/admin_ui.md) | 页面功能、组件说明、签名流程 |
| [Server 快速启动](free_models_server/README.md) | 环境变量、配置详解、目录结构 |
| [Manager 快速启动](free_models_manager/README.md) | 功能模块、开发指引 |
