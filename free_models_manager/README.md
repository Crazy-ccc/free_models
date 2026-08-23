# free_models_manager

基于 Tauri v2 的桌面管理应用，用于管理 `free_models_server` 后端代理服务的供应商、模型、API Key 等资源。

技术栈：Rust (Tauri v2) / React 18 / TypeScript / LESS / Vite 5

> **UI 组件说明**：界面组件为项目自研的 "Doodle" 手绘涂鸦风格组件库（`src/components/doodle/`），**不依赖 Ant Design**（`package.json` 中未声明 `antd` 依赖，运行时依赖仅为 `@tauri-apps/api`、`react`、`react-dom`）。部分表单控件沿用了 `ant-input` / `ant-select` / `ant-btn` 等样式类名，但无对应运行时依赖。

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
- pnpm 11+（`package.json` 中通过 `packageManager` 锁定 11.16.0）
- Rust toolchain（参考 [Tauri 官方文档](https://v2.tauri.app/start/prerequisites/)）

### 常用校验命令

```bash
pnpm build              # tsc 类型检查 + vite 构建（TS 错误会导致构建失败）
npx tsc --noEmit        # 仅 TypeScript 类型检查
```

---

## 页面功能

### 概览

调用 `GET /admin/service/status` 获取服务状态，以彩色卡片展示统计数据：

| 指标 | 说明 |
|------|------|
| 服务状态 | `Healthy` / `Down` |
| 模型数 | 总数 / 可用 / 不可用 |
| 供应商数 | 总数 / 可用 / 不可用 |
| API Key 数 | 总数 / 可用 / 不可用 |

额外展示：

- **昨日 Token 消耗表**：调用 `GET /admin/usage_log/stats?group_by=provider_model&start_time=昨日&end_time=昨日`，按"供应商 / 模型"聚合展示（请求数、Prompt/Completion/Total Tokens、缓存命中与命中率）
- **惩罚中的模型**：展示 `penalties` 列表（模型名、供应商名、剩余冷却分钟）

提供"刷新缓存"按钮，调用 `POST /admin/cache/refresh` 清空服务端所有缓存。

### 供应商

| 功能 | 说明 |
|------|------|
| 列表 | 表格展示所有供应商，支持按名称/Base URL 搜索 |
| 创建/编辑 | 抽屉表单（名称 + Base URL） |
| 删除 | 二次确认弹框 |
| 一键导入模型 | 调用 `GET /admin/providers/{id}/models` 获取供应商模型列表，与已导入映射（`provider_model_map`）去重后弹框勾选导入，可配置映射到全局模型、协议多选与上下文长度；确认后调用 `POST /admin/providers/{id}/models/import` |
| 凭证管理 | 每个供应商可配置多组凭证（名称/API Key/账号/密码/优先级/启停），支持新增/编辑/删除 |
| 凭证状态 | 凭证表格展示 `quota_exhausted` 状态：额度耗尽显示红色"额度耗尽"标签，否则"正常" |
| 凭证状态重置 | 额度耗尽的凭证显示"重置状态"按钮，调用 `POST /admin/provider_credentials/{id}/reset_status` |
| 凭证测试 | 选择凭证和模型发送测试请求，调用 `POST /admin/test_credential`，验证连接有效性并显示响应时间 |

### 模型

| 功能 | 说明 |
|------|------|
| 列表 | 表格展示，列为名称、优先级、超时、上下文长度、启用、操作（**无供应商列**）；优先级列可点击排序 |
| 创建/编辑 | 抽屉表单（名称、优先级、超时秒数、上下文长度、启用 `is_active`） |
| 删除 | 二次确认弹框 |
| 供应商映射 | 点击"供应商映射"进入映射管理子页，管理 `provider_model_map` 记录（供应商、Provider Model ID、协议、优先级、状态、上下文长度、超时、启用） |
| 协议多选 | 映射表单中通过复选框组多选 `openai` / `anthropic` / `responses`（标签为英文原值），逗号分隔存储 |
| 映射状态 | 可用 / 不可用 / 废弃（`available` / `unavailable` / `deprecated`），彩色标签展示 |

### API Keys

| 功能 | 说明 |
|------|------|
| 列表 | 表格展示：名称、Key（掩码显示 + 复制按钮）、启用、创建时间 |
| 创建 | 密钥由服务端自动生成（`fm-` 前缀 + 随机字符），**创建成功后弹框展示完整 Key**，提示立即复制保存（关闭后不再显示） |
| 编辑 | 可修改名称和状态；Key Value 只读展示，可切换显示/隐藏 |
| 删除 | 二次确认弹框 |

### 统计

调用 `GET /admin/usage_log/stats` 获取使用量统计数据，支持多维度分组查询：

| 功能 | 说明 |
|------|------|
| 分组维度 | 按供应商 / 凭证 / 模型 / API Key / 按天（`provider` / `credential` / `model` / `api_key` / `day`） |
| 服务端白名单 | `provider`、`model`、`api_key`、`day`、`credential`、`provider_model`（6 维） |
| 时间筛选 | 可选开始日期（`start_time`）和结束日期（`end_time`） |
| 汇总行 | 总请求数、总 Token 数、Prompt Tokens、Completion Tokens、平均耗时 |
| 明细表 | 维度名、请求数、Prompt/Completion/总 Tokens、Cache Hit/Cache Miss、缓存命中率、平均耗时、最大耗时 |
| 排序 | 按"天"分组时按日期倒序返回 |

### 设置

- 配置 Admin API 后端服务地址，保存到本地
- 配置 Ed25519 私钥/公钥文件路径（`ssh-keygen -t ed25519` 生成），加载后显示公钥 SHA256 指纹
- 保存密钥配置到本地，加载失败时展示错误提示

---

## 目录结构

```
free_models_manager/
├── package.json               # 依赖与脚本（无 antd 依赖）
├── vite.config.ts             # Vite 5 配置（LESS javascriptEnabled、端口 1420）
├── tsconfig.json
├── src/
│   ├── App.tsx                # 应用根组件（PageKey 状态路由）
│   ├── main.tsx               # 入口
│   ├── types/index.ts         # TypeScript 类型定义
│   ├── styles/
│   │   ├── global.less        # 全局样式
│   │   ├── variables.less     # 手绘涂鸦 Design Token
│   │   └── cjk-fonts.less     # CJK 字体
│   ├── components/
│   │   ├── Sidebar.tsx/less    # 侧边栏导航
│   │   ├── Drawer.tsx/less     # 抽屉面板（编辑表单）
│   │   ├── Toolbar.tsx/less    # 顶部工具栏
│   │   ├── Toggle.tsx/less     # 开关组件
│   │   └── doodle/             # 自研手绘组件库
│   │       ├── DoodleButton.tsx/less
│   │       ├── DoodleInput.tsx/less
│   │       ├── DoodleSelect.tsx/less
│   │       ├── DoodleCheckbox.tsx/less
│   │       ├── DoodleCheckboxGroup.tsx/less
│   │       ├── DoodleTag.tsx/less
│   │       ├── DoodleModal.tsx/less
│   │       ├── DoodleMessage.tsx/less
│   │       ├── DoodleEmpty.tsx/less
│   │       └── index.ts
│   └── pages/
│       ├── Overview.tsx/less    # 概览仪表盘
│       ├── Providers.tsx/less   # 供应商管理（含凭证管理子页）
│       ├── Models.tsx/less      # 模型管理（含供应商映射子页）
│       ├── ApiKeys.tsx/less     # API Key 管理
│       ├── Stats.tsx/less       # 使用统计（多维分组查询）
│       └── Settings.tsx/less    # 设置页
└── src-tauri/
    └── src/
        ├── main.rs          # Tauri 入口 + 全部 Tauri 命令
        ├── api.rs           # AdminClient 封装的 HTTP 请求 + 签名
        └── crypto.rs        # OpenSSH Ed25519 密钥对加载 + 指纹计算
```

## 鉴权流程

管理员界面通过 Ed25519 签名调用后端 Admin API：

1. 在设置页加载 Ed25519 私钥/公钥文件（`ssh-keygen -t ed25519` 生成的 OpenSSH 格式）
2. 将公钥插入服务端 `admin_key` 表
3. 每次 Admin API 调用自动构造签名请求头：
   - `X-Admin-Fingerprint`：公钥 SHA256 指纹（`SHA256:` 前缀）
   - `X-Admin-Timestamp`：当前 Unix 时间戳
   - `X-Admin-Nonce`：每次请求随机生成的 16 位 hex 字符串
   - `X-Admin-Signature`：对 `METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH` 负载的 Ed25519 签名（PATH 不含 query；有请求体时计算 SHA-256 body hash）
   - `X-Admin-Body-Hash`：请求体 SHA-256 摘要（有请求体时携带）

## 详细文档

| 文档 | 内容 |
|------|------|
| [管理员界面详解](../docx/admin_ui.md) | 组件说明、样式系统、签名流程 |
| [认证鉴权](../docx/authentication.md) | Ed25519 签名鉴权完整流程 |
| [API 端点参考](../docx/api_endpoints.md) | Admin API 完整参考 |
| [模型代理全链路梳理](../docx/model_proxy_chain.md) | 模型配置到上游调用的完整链路 |

点击 [项目概览](../README.md) 返回根目录。
