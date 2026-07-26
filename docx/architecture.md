# 系统架构文档

## 项目概述

`free_models_token` 是一个统一的免费模型代理服务系统。它对外暴露类 OpenAI 与 Anthropic 兼容接口，按优先级和可用性将请求智能转发到多个上游 LLM 供应商。

项目采用双目录结构：后端代理服务（Rust） + Tauri 桌面管理端。

## 整体架构

```
                     ┌──────────────────────────────────┐
                     │   客户端 (OpenAI SDK / cURL 等)    │
                     └───────────────┬──────────────────┘
                                     │ Bearer Token
                                     ▼
┌─────────────────────────────────────────────────────────┐
│              free_models_server (Rust)                    │
│                                                          │
│  ┌──────────┐  ┌───────────┐  ┌──────────────────┐      │
│  │  Auth     │  │  Chat     │  │  Admin           │      │
│  │ Middleware │  │  Handler  │  │  Handler         │      │
│  └─────┬────┘  └─────┬─────┘  └───────┬──────────┘      │
│        │              │                │                 │
│   ┌────▼──────────────▼────────────────▼──────────────┐  │
│   │                  Service Layer                     │  │
│   │  model_service / provider_service / proxy_service  │  │
│   │  api_key_service / usage_log_service / ...         │  │
│   └────────────────────┬──────────────────────────────┘  │
│                        │ 通过 Arc<dyn StoreTrait> 调用   │
│   ┌────────────────────▼──────────────────────────────┐  │
│   │        Data Access Abstraction (三层 crate)        │  │
│   │                                                    │  │
│   │  free_models_store_api (接口层: trait + DTO)       │  │
│   │  └── free_models_store (实现层: SeaORM + Redis)   │  │
│   └───────────────────────────────────────────────────┘  │
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

## 三层 crate 架构

`free_models_server` 内部通过三个 crate 实现数据访问层的解耦：

```
free_models_server (二进制 crate)
    ├── depends on ── free_models_store_api (接口 crate)
    │                   ├── StoreError            — 统一错误类型
    │                   ├── CacheStore trait      — 缓存操作接口
    │                   ├── models/               — 7 个 DTO
    │                   └── stores/               — 7 个 Store trait + Database 聚合体
    │
    └── depends on ── free_models_store (实现 crate)
                        ├── cache.rs              — RedisManager（实现 CacheStore）
                        ├── entities/             — 7 个 SeaORM 实体
                        └── stores/               — 7 个 Store 实现
