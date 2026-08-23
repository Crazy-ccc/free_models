#!/bin/sh
set -e

# 容器以 root 启动时：修正 /data 卷属主（兼容 named volume / bind mount / 单文件挂载的 root 属主残留），然后降权执行主程序
if [ "$(id -u)" = "0" ]; then
    chown -R nobody:nobody /data
    exec su-exec nobody:nobody "$@"
fi

exec "$@"