# free_models_server

模型代理后端服务。对外暴露类 OpenAI 与 Anthropic 聊天补全接口，按优先级和可用性将请求自动切换到多个上游供应商。

技术栈：Rust / actix-web 4 / Sea-ORM 2 (MySQL) / reqwest 0.12 / Redis / moka (内存缓存) / tiktoken-rs / ed25519-dalek / AES-256-GCM

---

## 快速启动

```bash
# 1. 配置环境变量
cp .env.example .env
# 编辑 .env，至少设置 DATABASE_URL 和 ENCRYPTION_KEY

# 2. 创建数据库并执行迁移
mysql -u root -p -e "CREATE DATABASE IF NOT EXISTS free_models CHARACTER SET utf8mb4;"
mysql -u root -p free_models < migrations/001_init.sql
mysql -u root -p free_models < migrations/002_add_model_config_columns.sql
mysql -u root -p free_models < migrations/003_add_admin_key_and_usage.sql
mysql -u root -p free_models < migrations/004_create_usage_log_daily.sql
mysql -u root -p free_models < migrations/005_create_provider_model_map.sql

# 3. 运行
cargo run              # 开发
cargo run --release    # 生产
```

### 快速验证

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

---

## 环境变量

| 变量 | 默认值 | 必填 | 说明 |
|------|--------|------|------|
| `DATABASE_URL` | — | 是 | MySQL 连接串 |
| `ENCRYPTION_KEY` | — | 是 | AES-256 密钥，64 位 hex（32 字节） |
| `SERVER_HOST` | `0.0.0.0` | 否 | 监听地址 |
| `SERVER_PORT` | `8080` | 否 | 监听端口 |
| `DB_MAX_CONNECTIONS` | `100` | 否 | 数据库连接池上限 |
| `RUST_LOG` | `info` | 否 | 日志级别 |
| `REDIS_URL` | `redis://127.0.0.1:6379` | 否 | Redis 地址 |
| `REDIS_ENABLED` | `true` | 否 | 设为 `false` 禁用 Redis，全部降级到内存缓存 |
| `REDIS_CACHE_TTL_MODEL_SEC` | `30` | 否 | 模型缓存 TTL |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | `600` | 否 | 供应商缓存 TTL |
| `REDIS_CACHE_TTL_PENALTY_SEC` | `1800` | 否 | 惩罚缓存 TTL |
| `CIRCUIT_BREAKER_THRESHOLD` | `3` | 否 | 熔断器失败次数阈值 |
| `CIRCUIT_BREAKER_OPEN_TTL` | `30` | 否 | 熔断器开启持续时间（秒） |
| `CACHE_AFFINITY_MAX_CAPACITY` | `10000` | 否 | 模型-供应商亲和性缓存上限（moka） |
| `CACHE_AFFINITY_TTL_SEC` | `3600` | 否 | 模型-供应商亲和性缓存 TTL（秒） |
| `API_KEY_CACHE_MAX_CAPACITY` | `10000` | 否 | API Key fallback 缓存上限（moka） |
| `CIRCUIT_BREAKER_MAX_CAPACITY` | `10000` | 否 | 熔断器条目缓存上限（moka） |

---

## Docker

```bash
docker build -t free_models_server .
docker run --rm -p 8080:8080 --env-file .env free_models_server
```

