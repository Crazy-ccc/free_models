# free_models

## 项目简介

`free_models` 是一个**统一模型接入代理**服务，提供与 OpenAI 兼容的接口（`/v1/models`、`/v1/chat/completions`）。它屏蔽了底层多家模型供应商（provider）的差异，对外暴露单一的 OpenAI 风格的 API。

核心特性：

- **多供应商接入**：通过 `provider_config` + `model_config` 配置，一个模型可绑定多个供应商，共享 base_url 与 api_key。
- **故障切换（failover）**：当某个上游模型请求失败（HTTP 错误或网络异常）时，自动切换到下一个优先级的模型继续尝试。
- **模型优先级融合**：每次请求都基于全部激活模型构建列表，指定模型优先排在列表前端，其余激活模型按 priority 跟在后面，形成单一优先级列表供代理调用。

## 技术栈

- **Rust**（Edition 2021）
- **actix-web 4**：HTTP 服务框架
- **Sea-ORM 1**（sqlx-mysql + runtime-tokio-rustls）：MySQL 异步 ORM
- **MySQL**：元数据存储（provider 配置 / model 配置 / api_key）
- **reqwest 0.12**（json + stream）：向上游供应商转发请求（支持流式 SSE）
- 其他：`tokio`、`serde`/`serde_json`、`dotenv`、`chrono`、`log`/`env_logger`

## 目录结构

```
free_models/
├── .env                              # 环境配置（DATABASE_URL 等）
├── Cargo.toml                        # Rust 项目配置与依赖
├── README.md
├── migrations/
│   ├── 001_init.sql                  # 建表 SQL（全新部署手动执行）
│   └── 002_add_model_id.sql          # 升级脚本：为 model_config 新增 model_id 列
└── src/
    ├── main.rs                       # 入口：加载配置、连接数据库、加载 api_key 缓存、注册路由与中间件
    ├── config/
    │   └── mod.rs                    # 从环境变量读取 Config
    ├── db/
    │   ├── mod.rs                    # 数据库连接初始化（仅连接，不自动建表）
    │   └── entities/
    │       ├── mod.rs
    │       ├── provider_config.rs    # provider_config 实体
    │       ├── model_config.rs       # model_config 实体
    │       └── api_key.rs            # api_key 实体
    ├── error.rs                      # 统一 OpenAI 风格错误响应构造函数
    ├── middleware/
    │   ├── mod.rs
    │   └── auth.rs                   # Bearer Token 认证中间件（查内存 api_key 缓存）
    ├── handler/
    │   ├── mod.rs
    │   └── chat_handler.rs           # /v1/models 与 /v1/chat/completions 接口
    └── service/
        ├── mod.rs
        ├── model_service.rs          # 模型/供应商查询与多层缓存
        ├── api_key_cache.rs          # api_key 启动时全量加载到内存
        └── proxy_service.rs          # 上游代理转发（非流式 + SSE 流式）+ 故障切换
```

## 环境配置（`.env`）

服务通过 `dotenv` 读取项目根目录下的 `.env` 文件。支持的变量：

| 变量名               | 说明                            | 默认值     |
|---------------------|---------------------------------|-----------|
| `DATABASE_URL`      | MySQL 连接串（**必填**，缺失会 panic） | 无        |
| `SERVER_HOST`       | 服务监听地址                     | `0.0.0.0` |
| `SERVER_PORT`       | 服务监听端口                     | `8080`    |
| `DB_MAX_CONNECTIONS`| 数据库连接池最大连接数            | `100`     |
| `RUST_LOG`          | 日志级别（`error`/`warn`/`info`/`debug`）| `error`   |

`.env` 示例：

```env
DATABASE_URL=mysql://root:password@localhost:3306/model_proxy
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
DB_MAX_CONNECTIONS=100
RUST_LOG=info
```

## 数据库初始化

服务**启动时不再自动建表**。请先手动连接 MySQL 并执行 `migrations/001_init.sql` 完成建表，再启动服务。

若需要为已有数据库新增 `model_id` 列（升级），执行 `migrations/002_add_model_id.sql`。

### 表结构

#### `provider_config`（供应商配置）

| 字段           | 类型        | 说明                              |
|---------------|------------|-----------------------------------|
| `id`          | INT PK     | 自增主键                          |
| `name`        | VARCHAR(255)| 供应商名称（如 `openai`、`azure`） |
| `base_url`    | VARCHAR(512)| 供应商 API 基地址                  |
| `api_key`     | VARCHAR(512)| 供应商侧的 API Key                |
| `is_active`   | BOOLEAN    | 是否启用（默认 TRUE）              |
| `created_time`| DATETIME   | 创建时间                          |
| `last_updated`| DATETIME   | 更新时间（自动更新）               |

#### `model_config`（模型配置）

| 字段           | 类型        | 说明                                        |
|---------------|------------|---------------------------------------------|
| `id`          | INT PK     | 自增主键                                    |
| `provider_id` | INT        | 外键，关联 `provider_config(id)`             |
| `name`        | VARCHAR(255)| 模型名称（**客户端调用本系统接口使用的索引名**） |
| `model_id`    | VARCHAR(255)| **实际调用上游供应商 API 时填入 model 字段的值** |
| `priority`    | INT        | 故障切换优先级（数值越小越优先，默认 0）      |
| `is_active`   | BOOLEAN    | 是否启用（默认 TRUE）                        |
| `created_time`| DATETIME   | 创建时间                                    |
| `last_updated`| DATETIME   | 更新时间（自动更新）                         |

