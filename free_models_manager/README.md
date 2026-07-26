# free_models_manager

基于 Tauri v2 的桌面管理应用，用于管理 `free_models_server` 后端代理服务的供应商、模型、API Key 等资源。

技术栈：Rust (Tauri v2) / React 18 / TypeScript / Ant Design 5 / LESS / Vite

---

## 快速启动

```bash
# 安装依赖
pnpm install

# 开发模式（启动桌面窗口 + 热重载）
pnpm tauri dev

# 生产构建
pnpm tauri build
```

### 前置条件

- Node.js 18+
- pnpm 11+
- Rust toolchain（参考 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

---

## 页面功能

### 概览

调用 `GET /admin/service/status` 获取服务状态，以彩色卡片展示统计数据：

| 指标 | 颜色 | 说明 |
|------|------|------|
| 模型数 | 蓝/绿/红 | 总数 / 活跃 / 不活跃 |
| 供应商数 | 蓝/绿/红 | 总数 / 活跃 / 不活跃 |
| API Key 数 | 蓝/绿/红 | 总数 / 活跃 / 不活跃 |

提供"刷新缓存"按钮，清空服务端所有缓存。

### 供应商

| 功能 | 说明 |
|------|------|
| 列表 | 表格展示所有供应商 |
| 创建/编辑 | 弹出表单 |
| 删除 | 二次确认弹框 |
| 一键导入模型 | 调用供应商 `{base_url}/models` 接口获取模型列表，与已导入模型去重后，弹框让用户勾选导入 |
| 凭证管理 | 每个供应商可配置多组 API Key/账号密码凭证，支持新增/编辑/删除/启停 |
| 凭证测试 | 选择凭证和模型发送测试请求，验证连接有效性 |

### 模型

| 功能 | 说明 |
|------|------|
| 列表 | Ant Design Table 原生表格，sticky 表头 |
| 状态字段 | 只读显示（`available` / `disabled` 彩色标签），仅在编辑页修改 |
| 筛选 | 内置筛选器：按状态、按供应商 |
| 创建/编辑 | 弹出表单 |
| 删除 | 二次确认弹框 |

### API Keys

| 功能 | 说明 |
|------|------|
| 列表 | 表格展示 |
| 创建 | 密钥由服务端自动生成（`fm-` 前缀 + 64 位随机字符），创建无弹框提示 |
| 编辑 | 可修改名称和状态 |
| 删除 | 二次确认弹框 |

### 统计

调用 `GET /admin/usage/stats` 获取使用量统计数据，支持多维度分组查询：

| 功能 | 说明 |
|------|------|
| 分组维度 | 按供应商 / 凭证 / 模型 / API Key / 按天 |
| 时间筛选 | 可选开始和结束日期 |
| 汇总行 | 总请求数、总 Token 数、Prompt Tokens、Completion Tokens、平均耗时 |

### 设置

- 配置 Ed25519 私钥/公钥路径，加载后显示 Fingerprint
- 配置 Admin API 服务端地址
- 保存密钥配置到本地

---

## 目录结构

```
free_models_manager/
├── package.json
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── App.tsx              # 应用根组件 + 路由
│   ├── main.tsx             # 入口
│   ├── types/index.ts       # TypeScript 类型定义
│   ├── styles/
│   │   ├── global.less      # 全局样式
│   │   └── variables.less   # 样式变量
│   ├── components/          # 通用组件
│   │   ├── Sidebar.tsx/less    # 侧边栏导航
│   │   ├── Drawer.tsx/less     # 抽屉面板（编辑表单）
│   │   ├── Toolbar.tsx/less    # 顶部工具栏
│   │   └── Toggle.tsx/less     # 开关组件
│   └── pages/
│       ├── Overview.tsx/less    # 概览仪表盘
│       ├── Stats.tsx/less       # 使用统计（多维度分组查询）
│       ├── Providers.tsx/less   # 供应商管理
│       ├── Models.tsx/less      # 模型管理
│       ├── ApiKeys.tsx/less     # API Key 管理
│       └── Settings.tsx/less    # 设置页
└── src-tauri/
    └── src/
        ├── main.rs          # Tauri 入口 + 所有 Tauri 命令
        ├── api.rs           # AdminClient 封装的 HTTP 请求
        └── crypto.rs        # Ed25519 密钥对加载 + 签名生成
```

## 鉴权流程

管理员界面通过 Ed25519 签名调用后端 Admin API：

1. 在设置页加载 Ed25519 私钥/公钥文件
2. 将公钥插入服务端 `admin_key` 表（通过 SQL 或手动）
3. 每次 Admin API 调用自动构造签名头：
   - `X-Admin-Fingerprint`：公钥 SHA256 指纹
   - `X-Admin-Timestamp`：当前 Unix 时间戳
   - `X-Admin-Signature`：`METHOD:PATH:TIMESTAMP` 的 Ed25519 签名

## 详细文档

| 文档 | 内容 |
|------|------|
| [管理员界面详解](../docx/admin_ui.md) | 组件说明、样式系统、签名流程 |
| [认证鉴权](../docx/authentication.md) | Ed25519 签名鉴权完整流程 |
| [API 端点参考](../docx/api_endpoints.md) | Admin API 完整参考 |

点击 [项目概览](../README.md) 返回根目录。
