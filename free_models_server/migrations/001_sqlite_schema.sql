-- >>> schema_version: sqlite-001 (final state, merged from mysql migrations 001-006)
-- >>> 语句分隔标记：每条 SQL 以 "-- >>>" 行结束，migrate 脚本按此拆分执行
-- >>> BOOLEAN → INTEGER(0/1)；DATETIME/DATE → TEXT；主键 INTEGER = 64 位 ROWID 自增

CREATE TABLE IF NOT EXISTS provider_config (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE TABLE IF NOT EXISTS model_config (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    context_length INTEGER NOT NULL DEFAULT 256000,
    timeout INTEGER NOT NULL DEFAULT 30,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE TABLE IF NOT EXISTS api_key (
    id INTEGER PRIMARY KEY,
    key_value TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE TABLE IF NOT EXISTS admin_key (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    public_key TEXT NOT NULL,
    fingerprint TEXT NULL UNIQUE,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE TABLE IF NOT EXISTS provider_credential (
    id INTEGER PRIMARY KEY,
    provider_id INTEGER NOT NULL REFERENCES provider_config(id),
    name TEXT NOT NULL DEFAULT '',
    api_key TEXT NOT NULL,
    account TEXT NULL,
    encrypted_password TEXT NULL,
    priority INTEGER NOT NULL DEFAULT 0,
    quota_exhausted INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE INDEX IF NOT EXISTS idx_credential_provider_id ON provider_credential (provider_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_credential_is_active ON provider_credential (is_active)
-- >>>
CREATE TABLE IF NOT EXISTS provider_model_map (
    id INTEGER PRIMARY KEY,
    model_id INTEGER NOT NULL REFERENCES model_config(id) ON DELETE CASCADE,
    provider_id INTEGER NOT NULL REFERENCES provider_config(id) ON DELETE CASCADE,
    provider_model_id TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    context_length INTEGER NULL,
    protocols TEXT NOT NULL DEFAULT 'openai',
    status TEXT NOT NULL DEFAULT 'available',
    timeout INTEGER NULL,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (model_id, provider_id)
)
-- >>>
CREATE INDEX IF NOT EXISTS idx_map_model_id ON provider_model_map (model_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_map_provider_id ON provider_model_map (provider_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_map_is_active ON provider_model_map (is_active)
-- >>>
CREATE INDEX IF NOT EXISTS idx_map_status ON provider_model_map (status)
-- >>>
CREATE TABLE IF NOT EXISTS usage_log (
    id INTEGER PRIMARY KEY,
    api_key_id INTEGER NULL,
    api_key_name TEXT NULL,
    model_config_id INTEGER NULL,
    provider_config_id INTEGER NULL,
    provider_credential_id INTEGER NULL,
    model_name TEXT NOT NULL,
    provider_name TEXT NOT NULL,
    protocol TEXT NOT NULL DEFAULT 'openai',
    status TEXT NOT NULL DEFAULT 'success',
    error_message TEXT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cache_hit_tokens INTEGER NOT NULL DEFAULT 0,
    cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    is_stream INTEGER NOT NULL DEFAULT 0,
    request_timestamp TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_api_key_id ON usage_log (api_key_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_model_config_id ON usage_log (model_config_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_provider_config_id ON usage_log (provider_config_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_provider_credential_id ON usage_log (provider_credential_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_status ON usage_log (status)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_model_provider ON usage_log (model_name, provider_name)
-- >>>
CREATE INDEX IF NOT EXISTS idx_usage_request_timestamp ON usage_log (request_timestamp)
-- >>>
CREATE TABLE IF NOT EXISTS usage_log_daily (
    id INTEGER PRIMARY KEY,
    stat_date TEXT NOT NULL,
    api_key_id INTEGER NULL,
    api_key_name TEXT NULL,
    provider_config_id INTEGER NULL,
    provider_credential_id INTEGER NULL,
    provider_name TEXT NOT NULL,
    model_config_id INTEGER NULL,
    model_name TEXT NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cache_hit_tokens INTEGER NOT NULL DEFAULT 0,
    cache_miss_tokens INTEGER NOT NULL DEFAULT 0,
    avg_duration_ms INTEGER NOT NULL DEFAULT 0,
    min_duration_ms INTEGER NOT NULL DEFAULT 0,
    max_duration_ms INTEGER NOT NULL DEFAULT 0,
    created_time TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (stat_date, api_key_id, provider_config_id, model_config_id)
)
-- >>>
CREATE INDEX IF NOT EXISTS idx_daily_stat_date ON usage_log_daily (stat_date)
-- >>>
CREATE INDEX IF NOT EXISTS idx_daily_provider_config_id ON usage_log_daily (provider_config_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_daily_model_config_id ON usage_log_daily (model_config_id)
-- >>>
CREATE INDEX IF NOT EXISTS idx_daily_api_key_id ON usage_log_daily (api_key_id)
-- >>> 触发器：补齐 MySQL "ON UPDATE CURRENT_TIMESTAMP" 语义（recursive_triggers 默认 OFF，不会自递归）
CREATE TRIGGER IF NOT EXISTS trg_provider_config_updated AFTER UPDATE ON provider_config
BEGIN
    UPDATE provider_config SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
-- >>>
CREATE TRIGGER IF NOT EXISTS trg_model_config_updated AFTER UPDATE ON model_config
BEGIN
    UPDATE model_config SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
-- >>>
CREATE TRIGGER IF NOT EXISTS trg_api_key_updated AFTER UPDATE ON api_key
BEGIN
    UPDATE api_key SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
-- >>>
CREATE TRIGGER IF NOT EXISTS trg_admin_key_updated AFTER UPDATE ON admin_key
BEGIN
    UPDATE admin_key SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
-- >>>
CREATE TRIGGER IF NOT EXISTS trg_provider_credential_updated AFTER UPDATE ON provider_credential
BEGIN
    UPDATE provider_credential SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
-- >>>
CREATE TRIGGER IF NOT EXISTS trg_provider_model_map_updated AFTER UPDATE ON provider_model_map
BEGIN
    UPDATE provider_model_map SET last_updated = CURRENT_TIMESTAMP WHERE id = NEW.id;
END
