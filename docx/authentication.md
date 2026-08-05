# 认证鉴权文档

系统包含两层鉴权：外层公共 API 使用 Bearer Token 鉴权，Admin API 使用 Ed25519 签名鉴权。

---

## 1. 公共 API 鉴权（API Key）

### 鉴权流程

所有对 `/v1/models`、`/v1/chat/completions`、`/v1/messages`、`/v1/responses` 公共端点的请求需携带有效的 API Key。

```
客户端请求
    │
    ├─ AuthMiddleware 拦截
    │   ├─ 路径为 /health → 跳过鉴权
    │   ├─ 路径以 /admin/ 开头 → 跳过鉴权（由 AdminAuthMiddleware 处理）
    │   └─ 其他路径 → 提取 Bearer Token
    │
    ├─ 从 Authorization 头提取 Bearer <key>
    │   ├─ 缺少 Authorization 头 → 401（按协议格式）
    │   ├─ 格式非 "Bearer <key>" → 401（按协议格式）
    │   └─ 提取成功 → 查缓存
    │
    ├─ api_key_cache::contains(key)
    │   ├─ Redis Set（SISMEMBER）→ 存在 → 通过
    │   ├─ Redis Set（SISMEMBER）→ 不存在 → 401
    │   └─ Redis 不可用或查询出错 → moka 内存缓存查询
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

**OpenAI 协议（`/v1/chat/completions`、`/v1/models`、`/v1/responses`）：**
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

- **Redis 层：** 使用 Redis Set `app:free_models:api_keys:active` 存储所有活跃 API Key，通过 `SISMEMBER` 判断是否存在
- **内存层：** 启动时全量加载到 `moka::sync::Cache<String, ()>`，作为 Redis 不可用或查询出错时的 fallback
- **刷新：** 调用 `POST /admin/cache/refresh` 重新从数据库加载（API Key 的增删改操作成功后也会自动刷新缓存）

### 代码实现

鉴权中间件位于 [auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/auth.rs#L48-L96)：

```rust
fn call(&self, req: ServiceRequest) -> Self::Future {
    // /health 和 /admin/* 跳过鉴权
    if req.path() == "/health" || req.path().starts_with("/admin/") {
        return Box::pin(async move {
            let res = service.call(req).await?;
            Ok(res.map_into_boxed_body())
        });
    }

    // 提取 Bearer Token（失败时按请求路径返回对应协议的 401 格式）
    let key = match extract_bearer(&req) {
        Ok(k) => k,
        Err(resp) => {
            return Box::pin(async move {
                Ok(req.into_response(resp).map_into_boxed_body())
            });
        }
    };

    // 查缓存：命中放行，未命中返回 401
    Box::pin(async move {
        if api_key_cache.contains(&key).await {
            let res = service.call(req).await?;
            Ok(res.map_into_boxed_body())
        } else {
            let err = unauthorized_for(&req);
            Ok(req.into_response(err).map_into_boxed_body())
        }
    })
}
```

API Key 缓存位于 [api_key_cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/api_key_cache.rs#L45-L57)：

```rust
pub async fn contains(&self, key: &str) -> bool {
    match self.cache_store.sismember(&self.redis_key, key).await {
        Ok(true) => true,
        Ok(false) => {
            if self.cache_store.is_available() {
                false  // Redis 确认不存在
            } else {
                self.fallback.contains_key(key)
            }
        }
        Err(_) => self.fallback.contains_key(key),
    }
}
```

---

## 2. Admin API 鉴权（Ed25519 签名）

### 概述

Admin API 端点以 `/admin` 为前缀，使用 Ed25519 非对称签名进行鉴权。管理员持有 Ed25519 私钥，服务端存储对应的 OpenSSH 格式公钥。

### 请求头

| 请求头 | 必填 | 说明 |
| --- | --- | --- |
| `X-Admin-Fingerprint` | 必填 | 指纹，格式 `SHA256:xxxx` |
| `X-Admin-Timestamp` | 必填 | Unix 时间戳（秒） |
| `X-Admin-Signature` | 必填 | Ed25519 签名的 STANDARD Base64 编码 |
| `X-Admin-Nonce` | 可选 | 随机 nonce，用于防重放（前端始终携带，16 个 hex 字符） |
| `X-Admin-Body-Hash` | 可选 | 请求体 SHA-256 的 hex 编码（无请求体时不携带） |

### 鉴权流程

```
客户端请求
    │
    ├─ AdminAuthMiddleware 验证：
    │   ├─ 缺少任一必填头 → 401
    │   ├─ 时间戳解析失败 → 401 "Invalid timestamp"
    │   ├─ now - timestamp > 300（saturating）→ 401 "Timestamp expired"
    │   ├─ 携带 X-Admin-Nonce 时做防重放：
    │   │   ├─ 清理 300 秒窗口外的旧 nonce
    │   │   ├─ 窗口内已使用过 → 401 "Nonce already used"
    │   │   └─ 未使用过 → 记录本次使用时间
    │   ├─ 查 admin_key 表（fingerprint 匹配 且 is_active = true）
    │   │   ├─ 未找到 → 401 "Invalid fingerprint"
    │   │   └─ 找到 → 解析 OpenSSH 公钥
    │   │       └─ 格式非法 → 401 "Invalid public key format"
    │   ├─ 签名解码 / 长度校验
    │   │   ├─ Base64 解码失败 → 401 "Invalid signature encoding"
    │   │   └─ 长度非 64 字节 → 401 "Invalid signature length"
    │   └─ 验签
    │       ├─ payload = "METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH"
    │       ├─ NONCE / BODY_HASH 缺失时为空字符串参与签名
    │       ├─ 例如: "POST:/admin/providers:1712345678:a1b2c3d4e5f60718:d41d8cd98f00b204e9800998ecf8427e"
    │       ├─ 验签失败 → 401 "Signature verification failed"
    │       └─ 验签成功 → 继续处理请求
    │
    └─ 鉴权通过 → 转发到 Admin Handler
