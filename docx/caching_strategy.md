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
│              Redis 缓存（主缓存）                  │
│  ├─ 可用：快速 O(1) 查询                          │
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
│                MySQL 数据库（最终数据源）            │
└──────────────────────────────────────────────────┘
```

---

## 缓存类型

### 1. 模型缓存（ModelCache）

缓存每个模型名称对应的 `ModelProviderInfo` 列表。

**数据结构：**
```
Redis Key: model_cache:{model_name}
Redis Value: JSON 序列化的 Vec<ModelProviderInfo>
TTL: 30 秒（可配置，环境变量 REDIS_CACHE_TTL_MODEL_SEC）

进程内存: HashMap<String, (Instant, Vec<ModelProviderInfo>)>
```

**特殊键 `__all__`：** 用于缓存所有可用模型列表（按 priority 排序），避免每次请求都查库。

**查询流程：**
```rust
pub async fn get(&self, model_name: &str) -> Option<Vec<ModelProviderInfo>> {
    // 1. 尝试 Redis
    if self.redis.is_available() {
        if let Ok(Some(value)) = self.redis.get(&key).await {
            if let Ok(models) = serde_json::from_str(&value) {
                return Some(models);
            }
        }
    }
    // 2. 退回到进程内存缓存
    let cache = self.fallback.lock().unwrap();
    if let Some((timestamp, models)) = cache.get(model_name) {
        if timestamp.elapsed() < self.ttl {
            return Some(models.clone());
        }
    }
    None
}
```

**写入/刷新：**
```rust
pub async fn set(&self, model_name: String, models: Vec<ModelProviderInfo>) {
    // 同时写入 Redis 和内存
    if self.redis.is_available() {
        let _ = self.redis.set_ex(&key, &value, self.ttl.as_secs()).await;
    }
    let mut cache = self.fallback.lock().unwrap();
    cache.insert(model_name, (Instant::now(), models));
}
```

### 2. 供应商缓存（ProviderCache）

缓存每个供应商的基本信息（名称和基础 URL）。

**数据结构：**
```
Redis Key: provider_cache:{provider_id}
Redis Value: JSON 序列化的 ProviderInfo
TTL: 600 秒（10 分钟，可配置，环境变量 REDIS_CACHE_TTL_PROVIDER_SEC）

进程内存: HashMap<i32, (Instant, ProviderInfo)>
```

**查询流程：** 与 ModelCache 相同，先查 Redis，未命中或不可用时查内存。

### 3. API Key 缓存（ApiKeyCache）

缓存所有活跃的 API Key，用于快速鉴权。

**数据结构：**
```
Redis: SET "api_keys:active" 存储所有活跃 key_value
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
    match self.redis.sismember(&self.redis_key, key).await {
        Ok(true) => true,
        Ok(false) => {
            if self.redis.is_available() {
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

### 4. 优先级惩罚缓存（PriorityPenalty）

缓存失败的（model, provider）组合，使其优先级降低。

**数据结构：**
```
Redis Key: penalty:{model_name}|{provider_name}
Redis Value: "1"
TTL: 1800 秒（30 分钟，可配置，环境变量 REDIS_CACHE_TTL_PENALTY_SEC）

本地二级缓存: HashMap<String, (Instant, bool)>
  - 二级缓存 TTL: 1 秒，用于高并发场景下减少 Redis 查询
  
内存 fallback: HashMap<String, Instant>
  - 存储惩罚到期时间
  - Redis 不可用时作为最终保底
```

**查询流程（三层缓存）：**
```rust
pub async fn is_penalized(&self, model_name: &str, provider_name: &str) -> bool {
    // 第 1 层：本地二级缓存（1 秒 TTL）
    // 第 2 层：Redis EXISTS（主要查询层）
    // 第 3 层：内存 fallback（Redis 不可用时）
}
```

---

## 缓存刷新

### 手动刷新

```
POST /admin/cache/refresh
```

触发以下操作：
- `ModelCache.clear()` — 删除所有 `model_cache:*` Redis 键 + 清空内存
- `ProviderCache.clear()` — 删除所有 `provider_cache:*` Redis 键 + 清空内存
- `ApiKeyCache.refresh()` — 从数据库重新加载全量 API Key
- `redis.del_pattern("penalty:*")` — 清除所有惩罚记录

### 自动过期

各缓存 TTL 到期后自动失效，下次查询时从数据库重新加载。

---

## TTL 配置汇总

| 环境变量 | 默认值 | 用途 |
|---------|--------|------|
| `REDIS_CACHE_TTL_MODEL_SEC` | 30 | 模型缓存过期时间（秒） |
| `REDIS_CACHE_TTL_PROVIDER_SEC` | 600 | 供应商缓存过期时间（秒） |
| `REDIS_CACHE_TTL_PENALTY_SEC` | 1800 | 惩罚记录过期时间（秒） |

---

## Redis 不可用时的行为

| 场景 | 影响 |
|------|------|
| Redis 初始化失败 | 服务正常启动，所有缓存退回到进程内内存 |
| Redis 运行时故障 | 自动降级到内存缓存，功能不受影响 |
| Redis 恢复 | 需要手动调用刷新缓存或等待内存缓存过期后自动回查 Redis |

进程内内存缓存在服务重启后丢失，重启后会从数据库重新加载。

---

## 代码位置

| 缓存类型 | 文件 |
|---------|------|
| ModelCache | [model_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/model_service.rs) |
| ProviderCache | [model_service.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/service/model_service.rs) |
| ApiKeyCache | [api_key_cache.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/api_key_cache.rs) |
| PriorityPenalty | [penalty.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/util/penalty.rs) |
| RedisManager | [redis.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/db/redis.rs) |
