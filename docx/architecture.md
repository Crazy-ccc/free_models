# 系统架构文档

## 项目概述

`free_models_token` 是一个统一的免费模型代理服务系统。它对外暴露类 OpenAI 与 Anthropic 兼容接口，按优先级和可用性将请求智能转发到多个上游 LLM 供应商。

项目采用双目录结构：后端代理服务（Rust） + Tauri 桌面管理端。

## 整体架构

```
┌─────────────────────────────────────────────────┐
│                  客户端 / 用户                     │
│       (OpenAI SDK / Anthropic SDK / cURL)        │
└──────────────────┬──────────────────────────────┘
                   │
                   │ HTTP 请求 (Bearer Token 鉴权)
                   ▼
┌─────────────────────────────────────────────────┐
│            free_models_server (Rust)              │
│                                                   │
│  ┌─────────┐  ┌──────────┐  ┌────────────────┐   │
│  │  Auth    │  │  Chat    │  │  Admin          │   │
│  │ Middleware│  │  Handler │  │  Handler        │   │
│  └────┬────┘  └────┬─────┘  └───────┬─────────┘   │
│       │            │                │              │
│  ┌────▼────────────▼────────────────▼──────────┐   │
│  │              Service Layer                   │   │
│  │  model_service / provider_service             │   │
│  │  api_key_service / usage_log_service          │   │
│  │  proxy_service / model_service_ext            │   │
│  └────────────────┬─────────────────────────────┘   │
│                   │                                 │
│  ┌────────────────▼─────────────────────────────┐   │
│  │               Data Layer                      │   │
│  │  Sea-ORM (MySQL) / Redis / 内存缓存            │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
                   │
                   │ HTTP 请求转发 (携带 API Key)
                   ▼
┌─────────────────────────────────────────────────┐
│          上游 LLM 供应商                          │
│  (OpenAI / Anthropic / 兼容代理等)                │
└─────────────────────────────────────────────────┘
```

同时，Tauri 桌面管理端通过 Admin API 管理配置：

```
┌───────────────────────────────────────────┐
│      free_models_manager (Tauri + React)   │
│                                           │
│  ┌──────────┐ ┌──────────┐ ┌───────────┐  │
│  │ Overview │ │ Providers│ │ Models     │  │
│  └──────────┘ └──────────┘ └───────────┘  │
│  ┌──────────┐ ┌──────────┐               │
│  │ ApiKeys  │ │ Settings │               │
│  └──────────┘ └──────────┘               │
└──────────────────┬────────────────────────┘
                   │
                   │ Admin API (Ed25519 签名鉴权)
                   ▼
           free_models_server /admin/*
```

## 目录结构