```

### 签名内容（payload）

服务端在 [admin_auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/admin_auth.rs#L122-L143) 中构造签名内容：

```
METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH
```

- `METHOD`：HTTP 方法（如 `POST`），来自 `req.method()`
- `PATH`：请求路径，来自 `req.path()`，不含 query string
- `TIMESTAMP`：请求头 `X-Admin-Timestamp` 的值（字符串）
- `NONCE`：请求头 `X-Admin-Nonce` 的值；未携带时为空字符串
- `BODY_HASH`：请求头 `X-Admin-Body-Hash` 的值；未携带时为空字符串

### 签名生成（客户端，Python 示例）

```python
import time
import os
import hashlib
import base64
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import ed25519

# 加载私钥
with open("private_key.pem", "rb") as f:
    private_key = serialization.load_pem_private_key(f.read(), password=None)

# 构造签名内容（五段）
method = "POST"
path = "/admin/providers"
timestamp = str(int(time.time()))
nonce = os.urandom(8).hex()          # 16 个 hex 字符（可选，但建议总是携带）
body_hash = ""                       # 无请求体时为空字符串参与签名
payload = f"{method}:{path}:{timestamp}:{nonce}:{body_hash}"

# 签名并 Base64 编码
signature = base64.b64encode(private_key.sign(payload.encode())).decode()

# 请求头（X-Admin-Nonce / X-Admin-Body-Hash 可选）
headers = {
    "X-Admin-Fingerprint": "SHA256:xxxxxxxx",
    "X-Admin-Timestamp": timestamp,
    "X-Admin-Nonce": nonce,
    "X-Admin-Signature": signature,
}
# 若有请求体，则额外携带：
# headers["X-Admin-Body-Hash"] = hashlib.sha256(body.encode()).hexdigest()
```

### 服务端验签（Rust）

验签逻辑位于 [admin_auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/admin_auth.rs#L143-L167)：

```rust
// 1. 构造 payload（五段；nonce / body_hash 缺失时为空字符串）
let nonce_part = nonce.as_deref().unwrap_or("").to_string();
let body_hash_part = body_hash.as_deref().unwrap_or("").to_string();
let payload = format!("{}:{}:{}:{}:{}", method, path, timestamp, nonce_part, body_hash_part);

