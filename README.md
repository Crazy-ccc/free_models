# free_models_token

统一的免费模型代理服务：对外暴露类 OpenAI 与 Anthropic 接口，按优先级和可用性将请求自动转发到多个上游 LLM 供应商，并附带一个 Tauri 桌面管理端。

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

## 目录

- [项目组成](#项目组成)
- [核心能力](#核心能力)
- [快速开始](#快速开始)
- [使用示例](#使用示例)
- [详细文档](#详细文档)

## 项目组成

仓库包含两个相互独立的包（无 monorepo 工具链）：

```
free_models_token/
├── README.md                     # 本文件：项目概览
├── docx/                         # 详细技术文档
├── migrate_mysql_to_sqlite.py    # 存量 MySQL → SQLite 数据迁移脚本（PEP 723）
├── free_models_server/           # 后端代理服务 (Rust，单 crate)
│   ├── README.md                 # 快速启动与配置、Docker 部署
│   ├── .env.docker               # 容器部署专用环境变量
│   ├── entrypoint.sh             # 容器入口（数据卷属主自愈 + 降权）
│   └── src/                      # 业务逻辑 + Web 层 + 数据访问层
└── free_models_manager/          # 桌面管理端 (Tauri + React)
    └── README.md                 # 快速启动与功能概览
```

### free_models_server（Rust 后端）

- 技术栈：actix-web 4 / SeaORM 2 (SQLite) / reqwest 0.12 / moka / tiktoken-rs / ed25519-dalek / AES-256-GCM
- 单 crate 结构，数据访问层通过 `src/db/` 模块实现：
  - `src/db/entities/` — SeaORM 实体定义（7 张表）
  - `src/db/impls/` — 各 Store 的 SeaORM 实现，通过 `Database` 结构体聚合
  - `src/db/types.rs` — DTO 类型（`UsageLogInsert`、`UsageLogStatItem` 等）
- 业务代码直接调用 `Database` 上的具体 store（如 `state.database.provider_configs.list()`），不引入 trait 抽象层

### free_models_manager（Tauri 桌面管理端）

- 技术栈：Tauri v2 / React 18 / TypeScript / LESS / Vite 5（UI 为项目自研的 "Doodle" 手绘组件库，不依赖 Ant Design）
- 管理后端服务的供应商（含凭证管理 + 测试 + 一键导入模型）、模型、API Key，并查看用量统计

## 核心能力

- **三协议** — OpenAI `/v1/chat/completions`、Anthropic `/v1/messages`、OpenAI Responses `/v1/responses`
- **优先级与故障切换** — 按 `priority` 排序，失败自动降级 + 熔断惩罚机制
- **进程内内存缓存** — 全部基于 moka，模型调度缓存支持 TTL 配置（`SCHEDULER_CACHE_TTL_SEC`）
- **Token 计数** — `tiktoken-rs` cl100k_base BPE，请求前估算 prompt token 做上下文窗口校验
- **用量日志** — 每次请求记录完整用量到 `usage_log` 表（含 cache hit/miss tokens），支持每日自动归档
- **凭证管理** — 一个供应商支持多组凭证（api_key / account / password），AES-256-GCM 加密存储
- **配额持久化** — 上游配额耗尽自动标记凭证为 `quota_exhausted` 并跳过该凭证，支持在管理端手动重置状态
- **响应透明化** — 透传上游响应头，不伪造 SSE 结束帧，保持与上游协议一致
- **Admin 接口** — Ed25519 签名鉴权，按职责拆分为多个子模块的完整 CRUD
- **SSRF 防护** — 上游 URL 校验拦截私网 IP，DNS 解析失败时 fail-closed

## 快速开始

### 后端服务

```bash
cd free_models_server
cp .env.example .env           # 至少设置 DATABASE_URL 和 ENCRYPTION_KEY

# 数据库：服务启动自动幂等建表，无需手动步骤；
# 从存量 MySQL 迁移数据（在仓库根目录运行；uv 按 PEP 723 注释自动装依赖，
# 脚本自动建表并导出 free_models.db，可用 --output 指定输出路径）：
# uv run migrate_mysql_to_sqlite.py --mysql-url mysql://user:pass@127.0.0.1:3306/free_models

cargo run --release
```

快速验证：

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

#### Docker 部署

> SQLite 是单文件数据库，**必须挂载卷持久化**，否则容器重建后数据丢失。

```bash
cd free_models_server

# 构建镜像
docker build -t free_models_server .

# 运行（容器专用配置 .env.docker + 数据卷 /data）
docker run -d \
  --name free_models_server \
  --restart unless-stopped \
  -p 8080:8080 \
  -v free_models_data:/data \
  --env-file .env.docker \
  free_models_server
```

镜像以 root 启动 `entrypoint.sh`，自动修正数据卷属主后降权为 `nobody` 运行（详见子包 README 的 Docker 章节）。

### 桌面管理端

```bash
cd free_models_manager
pnpm install
pnpm tauri dev            # 开发模式（桌面窗口 + HMR）
pnpm tauri build          # 生产构建
```

## 使用示例

```bash
# 列出模型
curl http://localhost:8080/v1/models \
  -H "Authorization: Bearer <API_KEY>"

# OpenAI 协议聊天
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","messages":[{"role":"user","content":"Hello"}]}'

# Anthropic 协议聊天
curl -X POST http://localhost:8080/v1/messages \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet","messages":[{"role":"user","content":"Hello"}],"max_tokens":1024}'

# Responses 协议（OpenAI 新一代接口）
curl -X POST http://localhost:8080/v1/responses \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","input":"Hello"}'
```

## 详细文档

| 文档 | 内容 |
|------|------|
| [架构总览](docx/architecture.md) | 系统架构、模块划分、技术栈、核心流程 |
| [API 端点](docx/api_endpoints.md) | 公共 & Admin API 完整参考 |
| [数据库表结构](docx/database_schema.md) | 表字段、约束、索引、关系 |
| [认证鉴权](docx/authentication.md) | API Key Bearer + Ed25519 签名鉴权流程 |
| [缓存策略](docx/caching_strategy.md) | 进程内内存缓存（moka）、模型调度缓存 TTL、刷新机制 |
| [代理转发与 Token 计算](docx/proxy_and_token.md) | 故障切换、流式转发、tiktoken |
| [模型代理全链路梳理](docx/model_proxy_chain.md) | 模型配置到上游调用的完整链路梳理 |
| [管理员界面](docx/admin_ui.md) | 页面功能、组件说明、签名流程 |
| [Server 快速启动](free_models_server/README.md) | 环境变量、配置详解、目录结构、Docker 部署 |
| [Manager 快速启动](free_models_manager/README.md) | 功能模块、开发指引 |
