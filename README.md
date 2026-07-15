# free_models

统一的模型代理服务：对外暴露类 OpenAI 的 `/v1/chat/completions` 接口和 Anthropic 的 `/v1/messages` 接口，把请求按配置好的模型优先级转发到不同供应商（provider），并提供 `/v1/models` 模型列表查询。支持按模型粒度配置超时、供应商调用失败自动降级与故障切换，按协议筛选模型，以及多级内存缓存以加速访问。

- 代码包名：`free_models`（Cargo 包），项目目录：`free_models_token/`。
- 数据库不自动建表，需手动执行 `migrations/*.sql` 并执行数据初始化。

技术栈：Rust / [actix-web 4](https://actix.rs/) / [Sea-ORM 1](https://www.sea-orm.org/)（MySQL）/ [reqwest 0.12](https://docs.rs/reqwest)（rustls TLS）。

---

## 架构与路由

```
客户端
  │  Authorization: Bearer <api_key>
  ▼
AuthMiddleware（校验 api_key_cache）
  ▼
┌─────────────────────────────────────────────────────────────┐
│ GET  /health                   → health_check     │  健康检查（免鉴权）
│ GET  /v1/models                → list_models      │  返回 OpenAI 格式的模型列表
│ POST /v1/chat/completions      → chat_completions │  OpenAI 协议代理转发
│ POST /v1/messages              → anthropic_messages│ Anthropic 协议代理转发
│ POST /admin/cache/refresh      → refresh_cache    │  手动刷新缓存（需鉴权）
└─────────────────────────────────────────────────────────────┘
  │
  ▼
chat_handler：取全部激活模型 → 按协议筛选 → 客户端指定 model 则前置 → 应用失败惩罚排序
  ▼
proxy_service：按协议构建 URL/请求头，按模型 timeout 逐个转发；失败则降级并尝试链下一个
```

| 路由 | 方法 | Handler | 说明 |
|------|------|---------|------|
| `/health` | GET | `health_check` | 健康检查，返回 `{"status":"ok"}`，不需鉴权 |
| `/v1/models` | GET | `list_models` | 返回 OpenAI 格式 `{"object":"list","data":[...]}` 的模型列表（基于 `model_config.name` 去重） |
| `/v1/chat/completions` | POST | `chat_completions` | 接收 OpenAI 格式请求，校验 `messages` 非空，筛选 `protocols` 含 `"openai"` 的模型，转发到供应商，支持 `stream` 流式响应 |
| `/v1/messages` | POST | `anthropic_messages` | 接收 Anthropic Messages API 格式请求，校验 `messages` 非空，筛选 `protocols` 含 `"anthropic"` 的模型，转发到供应商，支持 `stream` 流式响应 |
| `/admin/cache/refresh` | POST | `refresh_cache` | 手动清空模型缓存和供应商缓存，需鉴权，返回 `{"status":"refreshed"}` |

---

## 鉴权

除 `/health` 外所有路由都被 `AuthMiddleware` 包裹。请求必须携带请求头：

```
Authorization: Bearer <key_value>
```

- 中间件从 `Authorization` 头提取 Bearer token（前缀 `Bearer `，缺失或格式错误返回 `401`）；
- 与启动时全量加载到内存的 `api_key_cache`（仅 `is_active = 1` 的 `api_key` 记录）比对，命中才放行，否则返回 `401`；
- 鉴权失败的错误格式按请求路径自动匹配：`/v1/messages` 返回 Anthropic 格式错误，其他路径返回 OpenAI 格式错误；
- 因此 **`api_key` 表必须至少有一条激活记录**，且客户端请求必须带正确的 Bearer 头，否则 handler 根本不会被执行。

---

## 模型选择与故障切换

每次 `/v1/chat/completions` 或 `/v1/messages` 调用：

1. **请求体校验**：检查 `messages` 字段是否存在且为非空数组，否则返回 400；
2. 从 `model_cache` 取出**全部激活**的模型信息（`get_all_available_models_by_priority`，按 `priority` 升序）；
3. **按协议筛选**：`/v1/chat/completions` 仅保留 `protocols` 含 `"openai"` 的模型；`/v1/messages` 仅保留 `protocols` 含 `"anthropic"` 的模型；
4. 若客户端请求带了 `model` 字段，用 `merge_preferred_first` 把匹配 `name` 的模型**前置**，其余保持优先级顺序；
5. 应用**失败惩罚排序**：被惩罚的模型（`PriorityPenalty`，见下）一律排到尝试链末尾，覆盖「客户端指定前置」；
6. 交给 `proxy_service` 依次尝试：对尝试链中的模型逐个转发，成功即返回；失败（非 2xx 或网络/超时错误）则降级该模型并继续下一个；
7. 全部失败返回 `503`（OpenAI 格式或 Anthropic 格式，取决于请求协议）。

模型激活判定：模型自身 `is_active = 1` **且**其所属供应商 `provider_config.is_active = 1`，二者都满足才参与尝试链（在 `collect_model_provider_infos` 中过滤未激活的 provider）。

---

## 超时与降级

### 按模型超时
每个模型在 `model_config.timeout`（整数，单位**秒**）中独立配置，**默认 30 秒**（由 `002_add_model_config_columns.sql` 设 `DEFAULT 30`）。代理转发时对每个模型使用 `Duration::from_secs(model_info.timeout)` 作为该次请求的超时；同时为请求显式带上 `Accept-Encoding: identity`，避免上游对 SSE 体做 gzip 压缩后截断导致的解码错误。

### 协议感知转发
转发时根据请求协议（`Protocol::OpenAI` / `Protocol::Anthropic`）构建不同的上游 URL 和请求头：

| 协议 | 上游路径 | 认证头 | 额外头 |
|------|----------|--------|--------|
| OpenAI | `{base_url}/chat/completions` | `Authorization: Bearer {api_key}` | — |
| Anthropic | `{base_url}/messages` | `x-api-key: {api_key}` | `anthropic-version: 2023-06-01` |

两种协议均带 `Content-Type: application/json` 和 `Accept-Encoding: identity`。

### 失败自动降级（故障切换）
`PriorityPenalty`（`src/service/penalty.rs`）按 `model_name|provider_name` 维度记录惩罚状态：
- 某个模型调用供应商**失败**（网络错误、超时、非 2xx）时，将其惩罚**可配置时长**（`PENALTY_TTL_SEC`，默认 1800 秒，惰性过期，无需后台线程）；
- 惩罚期间，该模型在尝试链中始终排在所有未惩罚模型**之后**；
- 即便客户端显式指定该模型，惩罚期间它仍排在末尾，不会因被指定而提升；
- 惩罚期过后自动恢复，按原有优先级/指定逻辑参与排序。

### SSE 流式优雅关闭
流式响应（`stream: true`）中，若上游在传输中途出错，`proxy_service` 不再抛 `5xx`（`text/event-stream` 响应头已发出，直接断连会导致客户端报 `SSE stream error`），而是下发一个干净的结束帧：

- **OpenAI 协议**：`data: [DONE]`
- **Anthropic 协议**：`event: error` + Anthropic 格式错误 JSON

让客户端以正常方式收尾。该行为由 `sse_close_event()` / `anthropic_sse_close_event()` 保证，并有单元测试覆盖。

### 优雅关闭
服务在收到 SIGTERM（Unix）或 Ctrl+C（所有平台）信号时，停止接受新连接并等待活跃请求完成（最多 30 秒），超时后强制退出。日志输出 "Shutting down gracefully..." 提示优雅关闭开始。

### 连接池配置
上游 HTTP 客户端（`reqwest::Client`）在启动时配置连接池参数：
- `pool_max_idle_per_host(20)`：每个上游主机最多保持 20 个空闲连接
- `pool_idle_timeout(90s)`：空闲连接 90 秒后自动回收
- `tcp_keepalive(30s)`：TCP 保活探测间隔 30 秒

---

## 缓存策略

| 缓存 | 范围 | TTL | 说明 |
|------|------|-----|------|
| `api_key_cache` | 全部 `is_active` 的 `api_key` | 进程生命周期（启动全量加载） | 启动时 `ApiKeyCache::load_all` 一次性加载，运行期不再查表 |
| `provider_cache` | `provider_config` | 可配置，默认 600 秒（`PROVIDER_CACHE_TTL_SEC`） | 按 `provider_id` 缓存供应商配置，降供应商查询压力 |
| `model_cache` | 全部激活的 `ModelProviderInfo` | 可配置，默认 30 秒（`MODEL_CACHE_TTL_SEC`） | 以 `__all__` 为键缓存；handler 优先读缓存，缓存未命中才查库 |

可通过 `POST /admin/cache/refresh` 端点手动清空 `model_cache` 和 `provider_cache`，下次请求时自动重建。

---

## 配置（环境变量与 Config）

服务通过 `Config`（由环境变量构造）读取配置：

| 环境变量 | 说明 | 默认值 |
|----------|------|--------|
| `DATABASE_URL` | MySQL 连接串（**必填**） | 无，缺失则启动 panic |
| `SERVER_HOST` | 监听地址 | `0.0.0.0` |
| `SERVER_PORT` | 监听端口 | `8080` |
| `DB_MAX_CONNECTIONS` | 数据库连接池上限 | `100` |
| `MODEL_CACHE_TTL_SEC` | 模型缓存 TTL（秒） | `30` |
| `PROVIDER_CACHE_TTL_SEC` | 供应商缓存 TTL（秒） | `600` |
| `PENALTY_TTL_SEC` | 失败惩罚时长（秒） | `1800` |
| `RUST_LOG` | 日志级别（env_logger） | 建议 `info` |

示例 `.env`：

```dotenv
DATABASE_URL=mysql://user:password@127.0.0.1:3306/free_models
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
DB_MAX_CONNECTIONS=100
MODEL_CACHE_TTL_SEC=30
PROVIDER_CACHE_TTL_SEC=600
PENALTY_TTL_SEC=1800
RUST_LOG=info
```

> 注意：日志默认级别为 `error`，不配置 `RUST_LOG=info` 将看不到 `info!`/`warn!` 输出（含请求转发与降级相关日志）。

---

## 数据库与迁移（手动执行 SQL）

本项目**不**在启动时自动建表，迁移以 SQL 文件形式放在 `migrations/` 目录，**需手动按顺序在 MySQL 中执行**：

| 文件 | 内容 |
|------|------|
| `migrations/001_init.sql` | 初始化 `provider_config`、`model_config`、`api_key` 三张表 |
| `migrations/002_add_model_config_columns.sql` | 为 `model_config` 增加 `model_id`、`timeout`、`protocols` 三个列 |

执行顺序示例：

```bash
mysql -u <user> -p free_models < migrations/001_init.sql
mysql -u <user> -p free_models < migrations/002_add_model_config_columns.sql
```

> 必须先完成迁移，否则启动后实体字段与表结构不匹配会导致查询失败。

### 表结构

**provider_config**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT PK | 供应商主键 |
| `name` | VARCHAR(255) | 供应商名称 |
| `base_url` | VARCHAR(512) | 上游 base URL（转发时会拼接 `/chat/completions`） |
| `api_key` | VARCHAR(512) | 访问上游的 Bearer token |
| `is_active` | BOOLEAN | 是否启用（false 时其下模型不参与尝试链） |

**model_config**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT PK | 模型主键 |
| `provider_id` | INT FK | 关联 `provider_config.id` |
| `name` | VARCHAR(255) | 对外暴露的模型名（客户端 `model` 字段匹配此值） |
| `model_id` | VARCHAR(255) | 实际转发上游时使用的 model 名 |
| `timeout` | INT | 该模型单次转发超时（秒），默认 30 |
| `protocols` | VARCHAR(255) | 支持的协议列表，逗号分隔，如 `"openai"`、`"anthropic"`、`"openai,anthropic"`，默认 `"openai"` |
| `priority` | INT | 排序优先级，升序优先尝试（默认 0） |
| `is_active` | BOOLEAN | 是否启用（false 时该模型不参与尝试链） |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

**api_key**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | INT PK | 主键 |
| `key_value` | VARCHAR(512) UNIQUE | 客户端请求携带的 Bearer token 值 |
| `name` | VARCHAR(255) | 备注/名称 |
| `is_active` | BOOLEAN | 是否启用（仅 `is_active=1` 的 key 会被加载到缓存用于鉴权） |
| `created_time` | DATETIME | 创建时间 |
| `last_updated` | DATETIME | 更新时间 |

建表后还需：
- 在 `api_key` 表插入至少一条 `is_active = 1` 的记录（用于鉴权）；
- 在 `provider_config` 配置供应商（含 `base_url`、`api_key`、`is_active`），在 `model_config` 配置模型（含 `name`、`model_id`、`timeout`、`protocols`、`priority`、`is_active` 及关联的 `provider_id`）。

---

## 构建与运行

### 本地（cargo）

```bash
# 1. 准备 .env（见上）
# 2. 手动执行 migrations/*.sql
# 3. 构建并运行
cargo build --release
./target/release/free_models
```

### Docker

当前 `Dockerfile` 为**多阶段**构建（基于 `rust:alpine`，运行时 `alpine:3.21`）。它依赖纯 Rust TLS（rustls），无需在镜像中安装 OpenSSL 构建库：

```dockerfile
FROM rust:alpine AS builder
RUN apk add --no-cache ca-certificates musl-dev curl
RUN rustup update stable && rustup default stable
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    CARGO_HTTP_MULTIPLEXING=false \
    CARGO_NET_RETRY=5 \
    cargo build --release && \
    cp target/release/free_models /tmp/free_models && \
    strip /tmp/free_models

FROM alpine:3.21
RUN apk add --no-cache ca-certificates tzdata
COPY --from=builder --chown=nobody:nobody /tmp/free_models /usr/local/bin/free_models
USER nobody
EXPOSE 8080
CMD ["free_models"]
```

构建与运行：

```bash
docker build -t free_models .
docker run --rm -p 8080:8080 --env-file .env free_models
```

> 镜像内不含 `.env` 与数据库表；需通过 `--env-file` 注入配置，并提前在数据库中执行迁移与初始化数据。

---

## 调用示例

**健康检查（免鉴权）：**

```bash
curl http://localhost:8080/health
```

**查询可用模型：**

```bash
curl -H "Authorization: Bearer <KEY>" http://localhost:8080/v1/models
```

**非流式对话补全：**

```bash
curl -H "Authorization: Bearer <KEY>" -H "Content-Type: application/json" \
  -X POST http://localhost:8080/v1/chat/completions \
  -d '{"model":"<MODEL>","messages":[{"role":"user","content":"hi"}]}'
```

**流式对话补全（`stream: true`）：**

```bash
curl -N -H "Authorization: Bearer <KEY>" -H "Content-Type: application/json" \
  -X POST http://localhost:8080/v1/chat/completions \
  -d '{"model":"<MODEL>","stream":true,"messages":[{"role":"user","content":"hi"}]}'
```

> 不传 `model` 时，服务按 `priority` 升序遍历所有激活模型尝试；传 `model` 时优先尝试同名模型，但仍受惩罚排序约束。

**Anthropic 协议对话补全（`/v1/messages`）：**

```bash
curl -H "Authorization: Bearer <KEY>" -H "Content-Type: application/json" \
  -X POST http://localhost:8080/v1/messages \
  -d '{"model":"<MODEL>","max_tokens":1024,"messages":[{"role":"user","content":"hi"}]}'
```

> `/v1/messages` 仅使用 `protocols` 含 `"anthropic"` 的模型。请求体需符合 Anthropic Messages API 格式（`max_tokens` 必填）。

**手动刷新缓存：**

```bash
curl -X POST -H "Authorization: Bearer <KEY>" http://localhost:8080/admin/cache/refresh
```

---

## 目录结构

```
free_models_token/
├── Cargo.toml
├── Cargo.lock
├── Dockerfile                  # 多阶段 rust:alpine 构建
├── .env                        # 运行配置（DATABASE_URL / RUST_LOG 等）
├── migrations/                 # 手动执行的 SQL 迁移
│   ├── 001_init.sql
│   └── 002_add_model_config_columns.sql
└── src/
    ├── main.rs                 # 入口：AppState、路由、AppData 注册
    ├── config/mod.rs           # Config（环境变量读取）
    ├── error.rs                # 统一错误响应（OpenAI 格式 + Anthropic 格式）
    ├── db/                     # Sea-ORM 实体与连接
    │   ├── entities/           # model_config / provider_config / api_key 实体
    │   └── mod.rs              # init_db（连接池配置）
    ├── middleware/
    │   └── auth.rs             # AuthMiddleware（Bearer 鉴权，协议感知错误格式）
    ├── handler/
    │   └── chat_handler.rs     # list_models / chat_completions / anthropic_messages / health_check / refresh_cache
    └── service/
        ├── model_service.rs    # 模型查询、优先级、缓存、合并、协议检查
        ├── proxy_service.rs    # 供应商转发、多协议、超时、降级、SSE 优雅关闭
        ├── penalty.rs          # PriorityPenalty（失败惩罚）
        └── api_key_cache.rs    # api_key 全量缓存
```

---

## 注意事项

- **必须手动迁移数据库**：表不在启动时创建，未执行 `migrations/*.sql` 会导致启动失败或查询异常。
- **必须配置激活的 api_key**：否则任何请求都被 `401` 拦截，handler 进不去。
- **超时是模型粒度的**：默认值 30s，可在 `model_config.timeout` 按模型调整，慢模型调大、快模型调小。
- **失败会触发降级**：某模型调用供应商连续失败会进入惩罚期（默认 1800 秒，可通过 `PENALTY_TTL_SEC` 配置），期间该模型排在尝试链末尾。
- **请求体校验**：`/v1/chat/completions` 和 `/v1/messages` 要求 `messages` 字段存在且非空，否则返回 400。
- **日志需 `RUST_LOG=info`**：否则看不到请求转发与降级相关的 `info!`/`warn!` 日志。
- **协议配置**：`model_config.protocols` 默认 `"openai"`，若需支持 Anthropic 协议，设置为 `"openai,anthropic"` 或 `"anthropic"`。
- **缓存可配置**：`MODEL_CACHE_TTL_SEC`、`PROVIDER_CACHE_TTL_SEC`、`PENALTY_TTL_SEC` 可通过环境变量自定义，也可通过 `POST /admin/cache/refresh` 手动刷新。