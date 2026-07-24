# free_models_server

模型代理后端服务。对外暴露类 OpenAI 与 Anthropic 聊天补全接口，按优先级和可用性将请求自动切换到多个上游供应商。

技术栈：Rust / actix-web 4 / Sea-ORM 2 (MySQL) / reqwest 0.12 / Redis / tiktoken-rs / ed25519-dalek / AES-256-GCM

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
├── .env
├── migrations/
│   ├── 001_init.sql                      # provider_config, model_config, api_key
│   ├── 002_add_model_config_columns.sql  # model_id, timeout, protocols
│   └── 003_add_admin_key_and_usage.sql   # admin_key, usage_log, provider_credential, context_length
└── src/
    ├── main.rs                 # 入口、AppState、路由注册、优雅关闭
    ├── config/mod.rs           # 环境变量读取
    ├── error.rs                # 统一错误响应（OpenAI + Anthropic 格式）
    ├── db/
    │   ├── mod.rs              # 数据库连接池初始化
    │   ├── redis.rs            # Redis 管理器 + 降级封装
    │   └── entities/           # 6 个 ORM 实体
    ├── middleware/
    │   ├── auth.rs             # Bearer Token 鉴权
    │   └── admin_auth.rs       # Ed25519 签名鉴权
    ├── handler/
    │   ├── chat_handler.rs     # 公共请求处理（模型查询→筛选→转发）
    │   └── admin_handler.rs    # Admin CRUD（委托到 service 层）
    ├── service/
    │   ├── provider_service.rs            # 供应商 CRUD
    │   ├── model_service_ext.rs           # 模型 CRUD + 统计
    │   ├── model_service.rs               # 模型查询 + 缓存 + 优先级排序
    │   ├── api_key_service.rs             # API Key CRUD + 自动生成
    │   ├── admin_key_service.rs           # SSH 公钥管理 + 自动填充 fingerprint
    │   ├── provider_credential_service.rs # 凭证 CRUD + AES 加解密
    │   ├── proxy_service.rs              # 上游转发 + 故障切换 + SSE + 用量日志
    │   └── usage_log_service.rs          # 用量持久化
    └── util/
        ├── crud.rs             # 泛型 count_total / count_active
        ├── api_key_cache.rs    # Redis Set + 内存 fallback
        ├── encryption.rs       # AES-256-GCM 加密/解密
        ├── penalty.rs          # 优先级惩罚机制
        └── tokenizer.rs        # cl100k_base BPE token 估算
```

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
