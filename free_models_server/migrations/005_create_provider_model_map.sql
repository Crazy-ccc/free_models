-- provider_model_map 表：描述供应商和模型的多对多关系
CREATE TABLE IF NOT EXISTS provider_model_map (
    id INT AUTO_INCREMENT PRIMARY KEY,
    model_id INT NOT NULL,
    provider_id INT NOT NULL,
    provider_model_id VARCHAR(255) NOT NULL COMMENT '模型在目标供应商处的映射 ID',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    priority INT NOT NULL DEFAULT 0,
    context_length INT NULL COMMENT '可选，不填时使用 model_config.context_length',
    protocols VARCHAR(255) NOT NULL DEFAULT 'openai',
    status VARCHAR(16) NOT NULL DEFAULT 'available' COMMENT 'available / unavailable / deprecated',
    timeout INT NULL COMMENT '可选超时（秒），不填时使用 model_config.timeout',
    created_time DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_updated DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    FOREIGN KEY (model_id) REFERENCES model_config(id) ON DELETE CASCADE,
    FOREIGN KEY (provider_id) REFERENCES provider_config(id) ON DELETE CASCADE,
    INDEX idx_model_id (model_id),
    INDEX idx_provider_id (provider_id),
    INDEX idx_is_active (is_active),
    INDEX idx_status (status),
    UNIQUE KEY uk_model_provider (model_id, provider_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 迁移现有数据：将 model_config 中的 provider_id, model_id, protocols, status 迁移到 provider_model_map
INSERT INTO provider_model_map (model_id, provider_id, provider_model_id, is_active, priority, protocols, status, context_length, timeout)
SELECT
    id,
    provider_id,
    model_id,
    TRUE,
    priority,
    protocols,
    status,
    context_length,
    timeout
FROM model_config;

-- 修改 model_config 表：移除 provider_id, protocols, status, model_id
ALTER TABLE model_config DROP FOREIGN KEY model_config_ibfk_1;
ALTER TABLE model_config DROP COLUMN provider_id;
ALTER TABLE model_config DROP COLUMN protocols;
ALTER TABLE model_config DROP COLUMN status;
ALTER TABLE model_config DROP COLUMN model_id;

-- 为 model_config 新增 is_active 字段
ALTER TABLE model_config ADD COLUMN is_active BOOLEAN NOT NULL DEFAULT TRUE AFTER priority;
