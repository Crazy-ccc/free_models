# 缓存策略文档

系统采用多级缓存架构，结合 Redis（可选）和进程内 moka 内存缓存，以降低数据库查询频率并提升响应速度。

---

## 缓存层级

```
┌──────────────────────────────────────────────────┐
│                    客户端请求                       │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│            Redis 缓存（可选，主缓存）              │
│  ├─ 调度缓存 (app:free_models:scheduler:all)     │
│  └─ API Key Set (app:free_models:api_keys:active)│
│     └─ 禁用/不可用：自动降级到 moka 内存缓存       │
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│         moka 内存缓存（本地二级缓存）              │
│  ├─ SchedulerCache fallback                      │
│  ├─ ApiKeyCache fallback                         │
│  ├─ CircuitBreaker（纯内存，无 Redis 层）         │
│  ├─ CacheAffinity（纯内存）                      │
│  └─ SsrfChecker 校验结果缓存                      │
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│              MySQL 数据库（最终数据源）            │
└──────────────────────────────────────────────────┘
```

---

## 缓存类型

### 1. 模型调度缓存（SchedulerCache）

统一缓存全量模型调度信息（`Vec<ModelScheduleInfo>`），取代了旧的 `ModelCache` + `ProviderCache` 分离设计。缓存的是完整的三级结构：模型 → 映射（`ModelProviderMap`）→ 凭证（`CredentialInfo`，含解密后的 API Key）。

**数据结构：**
```
Redis Key: app:free_models:scheduler:all → JSON(Vec<ModelScheduleInfo>)
TTL: 30 秒（可配置，环境变量 REDIS_CACHE_TTL_MODEL_SEC）

moka fallback: moka::future::Cache<String, Vec<ModelScheduleInfo>>（TTL 同为 30s）
```

**查询流程：**
```rust
async fn get(&self, key: &str) -> Option<Vec<ModelScheduleInfo>> {
    let full_key = format!("app:free_models:scheduler:{}", key);
    if self.cache_store.is_available() {
        if let Ok(Some(value)) = self.cache_store.get(&full_key).await {
            if let Ok(models) = serde_json::from_str::<Vec<ModelScheduleInfo>>(&value) {
                return Some(models);
            }
        }
    }
    self.fallback.get(key).await
}
```

**写入：**
```rust
async fn set(&self, key: String, models: Vec<ModelScheduleInfo>) {
    if let Ok(value) = serde_json::to_string(&models) {
        if self.cache_store.is_available() {
            let full_key = format!("app:free_models:scheduler:{}", key);
            let _ = self.cache_store.set_ex(&full_key, &value, self.ttl.as_secs()).await;
        }
    }
    self.fallback.insert(key, models).await;
}
```

**清除：**
```rust
pub async fn clear(&self) {
    let _ = self.cache_store.del("app:free_models:scheduler:all").await;
    self.fallback.invalidate_all();
}
```

`schedule_all_available`（[model_scheduler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/model_scheduler.rs)）先查缓存，未命中则批量查询活跃的 model_config、provider_model_map、provider、credential，构造 `Vec<ModelScheduleInfo>` 后写回缓存。

### 2. API Key 缓存（ApiKeyCache）

缓存所有活跃的 API Key，用于快速鉴权。

**数据结构：**
```
Redis: SET "app:free_models:api_keys:active" 存储所有活跃 key_value
moka fallback: moka::sync::Cache<String, ()>（容量上限 API_KEY_CACHE_MAX_CAPACITY，默认 10000）
缓存类型: 全量缓存，无过期时间
```

**加载时机：**
1. 服务启动时从数据库全量加载（`ApiKeyCache::load_all`）
2. 调用 `POST /admin/cache/refresh` 时重新加载（`ApiKeyCache::refresh`）

**查询流程：**
```rust
pub async fn contains(&self, key: &str) -> bool {
    match self.cache_store.sismember(&self.redis_key, key).await {
        Ok(true) => true,
        Ok(false) => {
            if self.cache_store.is_available() {
                false  // Redis 确认不存在
            } else {
                // Redis 不可用：退回到 moka 内存
                self.fallback.contains_key(key)
            }
        }
        Err(_) => self.fallback.contains_key(key),
    }
}
```

### 3. 熔断器（CircuitBreaker）

熔断器是**纯内存实现**（`moka::sync::Cache`），**不依赖 Redis**，也没有"优先级惩罚缓存"。key 格式为 `model|provider|credential_id`。

**数据结构：**
```
内存: moka::sync::Cache<String, CircuitEntry>（容量上限 CIRCUIT_BREAKER_MAX_CAPACITY，默认 10000）
  - CircuitEntry: failure_count、circuit_state(Closed/Open/HalfOpen)、half_open_allowed、fallback(Open 到期时间)
  - key: "{model_name}|{provider_name}|{credential_id}"
```

**状态机：**
```
Closed（正常）
  └─ record_failure 累计失败；failure_count >= threshold（默认 3）→ Open
Open（熔断）
  ├─ is_allowed 返回 false，请求跳过该凭证
  └─ 到期（open_ttl，默认 30 秒）→ 转为 HalfOpen 并放行单次探测
HalfOpen（半开）
  ├─ 探测请求放行（half_open_allowed 置 false，单次探测语义）
  ├─ record_success → 重置为 Closed
  └─ record_failure → 再次 Open
```

