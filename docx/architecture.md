# 系统架构文档

## 项目概述

`free_models_token` 是一个统一的免费模型代理服务系统。它对外暴露类 OpenAI（chat/completions 与 responses）、Anthropic（messages）兼容接口，按优先级和可用性将请求智能转发到多个上游 LLM 供应商。

项目采用双目录结构：后端代理服务（Rust）+ Tauri 桌面管理端。

## 整体架构

```
                     ┌──────────────────────────────────┐
                     │   客户端 (OpenAI/Anthropic SDK)   │
                     └───────────────┬──────────────────┘
                                     │ Bearer Token
                                     ▼
┌─────────────────────────────────────────────────────────┐
│              free_models_server (Rust 单 crate)          │
│                                                          │
│  ┌──────────┐  ┌─────────────┐  ┌──────────────────┐    │
│  │  Auth     │  │  Chat       │  │  Admin           │    │
│  │ Middleware │  │  Handler    │  │  Handler (多文件)│    │
│  └─────┬────┘  └─────┬───────┘  └───────┬──────────┘    │
│        │              │                  │               │
│   ┌────▼──────────────▼──────────────────▼────────────┐  │
│   │                Service 层                          │  │
│   │      proxy_service / provider_credential_service   │  │
│   └────────────────────┬──────────────────────────────┘  │
│                        │ 直接调用具体 Store 结构体        │
│   ┌────────────────────▼──────────────────────────────┐  │
│   │     数据访问层（单 crate 模块化）                    │  │
│   │  src/db: entities/ (7 实体) + impls/ (7 Store)     │  │
│   │         + types.rs + cache.rs (RedisManager)      │  │
│   └───────────────────────────────────────────────────┘  │
│                                                          │
│   util/：SchedulerCache · CircuitBreaker · CacheAffinity │
│            · SsrfChecker · ApiKeyCache                   │
└──────────────────────────────────────────────────────────┘
                        │ HTTP 转发 (携带上游 API Key)
                        ▼
              ┌─────────────────────┐
              │ 上游 LLM 供应商      │
              │ (OpenAI/Anthropic等)│
              └─────────────────────┘
```

同时，Tauri 桌面管理端通过 Ed25519 签名调用 Admin API：

```
┌────────────────────────────────────────────┐
│     free_models_manager (Tauri + React)     │
│                                             │
│  ┌──────────┐ ┌──────────┐ ┌───────────┐   │
│  │ Overview │ │ Providers│ │  Models    │   │
│  └──────────┘ └──────────┘ └───────────┘   │
│  ┌──────────┐ ┌──────────┐ ┌───────────┐   │
│  │ ApiKeys  │ │  Stats   │ │  Settings  │   │
│  └──────────┘ └──────────┘ └───────────┘   │
└─────────────────────┬──────────────────────┘
                      │ Ed25519 签名
                      ▼
              free_models_server /admin/*
```

## 数据访问层

数据访问层采用**单 crate 模块化**设计，所有数据库访问代码都位于 `free_models_server/src/db/`，不存在独立的接口 crate / 实现 crate 划分：

```
src/db/
    ├── mod.rs        — StoreError 统一错误类型、Database 聚合结构体、init_db / build_database
    ├── types.rs      — 跨模块共享的 DTO（UsageLogInsert、UsageLogStatItem 等）
    ├── entities/     — 7 个 SeaORM 实体（映射数据库表）
    └── impls/        — 7 个 Store 结构体实现（业务代码直接调用具体 Store）
```

