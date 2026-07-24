-- 为 model_config 表新增 model_id 字段，用于实际调用上游供应商 API 时 model 字段的值
ALTER TABLE model_config ADD COLUMN model_id VARCHAR(255) NOT NULL DEFAULT '' AFTER name;

-- 提示运维：执行此脚本后需要手动回填存量行的 model_id 值
-- 例如：UPDATE model_config SET model_id = name WHERE model_id = '';

-- 为 model_config 表新增 timeout 字段，单位秒，默认 30
ALTER TABLE model_config ADD COLUMN timeout INT NOT NULL DEFAULT 30;

-- 为 model_config 表新增 protocols 字段，标识模型支持的协议，默认 'openai'
ALTER TABLE model_config ADD COLUMN protocols VARCHAR(255) NOT NULL DEFAULT 'openai';
