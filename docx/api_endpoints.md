# API 端点文档

服务启动后默认监听 `0.0.0.0:8080`，可通过 `SERVER_HOST` / `SERVER_PORT` 环境变量配置。

## 公共 API（无 Admin 鉴权）

这些端点对外暴露，使用 Bearer Token 鉴权（API Key）。

### 健康检查

```
GET /health
```

无需鉴权。返回服务运行状态。

**响应示例：**
```json
{
    "status": "ok"
}
```

### 列出可用模型

```
GET /v1/models
Authorization: Bearer <api_key>
```

返回所有可用模型列表（OpenAI 兼容格式）。模型可用性判定：活跃 `model_config` + 存在 `provider_model_map` 且对应供应商有活跃凭证。

**响应示例：**
```json
{
    "object": "list",
    "data": [
        {
            "id": "gpt-3.5-turbo",
            "object": "model",
            "created": 1700000000,
            "owned_by": "model-proxy"
        }
    ]
}
```

### 对话补全（OpenAI 协议）

```
POST /v1/chat/completions
Authorization: Bearer <api_key>
Content-Type: application/json
```

标准 OpenAI Chat Completions 接口代理。支持 `stream: true` 流式响应。

**请求体示例：**
```json
{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": false
}
```

**处理流程：**
1. 校验 `messages` 字段为非空数组
2. `schedule_all_available` 查询可用模型，`supports_protocol` 过滤出支持 `openai` 协议的模型
3. 若指定 `model`，`merge_preferred_first` 将匹配模型排到最前
4. 估算 prompt token，`filter_by_context_window` 过滤上下文窗口不足的模型
5. `reorder_by_affinity` 亲和排序（优先复用该 API Key 上次成功转发的目标）
6. 三重循环（模型 → 映射 → 凭证）向上游转发：跳过 `quota_exhausted` 的凭证与熔断中的目标
7. 转发成功即返回；可重试错误（429 非配额 / 500/502/503/504 / 网络错误 / 超时）按退避等待后重试一次；凭证级失败（401/402/403、429 配额）尝试下一凭证
8. 全部失败返回 503

**流式响应（`stream: true`）：**
- 强制 `stream=true`，OpenAI 协议自动设置 `stream_options: {"include_usage": true}`
- 后台 `SseScanner` 在流中检测 `usage` 字段并记录用量日志；检测到流内嵌错误即中断转发
- **SSE 数据原样透传，不伪造结束帧**（`data: [DONE]` 由上游数据自然携带，服务端不生成）

### Anthropic Messages（Anthropic 协议）

```
POST /v1/messages
Authorization: Bearer <api_key>
Content-Type: application/json
```

标准 Anthropic Messages API 代理。支持 `stream: true` 流式响应。

**请求体示例：**
```json
{
    "model": "claude-3-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello"}]
}
```

**处理流程：** 与 OpenAI 协议相同，但：
- 校验 `messages` 字段为非空数组，按 `anthropic` 协议筛选模型
- 上游路径为 `/messages`，添加 `x-api-key` 和 `anthropic-version: 2023-06-01` 请求头
- 错误响应使用 Anthropic 格式
- 流式时用量从 `usage` 字段提取，SSE 数据原样透传，不伪造结束帧

### Responses API（OpenAI Responses 协议）

```
POST /v1/responses
Authorization: Bearer <api_key>
Content-Type: application/json
```

OpenAI Responses API 代理。支持 `stream: true` 流式响应。

**请求体示例：**
```json
{
    "model": "gpt-4o",
    "input": "Hello"
}
```

**处理流程：** 与 OpenAI 协议相同，但：
- 校验 `input` 字段为非空字符串或非空数组
- 按 `responses` 协议筛选模型
- 上游路径为 `/responses`，鉴权头使用 `Authorization: Bearer`
- 流式时用量从 `usage` 或 `response.usage` 字段提取，SSE 数据原样透传，不伪造结束帧
- 错误响应使用 OpenAI 格式

---

## Admin API（需 Ed25519 签名鉴权）

所有 Admin API 端点以 `/admin` 为前缀，需携带 Ed25519 签名请求头（详见 [authentication.md](./authentication.md)）。

### 获取服务状态

```
GET /admin/service/status
```

返回概览统计数据。

