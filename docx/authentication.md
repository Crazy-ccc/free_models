# 认证鉴权文档

系统包含两层鉴权：外层公共 API 使用 Bearer Token 鉴权，Admin API 使用 Ed25519 签名鉴权。

---

## 1. 公共 API 鉴权（API Key）

### 鉴权流程

所有对 `/v1/chat/completions` 和 `/v1/messages` 公共端点的请求需携带有效的 API Key。

```
客户端请求
    │
    ├─ AuthMiddleware 拦截
    │   ├─ 路径为 /health → 跳过鉴权
    │   ├─ 路径以 /admin/ 开头 → 跳过鉴权（由 AdminAuthMiddleware 处理）
    │   └─ 其他路径 → 提取 Bearer Token
    │
    ├─ 从 Authorization 头提取 Bearer <key>
    │   ├─ 缺少 Authorization 头 → 401
    │   ├─ 格式非 "Bearer <key>" → 401
    │   └─ 提取成功 → 查缓存
    │
    ├─ api_key_cache::contains(key)
    │   ├─ Redis SET 查询 → 存在 → 通过
    │   ├─ Redis SET 查询 → 不存在 → 401
    │   └─ Redis 不可用 → 内存 HashSet 查询
    │
    └─ 鉴权通过 → 继续处理请求
```

### API Key 生成格式

```
格式: fm-{64位随机字符}
字符集: A-Z, a-z, 0-9
示例: fm-aB3xK9mP2...（共 67 字符）
```

### 错误响应

根据请求路径返回不同格式：

**OpenAI 协议（`/v1/chat/completions`）：**
```json
{
    "error": {
        "message": "Invalid API key",
        "type": "invalid_request_error"
    }
}
```

**Anthropic 协议（`/v1/messages`）：**
```json
{
    "type": "error",
    "error": {
        "type": "authentication_error",
        "message": "Invalid API key"
    }
}
```

### 缓存机制

- **Redis 层：** 使用 Redis Set `api_keys:active` 存储所有活跃 API Key
- **内存层：** 启动时全量加载到 `HashSet<String>`，作为 Redis 不可用时的 fallback
- **刷新：** 调用 `POST /admin/cache/refresh` 重新从数据库加载

### 代码实现

鉴权中间件位于 [auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/auth.rs)：

```rust
fn call(&self, req: ServiceRequest) -> Self::Future {
    // /health 和 /admin/* 跳过鉴权
    if req.path() == "/health" || req.path().starts_with("/admin/") {
        return Box::pin(async move {
            let res = service.call(req).await?;
            Ok(res.map_into_boxed_body())
        });
    }

    // 提取 Bearer Token
    let key = extract_bearer(&req)?;

    // 查缓存
    if api_key_cache.contains(&key).await {
        // 通过
    } else {
        // 401
    }
}
```

API Key 缓存位于 [api_key_cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/api_key_cache.rs)：

```rust
pub async fn contains(&self, key: &str) -> bool {
    match self.redis.sismember(&self.redis_key, key).await {
        Ok(true) => true,
        Ok(false) => {
            if self.redis.is_available() {
                false  // Redis 确认不存在
            } else {
                self.fallback.lock().unwrap().contains(key)  // Redis 不可用时查内存
            }
        }
        Err(_) => self.fallback.lock().unwrap().contains(key),  // Redis 异常时查内存
    }
}
```

---

## 2. Admin API 鉴权（Ed25519 签名）

### 概述

Admin API 端点以 `/admin` 为前缀，使用 Ed25519 非对称签名进行鉴权。管理员持有 Ed25519 私钥，服务端存储对应的公钥。

### 鉴权流程

```
客户端请求
    │
    ├─ 请求头携带三个字段：
    │   ├─ X-Admin-Fingerprint: SHA256:xxxx
    │   ├─ X-Admin-Timestamp: 1712345678
    │   └─ X-Admin-Signature: base64(signature)
    │
    ├─ AdminAuthMiddleware 验证：
    │   ├─ 缺少任一头部 → 401
    │   ├─ 时间戳超出 300 秒 → 401 "Timestamp expired"
    │   ├─ 查询 admin_key 表 → fingerprint + is_active 匹配
    │   │   ├─ 未找到 → 401 "Invalid fingerprint"
    │   │   └─ 找到 → 解析公钥
    │   └─ 验签
    │       ├─ payload = "METHOD:PATH:TIMESTAMP"
    │       ├─ 例如: "POST:/admin/providers:1712345678"
    │       ├─ 验签失败 → 401 "Signature verification failed"
    │       └─ 验签成功 → 继续处理请求
    │
    └─ 鉴权通过 → 转发到 Admin Handler
```

