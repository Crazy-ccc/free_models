-- 创建 admin_key 表，用于存储受信任的 SSH Ed25519 公钥
CREATE TABLE admin_key (
    id INT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL DEFAULT '',
    public_key TEXT NOT NULL,
    fingerprint VARCHAR(64) NULL UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_time DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 为 model_config 表新增 context_length 字段，用于上下文窗口校验
ALTER TABLE model_config ADD COLUMN context_length INT NOT NULL DEFAULT 256000 AFTER protocols;

-- usage_log 表，记录每次模型调用的 token 使用量
CREATE TABLE usage_log (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    api_key_id INT NULL,
    api_key_name VARCHAR(255) NULL,
    model_config_id INT NULL,
    provider_config_id INT NULL,
    provider_credential_id INT NULL,
    model_name VARCHAR(255) NOT NULL,
    provider_name VARCHAR(255) NOT NULL,
    protocol VARCHAR(32) NOT NULL DEFAULT 'openai',
    status VARCHAR(16) NOT NULL DEFAULT 'success',
    error_message TEXT NULL,
    prompt_tokens INT NOT NULL DEFAULT 0,
    completion_tokens INT NOT NULL DEFAULT 0,
    total_tokens INT NOT NULL DEFAULT 0,
    cache_hit_tokens INT NOT NULL DEFAULT 0,
    cache_miss_tokens INT NOT NULL DEFAULT 0,
    duration_ms INT NOT NULL DEFAULT 0,
    is_stream BOOLEAN NOT NULL DEFAULT FALSE,
    request_timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX idx_api_key_id (api_key_id),
    INDEX idx_model_config_id (model_config_id),
    INDEX idx_provider_config_id (provider_config_id),
    INDEX idx_provider_credential_id (provider_credential_id),
    INDEX idx_status (status),
    INDEX idx_model_provider (model_name, provider_name),
    INDEX idx_request_timestamp (request_timestamp)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- provider_credential 表，从 provider_config 中提取的凭证信息，一个 provider 可有多组凭证
CREATE TABLE IF NOT EXISTS provider_credential (
    id INT AUTO_INCREMENT PRIMARY KEY,
    provider_id INT NOT NULL,
    name VARCHAR(255) NOT NULL DEFAULT '',
    api_key VARCHAR(512) NOT NULL,
    account VARCHAR(255) NULL,
    encrypted_password VARCHAR(512) NULL,
    priority INT NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_time DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    FOREIGN KEY (provider_id) REFERENCES provider_config(id),
    INDEX idx_provider_id (provider_id),
    INDEX idx_is_active (is_active)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