**响应示例：**
```json
{
    "healthy": true,
    "models": {
        "total": 10,
        "active": 8,
        "inactive": 2
    },
    "providers": {
        "total": 3,
        "active": 2,
        "inactive": 1
    },
    "apiKeys": {
        "total": 5,
        "active": 4,
        "inactive": 1
    },
    "penalties": [
        {
            "modelName": "gpt-3.5-turbo",
            "providerName": "provider-a",
            "credentialId": 1,
            "remainingSecs": 120
        }
    ]
}
```

**统计规则：**
- `models.active`：真正可用的模型数（活跃 `model_config` + 活跃 `provider_model_map` + 活跃 `provider_credential` 三重校验）
- `models.inactive`：`total - active`
- `providers.active`：有活跃凭证的唯一供应商数
- `apiKeys.active`：`is_active = true` 的 API Key 数
- `penalties`：当前处于熔断状态的「模型 / 供应商 / 凭证」目标及剩余秒数

### 刷新缓存

```
POST /admin/cache/refresh
```

清空调度缓存（`SchedulerCache`：Redis + moka）并刷新 API Key 缓存。

**响应示例：**
```json
{"status": "ok"}
```

### 供应商管理

#### 列出所有供应商

```
GET /admin/providers
```

#### 获取单个供应商

```
GET /admin/providers/{id}
```

#### 创建供应商

```
POST /admin/providers
Content-Type: application/json

{
    "name": "MyProvider",
    "base_url": "https://api.example.com"
}
```

`base_url` 会经 SSRF 安全校验（仅允许 http/https，拒绝内网地址等），校验失败返回 400。

#### 更新供应商

```
PUT /admin/providers/{id}
Content-Type: application/json

{
    "name": "MyProvider",
    "base_url": "https://api.example.com"
}
```

同样会经 SSRF 安全校验。

#### 删除供应商

```
DELETE /admin/providers/{id}
```

**响应示例：**
```json
{"status": "ok"}
```

### 供应商模型导入

#### 拉取供应商模型列表

```
GET /admin/providers/{id}/models
```

使用该供应商第一个活跃凭证，向 `{base_url}/models` 发起请求（经 SSRF 校验），从响应顶层数组 / `data` / `models` 字段中提取模型 `id`。

**响应示例：**
```json
[
    {"id": "gpt-3.5-turbo"},
    {"id": "gpt-4"}
]
```

#### 一键导入映射

```
POST /admin/providers/{id}/models/import
Content-Type: application/json

[
    {
        "model_id": "gpt-3.5-turbo",
        "provider_model_id": "gpt-3.5-turbo-0125",
        "protocols": "openai",
        "context_length": 256000
    }
]
```

导入 = 创建或更新 `provider_model_map`（**不会**自动创建 `model_config`，对应模型需先在 `/admin/models` 创建）：
- 已存在映射：更新 `provider_model_id`、`protocols`（默认 `openai`）、`context_length`
- 不存在映射：创建，默认 `is_active=true`、`priority=9`、`status="available"`、`timeout=30`、`context_length` 默认 256000
- 对应模型不存在时报错（记入 `errors`）

**响应示例：**
```json
{
    "imported": [
        {
            "name": "gpt-3.5-turbo",
            "model_config_id": 1,
            "provider_model_id": "gpt-3.5-turbo-0125"
        }
    ],
    "errors": []
}
```

### 模型管理

#### 列出所有模型

```
GET /admin/models
```

返回所有模型配置（`model_config` 表）。

**模型字段：**
| 字段 | 类型 | 说明 |
|------|------|------|
| id | int | 主键 |
| name | string | 模型对外名称 |
| timeout | int | 默认超时秒数（映射可覆盖） |
| priority | int | 优先级（越小越优先） |
| is_active | bool | 是否启用 |
| context_length | int | 默认上下文窗口大小（映射可覆盖） |
| created_time | datetime | 创建时间 |
| last_updated | datetime | 最后更新时间 |

> 供应商关联、上游 `model_id`、协议、可用状态等字段已移至 `provider_model_maps` 表，见下文「供应商模型映射管理」。

#### 获取单个模型

```
GET /admin/models/{id}
```

#### 创建模型

```
POST /admin/models
Content-Type: application/json

{
    "name": "gpt-3.5-turbo",
    "timeout": 30,
    "priority": 0,
    "context_length": 256000,
    "is_active": true
}
```

`is_active` 可选，默认 `true`。

#### 更新模型

```
PUT /admin/models/{id}
Content-Type: application/json
```

body 为任意可选字段：`name` / `timeout` / `priority` / `is_active` / `context_length`。

#### 删除模型

```
DELETE /admin/models/{id}
```

### API Key 管理