```
free_models_token/
├── free_models_server/          # 后端代理服务 (Rust)
│   ├── src/
│   │   ├── main.rs              # 应用入口、全局状态初始化、路由注册
│   │   ├── config/mod.rs        # 环境变量配置
│   │   ├── error.rs             # 统一错误响应（OpenAI + Anthropic 格式）
│   │   ├── db/
│   │   │   ├── mod.rs           # 数据库连接初始化
│   │   │   ├── redis.rs         # Redis 管理器
│   │   │   └── entities/        # ORM 实体（6 张表）
│   │   │       ├── mod.rs
│   │   │       ├── admin_key.rs
│   │   │       ├── api_key.rs
│   │   │       ├── model_config.rs
│   │   │       ├── provider_config.rs
│   │   │       ├── provider_credential.rs
│   │   │       └── usage_log.rs
│   │   ├── handler/
│   │   │   ├── mod.rs
│   │   │   ├── admin_handler.rs # Admin CRUD 处理器
│   │   │   └── chat_handler.rs  # 公共请求处理器（模型查询→筛选→转发）
│   │   ├── middleware/
│   │   │   ├── mod.rs
│   │   │   ├── auth.rs          # Bearer Token 鉴权中间件
│   │   │   └── admin_auth.rs    # Ed25519 签名鉴权中间件
│   │   ├── service/
│   │   │   ├── mod.rs
│   │   │   ├── admin_key_service.rs       # 管理员密钥指纹自动填充
│   │   │   ├── api_key_service.rs         # API Key CRUD + 自动生成
│   │   │   ├── model_service.rs           # 模型缓存 + 供应商缓存 + 模型查询
│   │   │   ├── model_service_ext.rs       # 模型 CRUD（扩展）
│   │   │   ├── provider_service.rs        # 供应商 CRUD
│   │   │   ├── provider_credential_service.rs  # 凭证 CRUD（含 AES 加解密）
│   │   │   ├── proxy_service.rs           # 代理转发核心（故障切换 + 用量日志）
│   │   │   └── usage_log_service.rs       # 用量日志写入
│   │   └── util/
│   │       ├── mod.rs
│   │       ├── api_key_cache.rs  # API Key 全量缓存（Redis Set + 内存）
│   │       ├── crud.rs           # 通用 CRUD 辅助函数（count）
│   │       ├── encryption.rs     # AES-256-GCM 加密/解密
│   │       ├── penalty.rs        # 优先级惩罚机制
│   │       └── tokenizer.rs      # tiktoken BPE 分词器
│   ├── migrations/               # SQL 迁移文件（3 个）
│   ├── Cargo.toml
│   └── Dockerfile
│
└── free_models_manager/          # 桌面管理端 (Tauri v2 + React)
    ├── src/
    │   ├── App.tsx
    │   ├── main.tsx
    │   ├── components/           # 通用组件
    │   │   ├── Drawer.tsx/less
    │   │   ├── Sidebar.tsx/less
    │   │   ├── Toggle.tsx/less
    │   │   └── Toolbar.tsx/less
    │   ├── pages/                # 页面
    │   │   ├── Overview.tsx/less    # 概览仪表盘
    │   │   ├── Providers.tsx/less   # 供应商管理
    │   │   ├── Models.tsx/less      # 模型管理
    │   │   ├── ApiKeys.tsx/less     # API Key 管理
    │   │   └── Settings.tsx/less    # 设置页
    │   ├── styles/               # 全局样式
    │   │   ├── global.less
    │   │   └── variables.less
    │   └── types/index.ts        # TypeScript 类型定义
    ├── src-tauri/
    │   └── src/
    │       ├── main.rs           # Tauri 后端入口
    │       ├── api.rs            # Tauri 命令 / 网络请求封装
    │       └── crypto.rs         # 签名/加密工具
    └── package.json
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

### 前端 (free_models_manager)

| 组件 | 技术 | 用途 |
|------|------|------|
| 桌面框架 | Tauri v2 | 跨平台桌面应用 |
| UI 框架 | React 18 + TypeScript | 前端界面 |
| 构建工具 | Vite | 前端构建 |
| UI 组件 | Ant Design 5 | 组件库 |
| HTTP 客户端 | fetch | API 调用 |
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
    ├─ priority_penalty::sort_penalized_last
    │   └─ 被惩罚的模型排到最后
    │
    ├─ 估算 prompt token → 过滤 context_window 不足的模型
    │
    └─ proxy_service::proxy_chat_completion(_stream)
        └─ 遍历模型列表
            ├─ forward_to_provider: 构建 URL、设置 Header、发送请求
            ├─ 成功 → 返回上游响应（流式/非流式）
            ├─ 失败 → penalty.penalize → 继续下一个
            └─ 全部失败 → 503
```

### 启动流程

```
1. 加载 .env 环境变量
2. 初始化 MySQL 数据库连接池 (Sea-ORM)
3. 初始化 Redis（失败则使用 disabled() 空实现）
4. 自动填充 admin_key 表的 fingerprint
5. 加载 API Key 全量缓存（Redis Set + 内存）
6. 构建 HTTP 客户端（连接池 20、空闲超时 90s、TCP keepalive 30s）
7. 创建全局 AppState 注入 Actix Web
8. 注册路由，启动 HTTP 服务
```

## 全局状态 (AppState)

```rust
pub struct AppState {
    pub db: DatabaseConnection,          // MySQL 数据库连接
    pub model_cache: ModelCache,         // 模型缓存（Redis + 内存）
    pub provider_cache: ProviderCache,   // 供应商缓存（Redis + 内存）
    pub client: Client,                  // reqwest HTTP 客户端
    pub priority_penalty: Data<PriorityPenalty>,  // 优先级惩罚
    pub redis: RedisManager,             // Redis 管理器
    pub api_key_cache: ApiKeyCache,      // API Key 全量缓存
    pub encryption_key: [u8; 32],        // AES-256-GCM 加密密钥
}
```