---

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
```

---

## 目录结构

```
free_models_server/
├── Cargo.toml
├── Dockerfile
├── .env.example
├── free_models_store_api/           # 数据访问抽象接口（trait + DTO）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                   # 导出 models, stores；定义 StoreError, CacheStore
│       ├── models/                  # 7 个 DTO
│       └── stores/                  # 7 个 Store trait + Database 结构体
├── free_models_store/               # 数据访问实现（SeaORM + Redis）
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                   # 导出 cache, entities, stores
│       ├── cache.rs                 # RedisManager（实现 CacheStore）
│       ├── entities/                # 7 个 ORM 实体
│       └── stores/                  # 7 个 Store 实现
├── migrations/
│   ├── 001_init.sql                 # provider_config, model_config, api_key
│   ├── 002_add_model_config_columns.sql
│   ├── 003_add_admin_key_and_usage.sql
│   ├── 004_create_usage_log_daily.sql
│   └── 005_create_provider_model_map.sql
└── src/
    ├── main.rs                      # 入口、路由注册
    ├── app.rs                       # AppState、HTTP Client 构建
    ├── config.rs                    # 环境变量读取
    ├── response.rs                  # 统一错误响应（OpenAI + Anthropic 格式）
    ├── task.rs                      # 优雅关闭、每日归档定时任务
    ├── db/
    │   └── mod.rs                   # 构建 Database（委托到 free_models_store）
    ├── handler/
    │   ├── mod.rs
    │   ├── chat_handler.rs          # 公共请求处理（模型查询→筛选→转发）
    │   └── admin_handler.rs         # Admin CRUD（委托到 service 层）
    ├── middleware/
    │   ├── mod.rs
    │   ├── auth.rs                  # Bearer Token 鉴权
    │   └── admin_auth.rs            # Ed25519 签名鉴权
    ├── service/
    │   ├── mod.rs
    │   ├── provider_service.rs      # 供应商 CRUD
    │   ├── model_service_ext.rs     # 模型 CRUD + 统计
    │   ├── model_service.rs         # 模型查询 + 缓存 + 优先级排序
    │   ├── api_key_service.rs       # API Key CRUD + 自动生成
    │   ├── admin_key_service.rs     # SSH 公钥管理 + 自动填充 fingerprint
    │   ├── provider_credential_service.rs  # 凭证 CRUD + AES 加解密
    │   ├── provider_model_map_service.rs   # 供应商模型映射管理
    │   ├── proxy_service.rs         # 上游转发 + 故障切换 + SSE + 用量日志
    │   └── usage_log_service.rs     # 用量持久化
    └── util/
        ├── mod.rs
        ├── api_key_cache.rs         # Redis Set + 内存 fallback
        ├── cache_affinity.rs        # 模型-供应商缓存亲和性
        ├── encryption.rs            # AES-256-GCM 加密/解密
        ├── model_scheduler.rs       # 模型调度缓存
        ├── penalty.rs               # 优先级惩罚机制（熔断器）
        ├── proxy_ssrf.rs            # SSRF 防护
        ├── proxy_types.rs           # 代理转发类型定义
        ├── tokenizer.rs             # cl100k_base BPE token 估算
        └── usage_log_collector.rs   # 异步用量日志收集器
```

---

## 架构说明

本项目通过三个 crate 实现数据访问层的解耦：

```
free_models_server (二进制)
    ├── depends on ── free_models_store_api (接口层)
    │                   ├── StoreError          — 统一错误类型
    │                   ├── CacheStore          — 缓存操作 trait
    │                   ├── models/             — 7 个 DTO
    │                   └── stores/             — 7 个 Store trait + Database 聚合体
    └── depends on ── free_models_store (实现层)
                        ├── cache.rs            — RedisManager 实现 CacheStore
                        ├── entities/           — 7 个 SeaORM 实体
                        └── stores/             — 7 个 Store 实现
```

- **free_models_store_api**：纯接口 crate，仅定义 trait 和 DTO，无运行时依赖
- **free_models_store**：实现 crate，基于 SeaORM + Redis，业务层通过 trait 调用，不直接依赖数据库
- **通过 `Database` 结构体**聚合所有 Store 实现，业务层通过 `Arc<dyn StoreTrait>` 使用

---

## 详细文档

以下文档位于 `../docx/` 目录下，是该项目的**详细技术参考**：

| 文档 | 适合阅读 |
|------|---------|
| [架构总览](../docx/architecture.md) | 想了解系统全貌和请求处理流程 |
| [API 端点参考](../docx/api_endpoints.md) | 需要调用 API 时查阅 |
| [数据库表结构](../docx/database_schema.md) | 需要理解库表字段和关系 |
| [认证鉴权](../docx/authentication.md) | 需要对接鉴权或添加管理员密钥 |
| [缓存策略](../docx/caching_strategy.md) | 需要排查缓存问题或调整 TTL |
| [代理转发与 Token 计算](../docx/proxy_and_token.md) | 需要理解转发逻辑和 token 计数 |

点击 [项目概览](../README.md) 返回根目录。
