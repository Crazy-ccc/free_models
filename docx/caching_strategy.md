# 缓存策略文档

系统采用多级缓存架构，结合 Redis 和进程内内存缓存，以降低数据库查询频率并提升响应速度。

---

## 缓存层级

```
┌──────────────────────────────────────────────────┐
│                    客户端请求                       │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│             Redis 缓存（主缓存）                    │
│  ├─ 模型调度缓存 (app:scheduler:*)                │
│  ├─ API Key Set (app:api_keys:active)            │
│  ├─ 惩罚缓存 (penalty:*)                         │
│  └─ 不可用：自动降级到内存缓存                     │
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│         进程内内存缓存（fallback）                 │
│  ├─ Redis 正常时也作为补充                         │
│  │  （提供本地二级缓存加速）                        │
│  └─ Redis 宕机时作为唯一缓存                       │
└──────────────────────────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│         free_models_store (数据访问层)             │
│          MySQL 数据库（最终数据源）                 │
└──────────────────────────────────────────────────┘
```

---

## 缓存类型

### 1. 模型调度缓存（SchedulerCache）

统一缓存模型列表以及按名称查询的 `ModelProviderInfo` 列表。取代了旧的 `ModelCache` + `ProviderCache` 分离设计。

**数据结构：**
```
Redis Key: app:scheduler:{key}
    - 全量列表: app:scheduler:all → JSON(Vec<ModelProviderInfo>)
    - 单模型:   app:scheduler:{model_name} → JSON(Vec<ModelProviderInfo>)
TTL: 30 秒（可配置，环境变量 REDIS_CACHE_TTL_MODEL_SEC）

进程内存: HashMap<String, (Instant, Vec<ModelProviderInfo>)>
```

**查询流程：**
```rust
pub async fn get(&self, key: &str) -> Option<Vec<ModelProviderInfo>> {
    // 1. 尝试 Redis
    let full_key = format!("app:scheduler:{}", key);
    if self.cache_store.is_available() {
        if let Ok(Some(value)) = self.cache_store.get(&full_key).await {
            if let Ok(models) = serde_json::from_str(&value) {
                return Some(models);
            }
        }
    }
    // 2. 退回到进程内存缓存
    let cache = self.fallback.lock().unwrap();
    if let Some((timestamp, models)) = cache.get(key) {
        if timestamp.elapsed() < self.ttl {
            return Some(models.clone());
        }
    }
    None
}
```

**写入/刷新：**
```rust
pub async fn set(&self, key: String, models: Vec<ModelProviderInfo>) {
    let full_key = format!("app:scheduler:{}", key);
    if self.cache_store.is_available() {
        let _ = self.cache_store.set_ex(&full_key, &value, self.ttl.as_secs()).await;
    }
    let mut cache = self.fallback.lock().unwrap();
    cache.insert(key, (Instant::now(), models));
}
```

### 2. API Key 缓存（ApiKeyCache）

缓存所有活跃的 API Key，用于快速鉴权。

**数据结构：**
```
Redis: SET "app:api_keys:active" 存储所有活跃 key_value
进程内存: HashSet<String> 存储所有活跃 key_value
缓存类型: 全量缓存，无过期时间
```

**加载时机：**
1. 服务启动时从数据库全量加载
2. 调用 `POST /admin/cache/refresh` 时重新加载

**查询流程：**
```rust
pub async fn contains(&self, key: &str) -> bool {
    // 1. 主路径：Redis SISMEMBER 查询
    match self.cache_store.sismember(&self.redis_key, key).await {
        Ok(true) => true,
        Ok(false) => {
            if self.cache_store.is_available() {
                false  // Redis 确认不存在
            } else {
                // 2. Redis 不可用：退回到内存 HashSet
                self.fallback.lock().unwrap().contains(key)
            }
        }
        Err(_) => {
            // 3. Redis 异常：退回到内存 HashSet
            self.fallback.lock().unwrap().contains(key)
        }
    }
}
```

### 3. 优先级惩罚缓存（CircuitBreaker）

缓存失败的（model, provider）组合，使其优先级降低。使用基于失败次数的熔断器机制。

**数据结构：**
```
Redis Key: penalty:{model_name}|{provider_name}
Redis Value: "1"
TTL: 1800 秒（30 分钟，可配置，环境变量 REDIS_CACHE_TTL_PENALTY_SEC）

内存: HashMap<(String, String), (Instant, u32)>
  - 存储 (惩罚到期时间, 失败计数)
  - 达到阈值后标记为 penalized
  - Redis 不可用时作为最终保底
```

**熔断逻辑：**
```
CircuitBreaker::new(open_ttl, threshold)
  - threshold: 连续失败次数阈值（可配置，默认 3）
  - open_ttl: 惩罚持续时间（可配置，默认 30 秒）

penalize(model, provider):
  - 递增该组合的失败计数
  - 达到 threshold → 标记为 penalized，持续 open_ttl 时间

sort_penalized_last(models):
  - 将 penalized 的 (model, provider) 排序到列表末尾
  - 非惩罚的模型保持原有 priority 排序
```

---

## 缓存刷新

### 手动刷新

```
POST /admin/cache/refresh
```

触发以下操作：
- `SchedulerCache.clear()` — 删除所有 `app:scheduler:*` Redis 键 + 清空内存
- `ApiKeyCache.refresh()` — 从数据库重新加载全量 API Key
- 遍历 Redis 删除所有 `penalty:*` 键 + 清空内存惩罚状态

### 自动过期

各缓存 TTL 到期后自动失效，下次查询时从数据库重新加载。

---

## TTL 配置汇总

| 环境变量 | 默认值 | 用途 |
|---------|--------|------|
| `REDIS_CACHE_TTL_MODEL_SEC` | 30 | 模型调度缓存过期时间（秒） |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | 600 | （未使用，由 SchedulerCache 统一管理） |
| `REDIS_CACHE_TTL_PENALTY_SEC` | 1800 | 惩罚记录过期时间（秒） |
| `CIRCUIT_BREAKER_THRESHOLD` | 3 | 熔断器连续失败阈值 |
| `CIRCUIT_BREAKER_OPEN_TTL` | 30 | 熔断器惩罚持续时间（秒） |

---

## Redis 不可用时的行为

| 场景 | 影响 |
|------|------|
| Redis 初始化失败 | 服务正常启动，所有缓存退回到进程内内存 |
| Redis 运行时故障 | 自动降级到内存缓存，功能不受影响 |
| Redis 恢复 | 需要手动调用刷新缓存或等待内存缓存过期后自动重建 |

进程内内存缓存在服务重启后丢失，重启后会从数据库重新加载。

---

## 代码位置

| 缓存类型 | 文件 |
|---------|------|
| SchedulerCache | [model_scheduler.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/model_scheduler.rs) |
| ApiKeyCache | [api_key_cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/api_key_cache.rs) |
| CircuitBreaker | [penalty.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/penalty.rs) |
| RedisManager | [cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/free_models_store/src/cache.rs) |
| CacheStore trait | [lib.rs](file:///d:/workspace/trae/free_models_token/free_models_server/free_models_store_api/src/lib.rs) |