// 2. 解码并校验签名长度
let signature_bytes = base64::engine::general_purpose::STANDARD.decode(&signature)?;
if signature_bytes.len() != 64 {
    return ... // 401 "Invalid signature length"
}

// 3. 从 OpenSSH 公钥解析 Ed25519 密钥
let raw_pubkey = parse_openssh_ed25519_pubkey(&admin_record.public_key)?;
let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&raw_pubkey)?;

// 4. 验签
let signature_array: [u8; 64] = signature_bytes.try_into()?;
let sig = ed25519_dalek::Signature::from_bytes(&signature_array);
verifying_key.verify(payload.as_bytes(), &sig)?;
```

### OpenSSH Ed25519 公钥解析

服务端 `parse_openssh_ed25519_pubkey` 位于 [admin_auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/admin_auth.rs#L175-L201)，解析 `ssh-ed25519 AAAAC3... comment` 格式：

```rust
pub fn parse_openssh_ed25519_pubkey(pubkey_str: &str) -> Option<[u8; 32]> {
    // 解析格式: "ssh-ed25519 AAAAC3... comment"
    let parts: Vec<&str> = pubkey_str.trim().split_whitespace().collect();
    if parts.len() < 2 { return None; }
    let encoded = parts[1];
    let decoded = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    if decoded.len() < 51 { return None; }

    // decoded 结构:
    // [4 bytes type_len][type_len bytes "ssh-ed25519"][4 bytes key_len][32 bytes key]
    // type_len 应为 11, key_len 应为 32

    let type_len = u32::from_be_bytes([decoded[0], decoded[1], decoded[2], decoded[3]]) as usize;
    if type_len != 11 { return None; }
    if &decoded[4..15] != b"ssh-ed25519" { return None; }

    let key_len = u32::from_be_bytes([decoded[15], decoded[16], decoded[17], decoded[18]]) as usize;
    if key_len != 32 { return None; }

    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded[19..51]);
    Some(key)
}
```

### 指纹计算

`compute_fingerprint` 定义在前端 [crypto.rs](file:///d:/workspace/trae/free_models_token/free_models_manager/src-tauri/src/crypto.rs#L197-L200)，流程为：Ed25519 raw 公钥（32 字节）→ SHA256 哈希 → Base64 无填充（URL-safe，`STANDARD_NO_PAD`）编码 → 添加 `SHA256:` 前缀：

```rust
fn compute_fingerprint(verifying_key: VerifyingKey) -> String {
    let hash = Sha256::digest(verifying_key.to_bytes());
    format!("SHA256:{}", STANDARD_NO_PAD.encode(hash))
}
```

服务端不自行计算指纹。该值作为 `admin_key.fingerprint` 字段（见 [admin_key.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/db/entities/admin_key.rs)，UNIQUE 约束）存入数据库；验签时通过 `find_active_by_fingerprint` 查询 `fingerprint = ? AND is_active = true` 的活跃行获取对应公钥。

### 前端签名实现（Tauri）

前端通过 Tauri Rust 后端生成签名。签名实现分两部分：

**crypto.rs —— KeyPair 封装**（[crypto.rs](file:///d:/workspace/trae/free_models_token/free_models_manager/src-tauri/src/crypto.rs#L6-L38)）：

```rust
pub struct KeyPair {
    pub signing_key: SigningKey,
    pub fingerprint: String,  // "SHA256:..." 形式
}