- **entities/**：7 个 SeaORM 实体 —— `admin_key`、`api_key`、`model_config`、`provider_config`、`provider_credential`、`provider_model_map`、`usage_log`
- **impls/**：7 个对应 Store 实现 —— `AdminKeyStoreSeaorm`、`ApiKeyStoreSeaorm`、`ModelConfigStoreSeaorm`、`ProviderConfigStoreSeaorm`、`ProviderCredentialStoreSeaorm`、`ProviderModelMapStoreSeaorm`、`UsageLogStoreSeaorm`
- **types.rs**：跨模块共享的 DTO，如 `UsageLogInsert`、`UsageLogStatItem`、`UsageLogStatsResponse`
- 业务代码（handler / service / util）直接实例化并调用具体的 Store 结构体，**不使用** `Arc<dyn StoreTrait>` 动态分发，也没有 trait 接口层
- Redis 操作由独立的 [cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/cache.rs)（`RedisManager`）负责，Redis 不可用时透明降级
- `Database` 聚合结构体在 `build_database()` 中统一构建，包含 7 个 Store，均基于同一个 SeaORM `DatabaseConnection`

**模型可用性三重校验**：一个模型被视为可用，必须同时满足 `model_config.is_active = true`、存在对应的 `provider_model_map` 映射、且该映射对应的 provider 下存在 `provider_credential.is_active = true` 的活跃凭证。`schedule_all_available` 与 `get_all_model_names` 均按此规则筛选。

## 目录结构

```
free_models_token/
├── README.md
├── docx/                              # 详细技术文档
│
├── free_models_server/                # 后端代理服务 (Rust)
│   ├── Cargo.toml                     # 单 crate 依赖
│   ├── Dockerfile
│   ├── .env.example
│   ├── migrations/                    # 手动 SQL 迁移（按序号顺序执行 001-006）
│   │   ├── 001_init.sql               # provider_config, model_config, api_key
│   │   ├── 002_add_model_config_columns.sql
│   │   ├── 003_add_admin_key_and_usage.sql
│   │   ├── 004_create_usage_log_daily.sql
│   │   ├── 005_create_provider_model_map.sql
│   │   └── 006_add_credential_status_fields.sql
│   │
│   └── src/
│       ├── main.rs                    # 入口、路由注册、启动流程
│       ├── app.rs                     # AppState 定义 + HTTP Client 构建
│       ├── cache.rs                   # RedisManager（可选 Redis，透明降级）
│       ├── config.rs                  # Config 结构体（环境变量读取）
│       ├── response.rs                # 统一错误响应（OpenAI + Anthropic + Responses 格式）
│       ├── task.rs                    # 优雅关闭 + 每日归档定时任务
│       │
│       ├── db/                        # 数据访问层（单 crate 模块化）
│       │   ├── mod.rs                 # StoreError、Database、init_db、build_database
│       │   ├── types.rs               # 跨模块共享 DTO
│       │   ├── entities/              # 7 个 SeaORM 实体
│       │   │   ├── admin_key.rs
│       │   │   ├── api_key.rs
│       │   │   ├── model_config.rs
│       │   │   ├── provider_config.rs
│       │   │   ├── provider_credential.rs
│       │   │   ├── provider_model_map.rs
│       │   │   └── usage_log.rs
│       │   └── impls/                 # 7 个 Store 实现
│       │       ├── admin_key.rs
│       │       ├── api_key.rs
│       │       ├── model_config.rs
│       │       ├── provider_config.rs
│       │       ├── provider_credential.rs
│       │       ├── provider_model_map.rs
│       │       └── usage_log.rs
│       │
│       ├── handler/
│       │   ├── mod.rs
│       │   ├── chat_handler.rs        # 公共请求（鉴权解析→协议校验→调度→转发）
│       │   └── admin/                 # Admin 路由（/admin scope）
│       │       ├── mod.rs             # admin_routes() 路由清单
│       │       ├── api_key.rs
│       │       ├── import_models.rs
│       │       ├── model.rs
│       │       ├── provider.rs
│       │       ├── provider_credential.rs
│       │       ├── provider_model_map.rs
│       │       ├── stats.rs           # service/status、cache/refresh、usage_log/stats
│       │       └── test_credential.rs
│       │
│       ├── middleware/
│       │   ├── mod.rs
│       │   ├── auth.rs                # Bearer Token 鉴权（跳过 /health 与 /admin/*）
│       │   └── admin_auth.rs          # Ed25519 签名鉴权（时间戳 + nonce + body hash）
│       │
│       ├── service/
│       │   ├── mod.rs
│       │   ├── provider_credential_service.rs # 凭证创建/更新时的 AES-256-GCM 加解密
│       │   └── proxy_service.rs       # 上游转发 + 故障切换 + SSE 扫描 + 用量日志
│       │
│       └── util/
│           ├── mod.rs
│           ├── api_key_cache.rs       # API Key 缓存（Redis Set + moka fallback）
│           ├── cache_affinity.rs      # 模型-供应商缓存亲和性（moka）
│           ├── encryption.rs          # AES-256-GCM 加密/解密 + parse_key
│           ├── model_scheduler.rs     # 模型调度缓存（SchedulerCache）
│           ├── penalty.rs             # 熔断器（CircuitBreaker，纯内存 moka）
│           ├── proxy_ssrf.rs          # SSRF 防护（SsrfChecker，moka 缓存）
│           ├── proxy_types.rs         # 代理转发类型（Protocol、ForwardMeta 等）
│           ├── stream_usage_scanner.rs # SSE 流扫描（usage/error 提取）
│           ├── tokenizer.rs           # cl100k_base BPE token 估算
│           └── usage_log_collector.rs # 异步用量日志收集器
│
└── free_models_manager/               # 桌面管理端 (Tauri v2 + React)
    ├── package.json
    ├── src/
    │   ├── App.tsx                    # 应用根组件 + 路由
    │   ├── main.tsx                   # 入口
    │   ├── types/index.ts             # TypeScript 类型定义
    │   ├── components/                # 通用 UI 组件
    │   │   ├── Drawer.tsx/less
    │   │   ├── Sidebar.tsx/less
    │   │   ├── Toggle.tsx/less
    │   │   ├── Toolbar.tsx/less
    │   │   └── doodle/                # 手绘风 UI 组件库
    │   │       ├── index.ts
    │   │       ├── DoodleButton.tsx/less
    │   │       ├── DoodleCheckbox.tsx/less
    │   │       ├── DoodleCheckboxGroup.tsx/less
    │   │       ├── DoodleEmpty.tsx/less
    │   │       ├── DoodleInput.tsx/less
    │   │       ├── DoodleMessage.tsx/less
    │   │       ├── DoodleModal.tsx/less
    │   │       ├── DoodleSelect.tsx/less
    │   │       └── DoodleTag.tsx/less
    │   ├── pages/
    │   │   ├── Overview.tsx/less      # 概览仪表盘
    │   │   ├── Stats.tsx/less         # 使用统计
    │   │   ├── Providers.tsx/less     # 供应商管理
    │   │   ├── Models.tsx/less        # 模型管理
    │   │   ├── ApiKeys.tsx/less       # API Key 管理
    │   │   └── Settings.tsx/less      # 设置页
    │   ├── styles/                    # 全局样式
    │   │   ├── global.less
    │   │   ├── variables.less
    │   │   └── cjk-fonts.less
    │   └── assets/fonts/              # 内置中文字体
    └── src-tauri/
        └── src/
            ├── main.rs                # Tauri 入口 + 所有 Tauri 命令
            ├── api.rs                 # AdminClient 封装的 HTTP 请求
            └── crypto.rs              # Ed25519 密钥对加载 + 签名生成
```

## 技术栈

### 后端 (free_models_server)

| 组件 | 技术 | 用途 |
|------|------|------|
| HTTP 框架 | actix-web 4 | REST API 服务 |
| ORM | sea-orm 2 (MySQL) | 数据库访问（SeaORM 实体 + impls Store） |
| HTTP 客户端 | reqwest 0.12 | 上游请求转发（统一 `User-Agent: FreeModelsServer/1.0`） |
| 缓存 | Redis（可选）+ moka 内存 | 调度缓存、API Key 缓存、熔断、亲和、SSRF 校验 |
| Token 计数 | tiktoken-rs | cl100k_base BPE |
| 加密 | aes-gcm | 凭证 AES-256-GCM 加密 |
| 签名 | ed25519-dalek | Admin API 签名鉴权 |
| 数据访问层 | src/db 单 crate（entities + impls + types + cache） | SeaORM 2 实体与 Store 实现 |
| 熔断器 | 纯内存 moka | 失败计数 → Open → HalfOpen 单探测 |
| SSRF 防护 | 私网 IP 拦截 + DNS fail-closed | 拦截私网地址，带 moka 缓存（TTL 300s） |
| 流式扫描 | SSE 解析器 | 流式响应中提取 usage 与内嵌错误 |

### 前端 (free_models_manager)

| 组件 | 技术 | 用途 |
|------|------|------|
| 桌面框架 | Tauri v2 | 跨平台桌面应用 |
| UI 框架 | React 18 + TypeScript | 前端界面 |
| 构建工具 | Vite 5 + LESS | 前端构建 |
| UI 组件 | Ant Design 6 | 组件库 |
| 签名 | Tauri Rust 后端 (ed25519-dalek) | Ed25519 签名生成 |

## 核心流程

### 请求处理流程

```
客户端请求 (POST /v1/chat/completions | /v1/messages | /v1/responses)
    │
    ├─ AuthMiddleware: 提取 Bearer Token → ApiKeyCache.contains() 查询
    │   ├─ 无效 → 401（Anthropic 协议返回 anthropic 格式）
    │   └─ 有效 → 继续
    │
    ├─ dispatch_chat: 解析 API Key（DB 查询 id/name）→ handle_chat_request
    │
    ├─ validate_request_body
    │   ├─ Responses 协议 → 校验 input 字段
    │   └─ 其他协议 → 校验 messages 字段
    │
    ├─ schedule_all_available(database, encryption_key, scheduler_cache)
    │   ├─ 查 SchedulerCache（Redis app:free_models:scheduler:all + moka，TTL 30s）
    │   ├─ 未命中 → 批量查库（活跃 model_config / map / provider / credential）
    │   └─ 构造 Vec<ModelScheduleInfo>（模型→映射→凭证 三级结构）并写回缓存
    │
    ├─ supports_protocol: 过滤不支持当前协议的模型（空则 503）
    ├─ merge_preferred_first: 请求指定 model 排到最前
    ├─ tokenizer::estimate_prompt_tokens: 估算 prompt tokens
    ├─ filter_by_context_window: 过滤上下文窗口不足的模型（空则 400）
    │
    └─ proxy_chat_completion_inner（三重循环 model → map → credential）
        ├─ reorder_by_affinity: 按 CacheAffinity 亲和记录重排 provider/credential
        ├─ 跳过 quota_exhausted 凭证
        ├─ circuit_breaker.is_allowed 熔断检查（Closed / Open / HalfOpen）
        ├─ forward_to_provider
        │   ├─ SSRF validate_url_safe（拦截私网 IP，DNS 解析失败 fail-closed）
        │   ├─ join_url + 改写 model + stream / stream_options
        │   └─ 协议差异化鉴权头（OpenAI/Responses: Bearer；Anthropic: x-api-key）
        ├─ ForwardOutcome 四态: Success / Retry(Option<u64>) / Fail / CredentialFail
        ├─ Retry → 按 retry_delay（Retry-After 封顶 15s，否则 500-900ms）睡后重试一次
        │   └─ 重试后仍 Retry/Fail → record_failure；重试后 CredentialFail → record_credential_fail
        ├─ CredentialFail → record_credential_fail（熔断 + 持久化 quota_exhausted）
        ├─ Success → handle_success（流式: SSE 扫描 usage/error；非流式: 解析 usage）
        ├─ Fail → 立即返回 400（Upstream provider error）
        └─ 全部尝试失败 → 503（汇总 error_details）
```

### 启动流程

```
1. dotenv::dotenv() 加载 .env
2. env_logger::init() 初始化日志
3. Config::from_env() 读取环境变量
4. db::build_database()
   └─ init_db() 创建 SeaORM 连接（DB_MAX_CONNECTIONS）→ 构建 7 个 Store + Database 聚合
5. 克隆 usage_logs Store 供日志收集器与归档使用
6. cache::RedisManager::init()
   └─ REDIS_ENABLED=false 或连接失败 → client=None，透明降级到内存
7. ApiKeyCache::load_all() 全量加载活跃 API Key（moka + 同步 Redis Set）
8. app::build_client() 构建 HTTP 客户端
   └─ 连接池 20、空闲超时 90s、TCP keepalive 30s、User-Agent: FreeModelsServer/1.0
9. CircuitBreaker::new(open_ttl, threshold, max_capacity)
10. CacheAffinity::new(max_capacity, ttl)
11. SsrfChecker::new(max_capacity, ttl=300s)
12. 构造 AppState 注入 Actix Web（PayloadConfig 10MB）
13. init_usage_log_collector() 初始化异步用量日志收集器
14. usage_log_store.archive_yesterday() 归档昨天的用量日志
15. 注册路由并启动 HTTP 服务
    ├─ wrap(AuthMiddleware) + Logger
    ├─ /health、/v1/models、/v1/chat/completions、/v1/messages、/v1/responses
    └─ /admin scope（AdminAuthMiddleware 包裹）
16. spawn 后台任务：
    ├─ task::graceful_shutdown — SIGTERM/Ctrl-C → 优雅关闭（30s 超时）+ 刷新日志收集器
    └─ task::spawn_daily_archive — 每日 00:05 UTC 归档前一天用量日志（指数退避重试）
```

## 全局状态 (AppState)

```rust
pub struct AppState {
    pub database: Database,                          // 7 个 Store 的聚合体
    pub scheduler_cache: SchedulerCache,             // 模型调度缓存（Redis + moka）
    pub client: Client,                              // reqwest HTTP 客户端
    pub priority_penalty: web::Data<CircuitBreaker>, // 熔断器（纯内存 moka）
    pub api_key_cache: ApiKeyCache,                  // API Key 全量缓存
    pub cache_affinity: CacheAffinity,               // 模型-供应商缓存亲和性（moka）
    pub ssrf_checker: SsrfChecker,                   // SSRF 校验（moka 缓存）
    pub encryption_key: [u8; 32],                    // AES-256-GCM 加密密钥
}
```

其中 `Database` 结构体（定义在 [db/mod.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/db/mod.rs)）聚合了 7 个具体 Store：

```rust
pub struct Database {
    pub provider_configs: ProviderConfigStoreSeaorm,
    pub model_configs: ModelConfigStoreSeaorm,
    pub provider_credentials: ProviderCredentialStoreSeaorm,
    pub provider_model_maps: ProviderModelMapStoreSeaorm,
    pub api_keys: ApiKeyStoreSeaorm,
    pub admin_keys: AdminKeyStoreSeaorm,
    pub usage_logs: UsageLogStoreSeaorm,
}
```

## Admin API 鉴权

`/admin/*` 路由统一包裹 `AdminAuthMiddleware`，基于 Ed25519 签名：

- 请求头：`X-Admin-Fingerprint`、`X-Admin-Timestamp`、`X-Admin-Signature`（可选 `X-Admin-Nonce` 防重放、`X-Admin-Body-Hash` 保护请求体）
- 签名内容：`METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH`（nonce 与 body hash 为空字符串时占位为空）
- 时间戳允许 ±300 秒偏差，nonce 在 300 秒窗口内去重
- 公钥以 OpenSSH 格式（`ssh-ed25519`）存储在 `admin_key` 表，fingerprint 用于快速定位记录