#### 列出所有 API Key

```
GET /admin/api_keys
```

#### 获取单个 API Key

```
GET /admin/api_keys/{id}
```

#### 创建 API Key

```
POST /admin/api_keys
Content-Type: application/json

{
    "key_value": null,
    "name": "My Key",
    "is_active": true
}
```

若不传 `key_value`（或传 `null`），系统自动生成格式为 `fm-` 开头 + 64 位字母数字的密钥。

#### 更新 API Key

```
PUT /admin/api_keys/{id}
Content-Type: application/json

{
    "name": "My Key",
    "is_active": true
}
```

仅支持更新 `name` 与 `is_active`（`key_value` 不可通过该端点修改）。

#### 删除 API Key

```
DELETE /admin/api_keys/{id}
```

### 供应商凭证管理

凭证存储时使用 AES-256-GCM 加密，返回时自动解密并掩码。

#### 列出凭证

```
GET /admin/provider_credentials?provider_id={id}
```

可选 `provider_id` 查询参数，按供应商过滤。

**响应示例：**
```json
[
    {
        "id": 1,
        "provider_id": 1,
        "name": "Credential 1",
        "api_key": "sk-****",
        "account": null,
        "password": null,
        "priority": 0,
        "is_active": true,
        "quota_exhausted": false,
        "created_time": "2024-01-01T00:00:00",
        "last_updated": "2024-01-01T00:00:00"
    }
]
```

- `api_key`：返回前 4 位 + `****` 掩码
- `password`：返回 `****` 掩码
- `quota_exhausted`：配额是否已耗尽（耗尽时调度自动跳过该凭证）

#### 获取单个凭证

```
GET /admin/provider_credentials/{id}
```

#### 创建凭证

```
POST /admin/provider_credentials
Content-Type: application/json

{
    "provider_id": 1,
    "name": "Credential 1",
    "api_key": "sk-...",
    "account": null,
    "password": null,
    "priority": 0,
    "is_active": true
}
```

**字段说明：**
- `api_key`：明文传入，服务端自动加密存储
- `account`：可选，部分供应商需要账号
- `password`：可选，明文传入，服务端自动加密存储

#### 更新凭证

```
PUT /admin/provider_credentials/{id}
Content-Type: application/json

{
    "provider_id": 1,
    "name": "Credential 1",
    "api_key": "sk-...",
    "account": null,
    "password": null,
    "priority": 0,
    "is_active": true
}
```

#### 删除凭证

```
DELETE /admin/provider_credentials/{id}
```

#### 重置凭证状态

```
POST /admin/provider_credentials/{id}/reset_status
```

清除该凭证的 `quota_exhausted` 标记，并重置其全部熔断条目（`CircuitBreaker::reset_credential`）。

**响应示例：**
```json
{"success": true}
```

### 供应商模型映射管理

`provider_model_map` 将「模型配置 × 供应商」关联为具体的上游模型，承载协议、超时、上下文窗口、优先级等覆盖字段。

#### 列出映射

```
GET /admin/provider_model_maps?model_id={id}&provider_id={id}
```

可选 `model_id` / `provider_id` 查询参数过滤。

**响应示例（数组元素）：**
```json
{
    "id": 1,
    "model_id": 1,
    "provider_id": 1,
    "provider_model_id": "gpt-3.5-turbo-0125",
    "is_active": true,
    "priority": 0,
    "context_length": 256000,
    "protocols": "openai",
    "status": "available",
    "timeout": 30,
    "created_time": "2024-01-01T00:00:00",
    "last_updated": "2024-01-01T00:00:00"
}
```

- `context_length` / `timeout`：可为 `null`，为 null 时转发回退使用模型配置值

#### 获取单个映射

```
GET /admin/provider_model_maps/{id}
```

#### 创建映射

```
POST /admin/provider_model_maps
Content-Type: application/json

{
    "model_id": 1,
    "provider_id": 1,
    "provider_model_id": "gpt-3.5-turbo-0125",
    "is_active": true,
    "priority": 0,
    "protocols": "openai",
    "status": "available",
    "timeout": 30,
    "context_length": 256000
}
```

- 必填：`model_id`、`provider_id`、`provider_model_id`、`protocols`
- 可选：`is_active`（默认 `true`）、`priority`（默认 0）、`status`（默认 `"available"`）、`timeout`、`context_length`

#### 更新映射

```
PUT /admin/provider_model_maps/{id}
Content-Type: application/json
```

