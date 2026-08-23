# free_models_server

模型代理后端服务。对外暴露 OpenAI（`/v1/chat/completions`）、Anthropic（`/v1/messages`）与 Responses（`/v1/responses`）三种协议接口，按优先级和可用性将请求自动切换到多个上游供应商。

技术栈：Rust / actix-web 4 / Sea-ORM 2 (SQLite) / reqwest 0.12 / moka (进程内内存缓存) / tiktoken-rs / ed25519-dalek / AES-256-GCM。内置 SSRF 防护（私网 IP 拦截 + DNS fail-closed）与 OpenAI / Anthropic / Responses 三协议支持。

---

## 快速启动

```bash
# 1. 配置环境变量
cp .env.example .env
# 编辑 .env，至少设置 DATABASE_URL 和 ENCRYPTION_KEY

# 2. 数据库：服务启动自动幂等建表，无需手动步骤；

#    或从存量 MySQL 迁移数据（在仓库根目录 ../ 运行，脚本自动建表并导出 SQLite 文件；
#    uv 按 PEP 723 注释自动装依赖，无 uv 时先 pip install pymysql cryptography）：
#    uv run migrate_mysql_to_sqlite.py --mysql-url mysql://user:pass@127.0.0.1:3306/free_models
#    然后将 DATABASE_URL 指向生成的 free_models.db（默认输出到仓库根目录）

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
| `DATABASE_URL` | — | 是 | SQLite 连接串，如 `sqlite://free_models.db?mode=rwc` |
| `ENCRYPTION_KEY` | — | 是 | AES-256-GCM 密钥，64 位 hex（32 字节），用 `openssl rand -hex 32` 生成 |
| `SERVER_HOST` | `0.0.0.0` | 否 | 监听地址 |
| `SERVER_PORT` | `8080` | 否 | 监听端口 |
| `DB_MAX_CONNECTIONS` | `10` | 否 | 数据库连接池上限（`.env.example` 推荐值 100） |
| `SCHEDULER_CACHE_TTL_SEC` | `30` | 否 | 模型调度缓存 TTL（秒） |
| `RUST_LOG` | `info` | 否 | 日志级别（env_logger 读取，`.env.example` 推荐值） |
| `CIRCUIT_BREAKER_THRESHOLD` | `3` | 否 | 熔断器失败次数阈值 |
| `CIRCUIT_BREAKER_OPEN_TTL` | `30` | 否 | 熔断器开启持续时间（秒） |
| `CIRCUIT_BREAKER_MAX_CAPACITY` | `10000` | 否 | 熔断器条目缓存上限（moka） |
| `CACHE_AFFINITY_MAX_CAPACITY` | `10000` | 否 | 模型-供应商亲和性缓存上限（moka） |
| `CACHE_AFFINITY_TTL_SEC` | `300` | 否 | 模型-供应商亲和性缓存 TTL（秒） |
| `API_KEY_CACHE_MAX_CAPACITY` | `10000` | 否 | API Key 缓存上限（moka） |
| `SSRF_CACHE_MAX_CAPACITY` | `10000` | 否 | SSRF 校验结果缓存上限（moka） |

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

# Responses 协议（OpenAI 新一代接口）
curl -X POST http://localhost:8080/v1/responses \
  -H "Authorization: Bearer <API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4o","input":"Hello"}'