> **关键设计**：`name` 是客户端访问入口索引，`model_id` 是转发到上游时的真实模型名。两者可以不同（如 `name="gpt-4o"`, `model_id="gpt-4o-2024-08-06"`），实现模型名映射。

#### `api_key`（客户端调用凭证）

| 字段           | 类型        | 说明                              |
|---------------|------------|-----------------------------------|
| `id`          | INT PK     | 自增主键                          |
| `key_value`   | VARCHAR(512)| 客户端使用的 API Key（唯一）       |
| `name`        | VARCHAR(255)| Key 名称/备注                     |
| `is_active`   | BOOLEAN    | 是否启用（默认 TRUE）              |
| `created_time`| DATETIME   | 创建时间                          |
| `last_updated`| DATETIME   | 更新时间（自动更新）               |

### 数据关系

- `provider_config` **一对多** `model_config`：`model_config.provider_id` 外键引用 `provider_config(id)`。
- 模型是否可用需同时满足：`model_config.is_active = true` **且** 关联的 `provider_config.is_active = true`。

## 构建与启动

```bash
# 构建
cargo build --release

# 运行（debug 模式）
cargo run

# 运行（release 模式）
cargo run --release
```

启动流程：加载 `.env` → 初始化日志 → 连接数据库 → **全量加载启用状态 api_key 到内存** → 绑定 `SERVER_HOST:SERVER_PORT` 并监听。

## API 接口

所有接口都需 `Authorization: Bearer <api_key>` 认证（由 `AuthMiddleware` 校验，key 取自内存缓存，不查数据库）。

### `GET /v1/models`

列出当前所有可用模型的名称（去重），返回 OpenAI 格式的模型列表。

```json
{
  "object": "list",
  "data": [
    { "id": "gpt-3.5-turbo", "object": "model", "created": 1700000000, "owned_by": "model-proxy" }
  ]
}
```

### `POST /v1/chat/completions`

OpenAI 兼容的对话补全接口，支持 `stream` 参数。

**认证**：请求头 `Authorization: Bearer <api_key>`。

**模型选择逻辑**（`model` 参数可选）：

1. 先从缓存获取**全部激活模型列表**（`model_config.is_active` 且 `provider_config.is_active`）
2. 若 `model` 参数非空：将匹配 `name` 的条目移到列表前端，其余激活模型按 `priority` 升序排在后面
3. 若 `model` 参数为空或不传：直接使用全部激活模型列表（按 `priority` 升序）
4. 若列表为空（无任何可用模型）：返回 **503** `No available models`

**故障切换**：在构建好的列表中按顺序向上游发起请求：

- 上游返回 **2xx** → 成功，透传响应。
- 上游返回任何 HTTP 错误（4xx/5xx）或**网络异常** → 跳过当前条目，自动尝试列表中下一个。
- 全部条目都失败 → 返回 **503** `All models unavailable for: <model>`。

> 流式（`stream: true`）与非流式走相同的故障切换逻辑；流式成功时以 `text/event-stream` 透传上游 SSE 流。

> **model 字段替换**：转发到上游的请求中，`model` 字段会被替换为 `model_id`（不是客户端原始的 `model` 值）。其他字段（`messages`、`temperature` 等）保持透传不变。

## 缓存策略

| 缓存对象           | 策略 |
|-------------------|------|
| `api_key`         | 服务**启动时全量加载** `is_active = true` 的 key 到内存（`HashSet`）。运行期新增或禁用 key 不会自动生效，需重启服务。 |
| `provider_config` | 按 `id` 缓存，TTL **600 秒**（10 分钟）。 |
| `model_config`    | 全局激活模型列表按 `__all__` 键缓存，TTL **30 秒**。 |

> 数据库中的 model/provider 变更在缓存过期后（最长 30s/600s）生效；api_key 仅在启动时加载，变更需重启。

## 配置示例（SQL）

```sql
-- 1. 供应商
INSERT INTO provider_config (name, base_url, api_key, is_active, created_time, last_updated)
VALUES ('openai', 'https://api.openai.com', 'sk-provider-key-xxxx', TRUE, NOW(), NOW());

-- 2. 模型（绑定 provider_id = 1，name 是客户端索引，model_id 是上游实际模型名）
INSERT INTO model_config (provider_id, name, model_id, priority, is_active, created_time, last_updated)
VALUES (1, 'gpt-3.5-turbo', 'gpt-3.5-turbo', 10, TRUE, NOW(), NOW());

-- 3. 客户端调用凭证
INSERT INTO api_key (key_value, name, is_active, created_time, last_updated)
VALUES ('client-key-abc123', 'default-client', TRUE, NOW(), NOW());
```

## curl 调用示例

传 `model` 参数：

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer client-key-abc123" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "你好"}],
    "stream": false
  }'
```

不传 `model` 参数（自动在全量激活模型中按 priority 选择）：

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer client-key-abc123" \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [{"role": "user", "content": "你好"}]
  }'
```