```

- **free_models_store_api**：纯接口 crate，仅依赖 `async-trait`、`serde`、`chrono`，无运行时依赖
- **free_models_store**：实现 crate，基于 SeaORM (MySQL) + Redis，业务层通过 trait 调用，不直接依赖数据库实现细节
- **free_models_server**：二进制 crate，业务层通过 `Arc<dyn StoreTrait>` 调用存储操作

## 目录结构

```
free_models_token/
├── README.md
├── docx/                              # 详细技术文档
│
├── free_models_server/                # 后端代理服务 (Rust)
│   ├── Cargo.toml                     # 主 crate 依赖
│   ├── Dockerfile
│   ├── .env.example
│   │
│   ├── free_models_store_api/         # 数据访问抽象接口
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # StoreError, CacheStore trait, 导出
│   │       ├── models/                # 7 个 DTO
│   │       └── stores/                # 7 个 Store trait + Database 结构体
│   │
│   ├── free_models_store/             # 数据访问实现
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                 # 导出 cache, entities, stores；init_db()
│   │       ├── cache.rs               # RedisManager（实现 CacheStore）
│   │       ├── entities/              # 7 个 SeaORM 实体
│   │       └── stores/                # 7 个 Store 实现 + build_database()
│   │
│   ├── migrations/
│   │   ├── 001_init.sql               # provider_config, model_config, api_key
│   │   ├── 002_add_model_config_columns.sql
│   │   ├── 003_add_admin_key_and_usage.sql
│   │   ├── 004_create_usage_log_daily.sql
│   │   └── 005_create_provider_model_map.sql
│   │
│   └── src/
│       ├── main.rs                    # 入口、路由注册
│       ├── app.rs                     # AppState 定义 + HTTP Client 构建
│       ├── config.rs                  # Config 结构体（环境变量读取）
│       ├── response.rs                # 统一错误响应（OpenAI + Anthropic 格式）
│       ├── task.rs                    # 优雅关闭 + 每日归档定时任务
│       │
│       ├── db/
│       │   └── mod.rs                 # 构建 Database（委托到 free_models_store）
│       │
│       ├── handler/
│       │   ├── mod.rs
│       │   ├── chat_handler.rs        # 公共请求（模型查询→筛选→转发）
│       │   └── admin_handler.rs       # Admin CRUD（委托到 service 层）
│       │
│       ├── middleware/
│       │   ├── mod.rs
│       │   ├── auth.rs                # Bearer Token 鉴权
│       │   └── admin_auth.rs          # Ed25519 签名鉴权
│       │
│       ├── service/
│       │   ├── mod.rs
│       │   ├── provider_service.rs            # 供应商 CRUD
│       │   ├── model_service_ext.rs           # 模型 CRUD + 统计
│       │   ├── model_service.rs               # 模型查询 + 缓存 + 优先级排序
│       │   ├── api_key_service.rs             # API Key CRUD + 自动生成
│       │   ├── admin_key_service.rs           # SSH 公钥管理 + fingerprint 自动填充
│       │   ├── provider_credential_service.rs # 凭证 CRUD + AES 加解密
│       │   ├── provider_model_map_service.rs  # 供应商模型映射管理
│       │   ├── proxy_service.rs              # 上游转发 + 故障切换 + SSE + 用量日志
│       │   └── usage_log_service.rs          # 用量持久化
│       │
│       └── util/
│           ├── mod.rs
│           ├── api_key_cache.rs       # Redis Set + 内存 fallback
│           ├── cache_affinity.rs      # 模型-供应商缓存亲和性
│           ├── encryption.rs          # AES-256-GCM 加密/解密
│           ├── model_scheduler.rs     # 模型调度缓存（SchedulerCache）
│           ├── penalty.rs             # 优先级惩罚机制（熔断器）
│           ├── proxy_ssrf.rs          # SSRF 防护
│           ├── proxy_types.rs         # 代理转发类型定义
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
    │   │   └── Toolbar.tsx/less
    │   ├── pages/
    │   │   ├── Overview.tsx/less      # 概览仪表盘
    │   │   ├── Stats.tsx/less         # 使用统计
    │   │   ├── Providers.tsx/less     # 供应商管理
    │   │   ├── Models.tsx/less        # 模型管理
    │   │   ├── ApiKeys.tsx/less       # API Key 管理
    │   │   └── Settings.tsx/less      # 设置页
    │   └── styles/                    # 全局样式
    │       ├── global.less
    │       └── variables.less
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
| ORM | sea-orm 2 (MySQL) | 数据库访问 |
| HTTP 客户端 | reqwest 0.12 | 上游请求转发 |
| 缓存 | Redis + 内存 fallback | 多级缓存 |
| Token 计数 | tiktoken-rs 0.12 | cl100k_base BPE |
| 加密 | aes-gcm 0.10 | 凭证 AES-256-GCM 加密 |
| 签名 | ed25519-dalek 3 | Admin API 签名鉴权 |
| 数据访问层 | free_models_store_api + free_models_store | 接口-实现分离架构 |

### 前端 (free_models_manager)

| 组件 | 技术 | 用途 |
|------|------|------|
| 桌面框架 | Tauri v2 | 跨平台桌面应用 |
| UI 框架 | React 18 + TypeScript | 前端界面 |
| 构建工具 | Vite | 前端构建 |
| UI 组件 | Ant Design 5 | 组件库 |
| 签名 | Tauri Rust 后端 (ed25519-dalek) | Ed25519 签名生成 |

