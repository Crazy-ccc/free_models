-- 为 model_config 表新增 model_id 字段，用于实际调用上游供应商 API 时 model 字段的值
ALTER TABLE model_config ADD COLUMN model_id VARCHAR(255) NOT NULL DEFAULT '' AFTER name;

-- 提示运维：执行此脚本后需要手动回填存量行的 model_id 值
-- 例如：UPDATE model_config SET model_id = name WHERE model_id = '';
