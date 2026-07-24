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
└────────┬───────────────────────┬────────────┘
         │ HTTP 转发              │ Admin API (Ed25519)
         ▼                       ▼
   上游 LLM 供应商         free_models_manager
   (OpenAI/Anthropic 等)   (Tauri 桌面管理端)
```

## 项目结构

```
free_models_token/
├── README.md                 # 本文件：项目概览
├── free_models_server/       # 后端代理服务 (Rust)
│   ├── README.md             # 快速启动与配置
│   └── src/                  # Rust 源码
└── free_models_manager/      # 桌面管理端 (Tauri + React)
    └── README.md             # 快速启动与功能概览
```

## 核心能力

- **双协议** — OpenAI `/v1/chat/completions` 与 Anthropic `/v1/messages`
- **优先级与故障切换** — 按 `priority` 排序，失败自动降级 + 惩罚机制
- **多级缓存** — Redis + 内存 fallback，Redis 宕机不影响服务
- **Token 计数** — `tiktoken-rs` cl100k_base BPE，请求前估算 prompt token 做上下文窗口校验
- **用量日志** — 每次请求记录完整用量到 `usage_log` 表
- **凭证管理** — 一个供应商支持多组凭证（api_key / account / password），AES-256-GCM 加密存储
- **Admin 接口** — SSH Ed25519 公钥签名鉴权，完整 CRUD

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
| [架构总览](docx/architecture.md) | 系统架构图、目录结构、技术栈、核心流程 |
| [API 端点](docx/api_endpoints.md) | 公共 & Admin API 完整参考 |
| [数据库表结构](docx/database_schema.md) | 6 张表的字段、约束、索引、关系 |
| [认证鉴权](docx/authentication.md) | API Key Bearer + Ed25519 签名鉴权流程 |
| [缓存策略](docx/caching_strategy.md) | Redis + 内存多级缓存、TTL、刷新机制 |
| [代理转发与 Token 计算](docx/proxy_and_token.md) | 故障切换、流式转发、tiktoken |
| [管理员界面](docx/admin_ui.md) | 页面功能、组件说明、签名流程 |
| [Server 快速启动](free_models_server/README.md) | 环境变量、配置详解 |
| [Manager 快速启动](free_models_manager/README.md) | 功能模块、开发指引 |
