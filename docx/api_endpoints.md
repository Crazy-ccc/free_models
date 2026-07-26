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

返回所有可用模型列表（OpenAI 兼容格式）。仅返回 `status = 'available'` 且关联供应商有活跃凭证的模型。

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
1. 校验 `messages` 字段非空
2. 查询所有可用模型，过滤出支持 openai 协议的
3. 若指定 `model`，将匹配模型排到最前
4. 被惩罚的模型排到最后
5. 估算 prompt token，过滤上下文窗口不足的模型
6. 遍历模型列表向上游转发
7. 首次成功即返回，失败则标记惩罚并尝试下一个
8. 全部失败返回 503

**流式响应（`stream: true`）：**
- 自动设置 `stream_options: {"include_usage": true}`
- 在流中检测 `usage` 字段并记录用量日志
- 结束时发送 `data: [DONE]`
- 流中断时发送 `data: [DONE]` 关闭帧

### Anthropic Messages（Anthropic 协议）

```
POST /v1/messages
Authorization: Bearer <api_key>
Content-Type: application/json
```

标准 Anthropic Messages API 代理。

**请求体示例：**
```json
{
    "model": "claude-3-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello"}]
}
```

**处理流程：** 与 OpenAI 协议相同，但：
- 按 `anthropic` 协议筛选模型
- 添加 `x-api-key` 和 `anthropic-version: 2023-06-01` 请求头
- 错误响应使用 Anthropic 格式

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
    "penalties": []
}
```

**统计规则：**
- `models.active`：`status = 'available'` 的模型数
- `models.inactive`：`total - active`
- `providers.active`：有活跃凭证的唯一供应商数
- `apiKeys.active`：`is_active = true` 的 API Key 数

### 刷新缓存

```
POST /admin/cache/refresh
```

清空所有缓存（ModelCache、ProviderCache、ApiKeyCache、惩罚记录）。

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

#### 更新供应商

```
PUT /admin/providers/{id}
Content-Type: application/json

{
    "name": "MyProvider",
    "base_url": "https://api.example.com"
}
```

#### 删除供应商

```
DELETE /admin/providers/{id}
```

**响应示例：**
```json
{"status": "ok"}
```

### 模型管理

#### 列出所有模型

```
GET /admin/models
```

返回所有模型（含供应商名称）。

**模型字段：**
| 字段 | 类型 | 说明 |
|------|------|------|
| id | int | 主键 |
| provider_id | int | 关联供应商 ID |
| name | string | 模型对外名称 |
| model_id | string | 上游 API 实际使用的 model 值 |
| priority | int | 优先级（越小越优先） |
| status | string | 状态：`available` / `disabled` |
| timeout | int | 超时秒数 |
| protocols | string | 协议列表，逗号分隔：`openai,anthropic` |
| context_length | int | 上下文窗口大小 |
| created_time | datetime | 创建时间 |
| last_updated | datetime | 最后更新时间 |
| provider_name | string | 供应商名称（拓展字段） |

#### 获取单个模型

```
GET /admin/models/{id}
```

#### 创建模型

```
POST /admin/models
Content-Type: application/json

{
    "provider_id": 1,
    "name": "gpt-3.5-turbo",
    "model_id": "gpt-3.5-turbo",
    "timeout": 30,
    "protocols": "openai",
    "priority": 0,
    "status": "available",
    "context_length": 256000
}
```

#### 更新模型

```
PUT /admin/models/{id}
Content-Type: application/json

{
    "provider_id": 1,
    "name": "gpt-3.5-turbo",
    "model_id": "gpt-3.5-turbo",
    "timeout": 30,
    "protocols": "openai",
    "priority": 0,
    "status": "available",
    "context_length": 256000
}
```

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
    "key_value": "fm-xxxx...",
    "name": "My Key",
    "is_active": true
}
```

#### 删除 API Key

```
DELETE /admin/api_keys/{id}
```

### 供应商凭证管理

凭证存储时使用 AES-256-GCM 加密，返回时自动解密。

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
        "api_key": "sk-...",
        "account": null,
        "password": null,
        "priority": 0,
        "is_active": true,
        "created_time": "2024-01-01T00:00:00",
        "last_updated": "2024-01-01T00:00:00"
    }
]
```

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

### 使用统计

```
GET /admin/usage/stats?group_by=<维度>&start_time=<ISO>&end_time=<ISO>
```

获取使用量统计数据，支持多维度分组和时间筛选。

**查询参数：**
| 参数 | 必填 | 说明 |
|------|------|------|
| `group_by` | 是 | 分组维度：`provider` / `credential` / `model` / `api_key` / `day` |
| `start_time` | 否 | 开始时间（ISO 格式），用于时间范围筛选 |
| `end_time` | 否 | 结束时间（ISO 格式） |

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

### 管理员公钥管理

#### 列出所有公钥

```
GET /admin/admin_keys
```

#### 创建公钥

```
POST /admin/admin_keys
Content-Type: application/json

{
    "name": "My Key",
    "public_key": "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA...",
    "is_active": true
}
```

服务端会自动解析公钥计算 SHA256 指纹并回填到 `fingerprint` 字段。

#### 更新公钥

```
PUT /admin/admin_keys/{id}
Content-Type: application/json

{
    "name": "My Key",
    "is_active": true
}
```

#### 删除公钥

```
DELETE /admin/admin_keys/{id}
```

### 供应商模型映射管理

#### 列出映射

```
GET /admin/provider_model_maps?provider_id={id}
```

#### 创建映射

```
POST /admin/provider_model_maps
Content-Type: application/json

{
    "provider_id": 1,
    "model_id": "gpt-3.5-turbo",
    "remote_model_id": "gpt-3.5-turbo-0125"
}
```

#### 删除映射

```
DELETE /admin/provider_model_maps/{id}
```

### 测试凭证连接

```
POST /admin/test_credential
Content-Type: application/json

{
    "credential_id": 1,
    "model_id": "gpt-3.5-turbo",
    "prompt": "Hello"
}
```

向指定凭证对应的供应商发送测试请求，验证凭证有效性。

**响应示例：**
```json
{
    "success": true,
    "response_time_ms": 1234,
    "model_id": "gpt-3.5-turbo",
    "provider_id": 1,
    "credential_id": 1,
    "error": null
}
```

**处理流程：**
1. 根据 `credential_id` 查库获取凭证
2. 解密凭证中的 `api_key`
3. 根据 `credential.provider_id` 获取供应商信息
4. 根据 `model_id` 和 `provider_id` 获取模型配置
5. 根据模型支持的协议确定请求地址
6. 发送测试请求（`max_tokens: 10`）
7. 返回成功/失败及响应时间

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