### 签名生成（客户端）

```python
import time
import base64
from cryptography.hazmat.primitives import serialization, hashes
from cryptography.hazmat.primitives.asymmetric import ed25519

# 加载私钥
with open("private_key.pem", "rb") as f:
    private_key = serialization.load_pem_private_key(f.read(), password=None)

# 构造签名内容
method = "POST"
path = "/admin/providers"
timestamp = str(int(time.time()))
payload = f"{method}:{path}:{timestamp}"

# 签名
signature = private_key.sign(payload.encode())

# Base64 编码
signature_b64 = base64.b64encode(signature).decode()

# 请求头
headers = {
    "X-Admin-Fingerprint": "SHA256:xxxxxxxx",
    "X-Admin-Timestamp": timestamp,
    "X-Admin-Signature": signature_b64,
}
```

### 服务端验签（Rust）

验签逻辑位于 [admin_auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/admin_auth.rs)：

```rust
// 1. 构造 payload
let payload = format!("{}:{}:{}", method, path, timestamp);

// 2. 解码 signature
let signature_bytes = base64::engine::general_purpose::STANDARD.decode(&signature)?;

// 3. 从 OpenSSH 公钥解析 Ed25519 密钥
let raw_pubkey = parse_openssh_ed25519_pubkey(&admin_record.public_key)?;
let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&raw_pubkey)?;

// 4. 验签
let sig = ed25519_dalek::Signature::from_bytes(&signature_bytes);
verifying_key.verify(payload.as_bytes(), &sig)?;
```

### OpenSSH Ed25519 公钥解析

```rust
pub fn parse_openssh_ed25519_pubkey(pubkey_str: &str) -> Option<[u8; 32]> {
    // 解析格式: "ssh-ed25519 AAAAC3... comment"
    let parts: Vec<&str> = pubkey_str.trim().split_whitespace().collect();
    let encoded = parts[1];
    let decoded = base64::decode(encoded).ok()?;

    // decoded 结构:
    // [4 bytes type_len][type_len bytes "ssh-ed25519"][4 bytes key_len][32 bytes key]
    // type_len 应为 11, key_len 应为 32

    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded[19..51]);
    Some(key)
}
```

### 指纹计算

```rust
pub fn compute_fingerprint(raw_pubkey: &[u8; 32]) -> String {
    let hash = Sha256::digest(raw_pubkey);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(&hash)
    )
}
```

### 前端签名实现（Tauri）

前端通过 Tauri Rust 后端生成签名，使用 `ed25519-dalek` 库：

```rust
// free_models_manager/src-tauri/src/crypto.rs
use ed25519_dalek::SigningKey;
use base64::Engine;

pub fn sign_request(private_key_pem: &str, method: &str, path: &str) -> (String, String) {
    let sk = parse_private_key(private_key_pem);
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let payload = format!("{}:{}:{}", method, path, timestamp);
    let signature = sk.sign(payload.as_bytes());
    let sig_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());
    (sig_b64, timestamp)
}
```

### 安全注意事项

1. **时间戳有效期：** 签名时间戳超过 300 秒（5 分钟）视为过期
2. **密钥安全：** 私钥应妥善保管，不应存储在数据库中
3. **公钥格式：** 仅支持 Ed25519 密钥对，不支持 RSA 或其他类型
4. **指纹唯一性：** `admin_key.fingerprint` 有 UNIQUE 约束
5. **签名内容：** 签名内容包含 `METHOD:PATH:TIMESTAMP`，不包含请求体

### 路由注册

Admin 路由组和鉴权中间件在 [main.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/main.rs) 中关联：

```rust
.service(
    handler::admin_handler::admin_routes().wrap(AdminAuthMiddleware),
)
```