impl KeyPair {
    // 从 OpenSSH 私钥/公钥文件加载，校验公私钥匹配后计算指纹
    pub fn from_ssh_files(priv_key_path: &str, pub_key_path: &str) -> Result<Self, String> {
        let seed = parse_openssh_private_key(&priv_content)?; // 解析 openssh-key-v1 取 32 字节 seed
        let pub_key_bytes = parse_openssh_public_key(&pub_content)?;
        let signing_key = SigningKey::from_bytes(&seed);
        // 校验 verifying_key == 公钥文件，不一致报错
        let fingerprint = compute_fingerprint(verifying_key);
        Ok(KeyPair { signing_key, fingerprint })
    }

    // 对消息签名，返回 STANDARD Base64
    pub fn sign(&self, message: &str) -> String {
        let signature = self.signing_key.sign(message.as_bytes());
        STANDARD.encode(signature.to_bytes())
    }
}
```

**api.rs —— 构造签名请求**（[api.rs](file:///d:/workspace/trae/free_models_token/free_models_manager/src-tauri/src/api.rs#L18-L69)）：

```rust
async fn signed_request(&self, method: &str, path: &str, body: Option<String>) -> Result<reqwest::Response, String> {
    // 1. 时间戳：UNIX epoch 秒
    let timestamp = /* SystemTime 秒 */;

    // 2. 生成随机 nonce（8 字节 → 16 个 hex 字符），总是携带
    let mut nonce_bytes = [0u8; 8];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = nonce_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    // 3. body hash：有请求体时为 SHA-256 hex；无请求体时为空字符串
    let body_hash = match &body {
        Some(b) => /* Sha256(b).hexdigest() */,
        None => String::new(),
    };

    // 4. 签名只使用纯路径（不含 query string），服务端 req.path() 同样不含 query
    let sign_path = path.split('?').next().unwrap_or(path);
    let payload = format!("{}:{}:{}:{}:{}", method, sign_path, timestamp, nonce, body_hash);
    let signature = self.keypair.sign(&payload);

    // 5. 请求头：Fingerprint / Timestamp / Nonce / Signature 必带，
    //    body_hash 非空时额外携带 X-Admin-Body-Hash
    req.header("X-Admin-Fingerprint", &self.keypair.fingerprint);
    req.header("X-Admin-Timestamp", &timestamp);
    req.header("X-Admin-Nonce", &nonce);
    req.header("X-Admin-Signature", &signature);
    if !body_hash.is_empty() {
        req.header("X-Admin-Body-Hash", &body_hash);
    }
}
```

注意：前端始终生成并携带 nonce，因此前端签名的第五段 `NONCE` 恒为非空；`BODY_HASH` 在无请求体时为空字符串（此时不发送 `X-Admin-Body-Hash` 头，但空串仍参与签名），与服务端"缺失时为空字符串"的处理一致。

### 安全注意事项

1. **时间戳有效期：** 服务端校验 `now.saturating_sub(timestamp) > 300`（saturating 减法，未来时间戳不会触发）时拒绝，即时间戳距今超过 300 秒（5 分钟）视为过期
2. **Nonce 防重放：** 携带 `X-Admin-Nonce` 时，服务端在 300 秒窗口内对已使用过的 nonce 返回 401 `Nonce already used`；该窗口与时间戳窗口一致
3. **密钥安全：** 私钥应妥善保管，不应存储在数据库中
4. **公钥格式：** 仅支持 Ed25519（ssh-ed25519）密钥对，不支持 RSA 或其他类型
5. **指纹唯一性：** `admin_key.fingerprint` 有 UNIQUE 约束，验签时按 fingerprint + `is_active = true` 查询
6. **签名内容：** 签名内容为 `METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH` 五段拼接，请求体不直接参与签名，而是通过其 SHA-256 哈希（`BODY_HASH` 段）纳入签名，防篡改
7. **路径不含 query string：** 服务端使用 `req.path()`（不含 query），客户端签名时同样剥离 query string，二者保持一致

### 路由注册

Admin 路由组和鉴权中间件在 [main.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/main.rs#L90-L92) 中关联：

```rust
.service(
    handler::admin::admin_routes().wrap(AdminAuthMiddleware),
)
```