```

---

## 目录结构

```
free_models_server/
├── Cargo.toml
├── Dockerfile
├── .env.example
├── migrations/
│   └── 001_sqlite_schema.sql           # SQLite 全量建表（7 张业务表 + usage_log_daily 日汇总表）
└── src/
    ├── main.rs                          # 入口、路由注册、启动（/health、/v1/*、/admin/*）
    ├── app.rs                           # AppState、HTTP Client 构建（UA: FreeModelsServer/1.0）
    ├── config.rs                        # 环境变量读取与默认值
    ├── response.rs                      # 统一错误响应（OpenAI / Anthropic 格式）
    ├── task.rs                          # 优雅关闭、每日 00:05 UTC 归档定时任务
    ├── db/
    │   ├── mod.rs                       # StoreError、Database 聚合结构体、init_db / build_database
    │   ├── types.rs                     # DTO 类型（UsageLogInsert、UsageLogStatItem 等）
    │   ├── entities/                    # 7 个 SeaORM 实体
    │   │   ├── admin_key.rs
    │   │   ├── api_key.rs
    │   │   ├── model_config.rs
    │   │   ├── provider_config.rs
    │   │   ├── provider_credential.rs
    │   │   ├── provider_model_map.rs
    │   │   └── usage_log.rs
    │   └── impls/                       # 7 个 Store 实现（业务数据访问层）
    │       ├── admin_key.rs
    │       ├── api_key.rs
    │       ├── model_config.rs
    │       ├── provider_config.rs
    │       ├── provider_credential.rs   # 含 quota_exhausted 标记/清除
    │       ├── provider_model_map.rs
    │       └── usage_log.rs             # query_stats 聚合、archive_yesterday 归档
    ├── handler/
    │   ├── mod.rs
    │   ├── chat_handler.rs              # /v1/models、/v1/chat/completions、/v1/messages、/v1/responses
    │   └── admin/                       # Admin API（Ed25519 签名）
    │       ├── mod.rs                   # admin_routes() 路由注册
    │       ├── api_key.rs
    │       ├── import_models.rs         # 一键导入供应商模型
    │       ├── model.rs
    │       ├── provider.rs
    │       ├── provider_credential.rs   # 含 reset_status 配额重置
    │       ├── provider_model_map.rs
    │       ├── stats.rs                 # 服务状态、缓存刷新、用量统计
    │       └── test_credential.rs       # 凭证连通性测试
    ├── middleware/
    │   ├── mod.rs
    │   ├── auth.rs                      # Bearer Token 鉴权（API Key 缓存）
    │   └── admin_auth.rs                # Ed25519 签名鉴权（时间戳 + Nonce + BodyHash）
    ├── service/
    │   ├── mod.rs
    │   ├── provider_credential_service.rs # 凭证 CRUD + AES-256-GCM 加解密
    │   └── proxy_service.rs             # 上游转发 + 故障切换 + SSE + 用量日志 + 配额标记
    └── util/
        ├── mod.rs
        ├── api_key_cache.rs             # API Key 校验集合缓存（moka）
        ├── cache_affinity.rs            # 模型-供应商亲和性缓存（moka）
        ├── encryption.rs                # AES-256-GCM 加解密
        ├── model_scheduler.rs           # 模型调度：活跃配置查询 + 凭证解密 + 排序
        ├── penalty.rs                   # 熔断器（moka）
        ├── proxy_ssrf.rs                # SSRF 防护（私网 IP 拦截 + DNS fail-closed）
        ├── proxy_types.rs               # Protocol 三协议 / UsageInfo / 日志上下文
        ├── stream_usage_scanner.rs      # SSE 流式 usage 扫描
        ├── tokenizer.rs                 # tiktoken cl100k_base token 估算
        └── usage_log_collector.rs       # 异步用量日志收集器（内存队列批量落库）
```

---

## 架构说明

本项目为**单 crate** 架构，数据访问层在 `src/db/` 内按模块组织，业务代码直接调用具体 Store 结构体（不使用 trait 抽象，无需中间 crate）：

- `src/db/entities/`：7 个 SeaORM 实体（provider_config、model_config、provider_model_map、api_key、admin_key、provider_credential、usage_log）
- `src/db/impls/`：7 个对应的 Store 实现（`XxxStoreSeaorm`），封装 SeaORM 查询逻辑，如 `UsageLogStoreSeaorm`（用量聚合 `query_stats` 与归档 `archive_yesterday`）、`ProviderCredentialStoreSeaorm`（配额标记/清除）等
- `src/db/mod.rs`：定义统一的 `StoreError` 错误类型与 `Database` 聚合结构体，`init_db` / `build_database` 负责初始化连接池并装配各 Store
- 业务层（handler / service / util）通过 `AppState.database` 上的具体 Store 字段访问数据

**请求处理链路：**

```
客户端请求
  → AuthMiddleware（Bearer Token 鉴权，基于 API Key 缓存）
  → chat_handler / admin handler（Admin 侧先经 AdminAuthMiddleware：Ed25519 签名 + 时间戳 ±300s + Nonce + BodyHash）
  → model_scheduler 调度（调度缓存 → 活跃配置筛选 → 凭证解密 → 协议/上下文窗口过滤）
  → proxy_service 转发（亲和性排序 → 熔断器/配额检查 → SSRF 校验 → 上游请求 → 故障切换）
  → usage_log_collector 异步批量写入 usage_log
  → 每日 00:05 UTC 由 task.rs 将昨日明细归档到 usage_log_daily
```

**进程内内存缓存：** 全部基于 moka —— 模型调度缓存使用带 TTL 的 future Cache（`SCHEDULER_CACHE_TTL_SEC` 可配，默认 30s）；API Key 校验集合、熔断器、模型-供应商亲和性、SSRF 校验结果均为 moka 内存缓存。API Key 增删改及 `/admin/cache/refresh` 会触发全量重建。

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
| [模型代理全链路梳理](../docx/model_proxy_chain.md) | 需要从请求进入（鉴权/校验）到响应返回（调度/转发/用量记录）的完整链路

点击 [项目概览](../README.md) 返回根目录。