**方法语义：**
```rust
circuit_breaker.record_failure(model, provider, cred_id)  // 失败计数 +1，达到阈值 → Open
circuit_breaker.record_success(model, provider, cred_id)  // 清零计数，恢复 Closed
let (allowed, state) = circuit_breaker.is_allowed(model, provider, cred_id)  // 是否允许转发
circuit_breaker.reset_credential(cred_id)   // 清除某凭证的全部熔断记录
circuit_breaker.list_blocked_models()       // 列出当前 Open 的 (model, provider, credential, 剩余秒数)
```

失败记录发生的场景：转发重试后仍失败、凭证级失败（401/402/403 或 quota 错误）、流式/非流式响应中的内嵌错误。

### 4. 缓存亲和（CacheAffinity）

纯内存 moka 缓存，记录"某个 API Key + 模型"最近一次成功转发的目标，后续请求优先复用同一上游以提升缓存命中率。

**数据结构：**
```
内存: moka::sync::Cache<(i32, String), CacheEntry>
  - key: (api_key_id, model_name)
  - value: CacheEntry { provider_name, provider_credential_id }
TTL: 300 秒（CACHE_AFFINITY_TTL_SEC），容量上限 CACHE_AFFINITY_MAX_CAPACITY（默认 10000）
```

**方法语义：**
```rust
cache_affinity.record_success(api_key_id, model_name, provider_name, provider_credential_id)
    // 成功转发后记录亲和目标
cache_affinity.get_affinity(api_key_id, model_name) -> Option<CacheEntry>
    // 查询亲和记录，proxy_service::reorder_by_affinity 据此将对应 provider/credential 排前
cache_affinity.count_by_credential(provider_credential_id) -> usize
    // 统计关联到某凭证的 API Key 数量，用于在无亲和记录时均衡负载
```

### 5. SSRF 校验缓存

`SsrfChecker`（[proxy_ssrf.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/proxy_ssrf.rs)）在转发前校验上游 URL：解析 host 后检查是否指向私网 IP（loopback/private/link-local 等），DNS 解析失败时 fail-closed（拒绝转发）。校验结果按 host 缓存，避免每个请求重复 DNS 解析。

**数据结构：**
```
内存: moka::sync::Cache<String, bool>
  - key: host（如 api.openai.com）
  - value: 是否安全
TTL: 300 秒，容量上限 SSRF_CACHE_MAX_CAPACITY（默认 10000）
```

---

## 缓存刷新

### 手动刷新

```
POST /admin/cache/refresh
```

触发以下操作（[stats.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/handler/admin/stats.rs)）：
- `SchedulerCache.clear()` — 删除 Redis `app:free_models:scheduler:all` 键 + 清空 moka fallback
- `ApiKeyCache.refresh()` — 从数据库重新加载全量 API Key（重建 moka + 同步 Redis Set）

**注意：熔断器（CircuitBreaker）、缓存亲和（CacheAffinity）与 SSRF 校验缓存不在刷新范围内**，只能靠 TTL 自然过期。

### 自动过期

各缓存 TTL 到期后自动失效，下次查询时从数据库（或上游 DNS）重新加载。

---

## TTL 配置汇总

| 环境变量 | 默认值 | 用途 |
|---------|--------|------|
| `REDIS_CACHE_TTL_MODEL_SEC` | 30 | 模型调度缓存（Redis + moka）过期时间（秒） |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | 600 | **未实际使用**（调度缓存已统一管理，config.rs 未读取） |
| `REDIS_CACHE_TTL_PENALTY_SEC` | 1800 | **未实际使用**（熔断器为纯内存实现，config.rs 未读取） |
| `CIRCUIT_BREAKER_THRESHOLD` | 3 | 熔断器连续失败阈值 |
| `CIRCUIT_BREAKER_OPEN_TTL` | 30 | 熔断 Open 持续时间（秒） |
| `CIRCUIT_BREAKER_MAX_CAPACITY` | 10000 | 熔断器内存缓存容量上限 |
| `CACHE_AFFINITY_MAX_CAPACITY` | 10000 | 缓存亲和内存缓存容量上限 |
| `CACHE_AFFINITY_TTL_SEC` | 300 | 缓存亲和记录过期时间（秒） |
| `API_KEY_CACHE_MAX_CAPACITY` | 10000 | API Key 内存 fallback 容量上限 |
| `SSRF_CACHE_MAX_CAPACITY` | 10000 | SSRF 校验结果缓存容量上限 |

---

## Redis 不可用时的行为

| 场景 | 影响 |
|------|------|
| `REDIS_ENABLED=false` | 服务正常启动，调度缓存与 API Key 缓存退回到 moka 内存 |
| Redis 初始化失败 | `RedisManager.client = None`，同样透明降级 |
| Redis 运行时故障 | 读操作返回空/异常时回退到 moka 内存缓存 |
| Redis 恢复 | 调度缓存等 moka 条目过期后自动重建；也可手动调用 `POST /admin/cache/refresh` |

进程内 moka 缓存在服务重启后丢失，重启后会从数据库重新加载。

---

## 代码位置

| 缓存类型 | 文件 |
|---------|------|
| SchedulerCache | [model_scheduler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/model_scheduler.rs) |
| ApiKeyCache | [api_key_cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/api_key_cache.rs) |
| CircuitBreaker | [penalty.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/penalty.rs) |
| CacheAffinity | [cache_affinity.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/cache_affinity.rs) |
| SsrfChecker | [proxy_ssrf.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/proxy_ssrf.rs) |
| RedisManager | [cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/cache.rs) |
| 缓存刷新入口 | [stats.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/handler/admin/stats.rs) |