body 为任意可选字段：`provider_model_id` / `is_active` / `priority` / `protocols` / `status` / `timeout` / `context_length`。

#### 删除映射

```
DELETE /admin/provider_model_maps/{id}
```

**响应示例：**
```json
{"status": "ok"}
```

### 使用统计

```
GET /admin/usage_log/stats?group_by=<维度>&start_time=<ISO>&end_time=<ISO>
```

获取使用量统计数据，支持多维度分组和时间筛选。

**查询参数：**
| 参数 | 必填 | 说明 |
|------|------|------|
| `group_by` | 是 | 分组维度：`provider` / `model` / `api_key` / `day` / `credential` / `provider_model` |
| `start_time` | 否 | 开始时间（ISO 格式），用于时间范围筛选 |
| `end_time` | 否 | 结束时间（ISO 格式） |

`group_by` 仅接受上述白名单，其他值返回 400。

**响应示例：**
```json
{
    "items": [
        {
            "dimension_name": "供应商名称",
            "requests": 120,
            "prompt_tokens": 50000,
            "completion_tokens": 30000,
            "total_tokens": 80000,
            "cache_hit_tokens": 20000,
            "cache_miss_tokens": 60000,
            "avg_duration_ms": 1500.5,
            "max_duration_ms": 5000.0
        }
    ],
    "total": {
        "requests": 120,
        "prompt_tokens": 50000,
        "completion_tokens": 30000,
        "total_tokens": 80000,
        "avg_duration_ms": 1500.5
    }
}
```

### Admin 公钥管理

服务端管理端鉴权使用 Ed25519 签名，公钥存储在数据库 `admin_key` 表中（fingerprint 由 Ed25519 公钥 SHA256 生成，格式 `SHA256:` 前缀 + Base64 无填充）。**当前未开放 HTTP 管理接口**，公钥管理通过环境变量引导完成：首次部署设置环境变量 `ADMIN_BOOTSTRAP_PUBLIC_KEY`（OpenSSH 单行公钥，形态如 `ssh-ed25519 AAAA... comment`，由 `ssh-keygen -t ed25519` 生成），服务端启动时解析校验该公钥、计算指纹并幂等写入 `admin_key` 表（默认备注名可用可选的 `ADMIN_BOOTSTRAP_KEY_NAME` 指定）；后续追加管理员钥匙同样只需更新该环境变量并重启服务；停用某把钥匙时将 `admin_key` 表中对应记录的 `is_active` 置为 false（重启不会被自动激活）。签名鉴权细节见 [authentication.md](./authentication.md)。

### 测试凭证连接

```
POST /admin/test_credential
Content-Type: application/json

{
    "credential_id": 1,
    "model_id": "gpt-3.5-turbo-0125",
    "prompt": "Hello"
}
```

向指定凭证对应的供应商发送测试请求，验证凭证有效性。

**响应示例：**
```json
{
    "success": true,
    "response_time_ms": 1234,
    "model_id": "gpt-3.5-turbo-0125",
    "provider_id": 1,
    "credential_id": 1,
    "error": null
}
```

**处理流程：**
1. 根据 `credential_id` 查库获取凭证
2. 根据 `credential.provider_id` 获取供应商
3. 查询该供应商的映射，找到 `provider_model_id == model_id` 的映射（`model_id` 传上游模型值）
4. 解密凭证中的 `api_key`
5. 根据映射 `protocols` 是否包含 `anthropic` 决定请求地址与鉴权头：Anthropic → `{base_url}/messages` + `x-api-key` + `anthropic-version: 2023-06-01`；否则 → `{base_url}/chat/completions` + `Authorization: Bearer`
6. 经 SSRF 校验后发送测试请求（`max_tokens: 10`）
7. 返回成功/失败及响应时间；成功时自动清除该凭证的 `quota_exhausted` 标记

---

## 错误响应格式

### OpenAI 格式

```json
{
    "error": {
        "message": "错误描述",
        "type": "error_type"
    }
}
```

错误类型：
- `invalid_request_error`：请求参数错误 / 鉴权失败
- `server_error`：服务端错误 / 服务不可用

### Anthropic 格式

```json
{
    "type": "error",
    "error": {
        "type": "error_type",
        "message": "错误描述"
    }
}
```

错误类型：
- `authentication_error`：鉴权失败
- `invalid_request_error`：请求参数错误
- `api_error`：服务端错误 / 服务不可用

### Admin 扁平格式

Admin 接口（以及数据库错误）使用扁平结构：

```json
{"error": "错误描述"}
```