## 核心流程

### 请求处理流程

```
客户端请求 (POST /v1/chat/completions 或 /v1/messages)
    │
    ├─ AuthMiddleware: 提取 Bearer Token → 查 API Key 缓存
    │   ├─ 无效 → 401
    │   └─ 有效 → 继续
    │
    ├─ chat_handler::chat_completions / anthropic_messages
    │   ├─ 解析模型名
    │   └─ 校验 messages 字段
    │
    ├─ model_service::get_all_available_models_by_priority
    │   ├─ 查 ModelCache → 未命中 → 查库
    │   ├─ 过滤 status='available'
    │   ├─ 过滤有活跃凭证的供应商
    │   └─ 按 priority 升序排列
    │
    ├─ 按协议筛选（openai / anthropic）
    ├─ 用户指定模型时，指定模型排到最前
    ├─ CircuitBreaker::sort_penalized_last
    │   └─ 被惩罚的模型排到最后
    │
    ├─ 估算 prompt token → 过滤 context_window 不足的模型
    │
    └─ proxy_service::proxy_chat_completion
        └─ 遍历模型列表
            ├─ forward_to_provider: 构建 URL、设置 Header、发送请求
            ├─ 成功 → 返回上游响应（流式/非流式），记录用量日志
            ├─ 失败 → CircuitBreaker::penalize → 继续下一个
            └─ 全部失败 → 503
```

### 启动流程

```
1. 加载 .env 环境变量（dotenv）
2. 初始化 env_logger
3. 读取 Config（server_host, server_port, encryption_key 等）
4. 初始化数据库：db::build_database()
   └─ 委托到 free_models_store::build_database()
       └─ 创建 7 个 Store 实现 + Database 聚合结构体
5. 初始化 Redis：free_models_store::cache::RedisManager::init()
   └─ 失败时使用 disabled() 空实现，服务正常启动
6. 构建 HTTP 客户端（连接池 20、空闲超时 90s、TCP keepalive 30s）
7. 加载 API Key 全量缓存（Redis Set + 内存 HashSet）
8. 创建 CircuitBreaker（优先级惩罚机制）
9. 创建 SchedulerCache（模型调度缓存）
10. 构造 AppState 注入 Actix Web
11. 初始化 UsageLogCollector（异步用量日志收集器）
12. 归档昨天的用量日志（archive_yesterday）
13. 注册路由，启动 HTTP 服务
14. spawn 后台任务：
    ├─ task::graceful_shutdown — 监听 SIGTERM/Ctrl-C，优雅关闭
    └─ task::spawn_daily_archive — 每日凌晨归档前一天用量日志
```

## 全局状态 (AppState)

```rust
pub struct AppState {
    pub database: Database,                     // 7 个 Store trait 的聚合体（Arc<dyn ...>）
    pub scheduler_cache: SchedulerCache,        // 模型调度缓存（Redis + 内存）
    pub client: Client,                         // reqwest HTTP 客户端
    pub priority_penalty: web::Data<CircuitBreaker>,  // 熔断器（失败计数 + 惩罚 TTL）
    pub api_key_cache: ApiKeyCache,             // API Key 全量缓存
    pub cache_affinity: CacheAffinity,          // 模型-供应商缓存亲和性
    pub encryption_key: [u8; 32],               // AES-256-GCM 加密密钥
}
```

其中 `Database` 结构体（定义在 free_models_store_api）聚合了 7 个 Store trait：

```rust
pub struct Database {
    pub providers: Arc<dyn ProviderConfigStore>,
    pub models: Arc<dyn ModelConfigStore>,
    pub credentials: Arc<dyn ProviderCredentialStore>,
    pub model_maps: Arc<dyn ProviderModelMapStore>,
    pub api_keys: Arc<dyn ApiKeyStore>,
    pub admin_keys: Arc<dyn AdminKeyStore>,
    pub usage_logs: Arc<dyn UsageLogStore>,
}
```
