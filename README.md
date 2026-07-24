# free_models_token

统一的免费模型代理服务，对外暴露类 OpenAI 与 Anthropic 接口，按优先级和可用性将请求转发到多个上游供应商。

项目采用双目录结构：后端代理服务 + Tauri 桌面管理端。

```
free_models_token/
├── free_models_server/    # 后端代理服务（Rust + actix-web + Sea-ORM）
│   ├── src/               # Rust 源码
│   ├── migrations/        # 数据库迁移 SQL
│   ├── Cargo.toml
│   └── Dockerfile
└── free_models_manager/   # 本地桌面管理端（Tauri + React + TS + LESS）
    ├── src/               # 前端源码
    ├── src-tauri/         # Tauri Rust 后端
    ├── package.json
    └── vite.config.ts
```

## free_models_server

后端代理服务，核心能力：

- **双协议支持**：OpenAI (`/v1/chat/completions`) 与 Anthropic (`/v1/messages`)
- **按模型配置协议**：`model_config.protocols` 字段标识模型支持的协议，请求时自动匹配
- **优先级与故障切换**：按 `priority` 排序，失败自动降级，带惩罚机制
- **多级缓存**：基于 Redis 的模型/供应商/API Key 缓存，支持 Redis 不可用时自动降级到内存缓存
- **Admin 管理接口**：SSH Ed25519 公钥签名鉴权，提供 providers / models / api_keys 的完整 CRUD

技术栈：Rust / actix-web 4 / Sea-ORM 1（MySQL）/ reqwest 0.12

**详细文档**：[free_models_server/README.md](./free_models_server/README.md)

### 快速启动

```bash
cd free_models_server

# 1. 创建 .env 并配置 DATABASE_URL
cp .env.example .env

# 2. 执行数据库迁移（手动执行 SQL）
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key.sql

# 3. 编译并运行
cargo run --release
```

### Docker 构建

```bash
cd free_models_server
docker build -t free_models_server .
docker run --rm -p 8080:8080 --env-file .env free_models_server
```

## free_models_manager

基于 Tauri 的本地桌面管理应用，用于可视化管理模型配置、供应商配置、API Key 等。

**技术栈**：Rust / Tauri v2 / React / TypeScript / LESS / Vite

**UI 风格**：Ant Design 风格深色主题，侧边栏 + 密集表格布局

### 功能模块

| 模块 | 功能 |
|------|------|
| 概览 | 服务健康状态、各类资源的激活/总数统计（彩色分类显示）、刷新缓存 |
| 供应商 | 供应商 CRUD（名称、Base URL、API Key、启停）、一键导入模型 |
| 模型 | 模型 CRUD（名称、Model ID、协议标签、优先级、超时、启停）、原生表格+内置筛选 |
| API Keys | API Key CRUD（自动生成 `fm-` 前缀 64 位随机密钥、脱敏显示、一键复制、启停） |
| 设置 | 后端服务地址绑定、SSH 公钥 Fingerprint 展示、保存密钥 |

### Admin 接口鉴权机制

管理端通过 **SSH Ed25519 公钥签名** 鉴权调用后端 Admin 接口：

1. 管理端首次启动时自动生成 Ed25519 密钥对，私钥存储于 Tauri 配置目录
2. 每个 Admin API 请求在 Header 中携带：
   - `X-Admin-Fingerprint` — 公钥 SHA256 fingerprint
   - `X-Admin-Timestamp` — 当前 Unix 时间戳（±300 秒防重放）
   - `X-Admin-Signature` — 对 `METHOD:PATH:TIMESTAMP` 的 Ed25519 签名（Base64）
3. 服务端按 fingerprint 查 `admin_key` 表，验证公钥存在且激活，验证签名和时间戳

### 使用方式

1. 启动 free_models_server
2. 在管理端设置页获取 SSH 公钥 fingerprint
3. 将公钥添加到服务端 `admin_key` 表
4. 管理端即可调用 Admin 接口

### 开发启动

```bash
cd free_models_manager

# 安装前端依赖
pnpm install

# 启动开发模式（同时启动前端和 Tauri）
pnpm tauri dev

# 构建生产版本
pnpm tauri build
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
| `max_output` | INT | 最大输出 token 数 |
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
