# 管理员界面文档

管理员界面基于 Tauri v2 + React 18 + TypeScript + LESS + Vite 5 构建，提供供应商、模型、API Key 等资源的可视化管理功能。

> **UI 组件说明**：界面组件全部为项目自研的 "Doodle" 手绘涂鸦风格组件库（`src/components/doodle/`），**不依赖 Ant Design**（`free_models_manager/package.json` 中未声明 `antd` 依赖，运行时依赖仅为 `@tauri-apps/api`、`react`、`react-dom`）。部分表单控件沿用了 `ant-input` / `ant-select` / `ant-btn` 等样式类名，但无对应运行时依赖。

---

## 架构概览

```
free_models_manager/
├── package.json               # 依赖与脚本（无 antd 依赖）
├── vite.config.ts             # Vite 5 配置（LESS javascriptEnabled、端口 1420）
├── tsconfig.json
├── src/
│   ├── App.tsx                # 应用根组件：PageKey 状态路由 + 全局布局
│   ├── main.tsx               # 入口
│   ├── types/index.ts         # TypeScript 类型定义（Provider/Model/ApiKey 等）
│   ├── components/            # 通用 UI 组件
│   │   ├── Sidebar.tsx/less   # 侧边栏导航（含服务连接状态）
│   │   ├── Drawer.tsx/less    # 抽屉面板（新增/编辑表单）
│   │   ├── Toolbar.tsx/less   # 顶部工具栏（标题 + 搜索 + 操作按钮）
│   │   ├── Toggle.tsx/less    # 开关组件
│   │   └── doodle/            # 自研手绘组件库
│   │       ├── DoodleButton.tsx/less
│   │       ├── DoodleInput.tsx/less
│   │       ├── DoodleSelect.tsx/less
│   │       ├── DoodleCheckbox.tsx/less
│   │       ├── DoodleCheckboxGroup.tsx/less
│   │       ├── DoodleTag.tsx/less
│   │       ├── DoodleModal.tsx/less
│   │       ├── DoodleMessage.tsx/less
│   │       ├── DoodleEmpty.tsx/less
│   │       └── index.ts       # 统一导出
│   ├── pages/                 # 六个页面
│   │   ├── Overview.tsx/less  # 概览仪表盘
│   │   ├── Providers.tsx/less # 供应商管理（含凭证管理子页）
│   │   ├── Models.tsx/less    # 模型管理（含供应商映射子页）
│   │   ├── ApiKeys.tsx/less   # API Key 管理
│   │   ├── Stats.tsx/less     # 使用统计
│   │   └── Settings.tsx/less  # 设置页
│   ├── styles/
│   │   ├── global.less        # 全局样式
│   │   ├── variables.less     # 手绘涂鸦 Design Token
│   │   └── cjk-fonts.less     # CJK 字体
│   └── assets/fonts/          # 手绘字体（Ma Shan Zheng / ZCOOL KuaiLe / Gochi Hand / Comic Neue）
└── src-tauri/
    └── src/
        ├── main.rs            # Tauri 入口 + 全部 Tauri 命令
        ├── api.rs             # AdminClient：封装 HTTP 请求 + Ed25519 签名
        └── crypto.rs          # OpenSSH Ed25519 密钥对解析 + 指纹计算
```

### Tauri 后端（src-tauri）

Tauri 后端提供三项核心能力：

1. **网络请求封装（api.rs）：** `AdminClient` 封装所有 Admin API 调用，自动添加 Ed25519 签名鉴权请求头（含 nonce 与 body hash）
2. **签名生成（crypto.rs）：** 解析 OpenSSH 格式的 Ed25519 密钥对（`ssh-keygen -t ed25519` 生成），校验公私钥匹配，计算 SHA256 指纹
3. **进程管理（main.rs）：** Tauri 应用生命周期管理 + `invoke_handler` 注册全部 Tauri 命令（`get_service_status`、`fetch_providers`、`reset_provider_credential_status`、`fetch_usage_log_stats` 等 33 个）

### 前端路由

`App.tsx` 不依赖 react-router，通过 `PageKey` 状态切换渲染对应页面：

| 路径 | 页面 | 功能 |
|------|------|------|
| `/` | Overview | 概览仪表盘：服务状态、资源统计、昨日 Token 消耗、惩罚中的模型 |
| `/providers` | Providers | 供应商 CRUD + 一键导入模型 + 凭证管理（含测试与状态重置） |
| `/models` | Models | 模型列表（无供应商列）+ 供应商映射管理 |
| `/api-keys` | ApiKeys | API Key CRUD（密钥自动生成 + 创建后展示） |
| `/stats` | Stats | 使用统计，多维分组查询 |
| `/settings` | Settings | 服务地址与 Ed25519 密钥配置 |

侧边栏导航顺序为：概览 → 供应商 → 模型 → API Keys → 统计 → 设置。

---

## 页面详解

### 1. Overview（概览页）

文件：[Overview.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Overview.tsx)

调用 `GET /admin/service/status`（Tauri 命令 `get_service_status`）获取服务状态。

**展示指标：**

| 指标 | 说明 |
|------|------|
| 服务状态 | `Healthy` / `Down` 状态点 + 文本 |
| 模型数 | 总数 / 可用 / 不可用（活跃模型 = 活跃 model_config + 活跃 provider_model_map + 活跃 provider_credential 三者齐备） |
| 供应商数 | 总数 / 可用 / 不可用（可用 = 存在活跃凭证的唯一供应商） |
| API Keys 数 | 总数 / 可用 / 不可用 |

**昨日 Token 消耗表：** 页面加载时调用 `GET /admin/usage_log/stats?group_by=provider_model&start_time=昨日&end_time=昨日`，按"供应商 / 模型"聚合展示昨日数据，列包括：供应商、模型、请求数、Prompt Tokens、Completion Tokens、Total Tokens、缓存命中、缓存命中率。

**惩罚中的模型：** 展示 `status.penalties` 列表（`modelName` / `providerName` / `credentialId` / `remainingSecs`），每行显示"模型名 | 供应商名"及剩余冷却时间（按分钟向上取整）。

**缓存刷新：** "刷新缓存"按钮调用 `POST /admin/cache/refresh`（Tauri 命令 `refresh_cache`）。

### 2. Providers（供应商页）

文件：[Providers.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Providers.tsx)

**供应商主列表：**

| 功能 | 实现 |
|------|------|
| 列出供应商 | 调用 `GET /admin/providers`，支持按名称/Base URL 搜索 |
| 创建供应商 | 抽屉表单（名称 + Base URL），调用 `POST /admin/providers` |
| 编辑供应商 | 抽屉表单，调用 `PUT /admin/providers/{id}` |
| 删除供应商 | 二次确认弹框，调用 `DELETE /admin/providers/{id}` |
| 一键导入模型 | 见下方流程 |
| 凭证管理 | 进入凭证管理子页（见下） |

**凭证管理子页（CredentialsPage）：**

| 功能 | 实现 |
|------|------|
| 凭证列表 | 表格展示：名称、Key（掩码）、账号、优先级、状态、启用、操作 |
| 状态标记 | `quota_exhausted = true` 显示红色"额度耗尽"标签，否则显示"正常" |
| 凭证 CRUD | 新增/编辑（名称、API Key、账号、密码、优先级、状态）、删除，调用 `/admin/provider_credentials` 系列端点 |
| 启停 | Toggle 开关切换 `is_active`（调用 `PUT /admin/provider_credentials/{id}`） |
| 重置状态 | 仅额度耗尽的凭证显示"重置状态"按钮，调用 `POST /admin/provider_credentials/{id}/reset_status`，成功后提示"状态已重置" |
| 凭证测试 | 见下方流程 |

**凭证测试流程：**
1. 选择凭证点击"测试"（有防重复点击控制）
2. 若该供应商只有一个已映射模型则直接测试；多个模型则弹框选择测试模型
3. 调用 `POST /admin/test_credential`（body：`credential_id` / `model_id` / `prompt`，prompt 固定为"你好"）
4. 显示结果：成功（含响应时间 `response_time_ms`）或失败（含错误信息）

**一键导入模型流程：**
1. 点击"一键添加模型"按钮
2. 并行调用 `GET /admin/providers/{id}/models` 与 `GET /admin/provider_model_maps?provider_id={id}`
3. 获取失败 → 提示"该供应商不支持一键导入模型"
4. 获取成功 → 弹框展示模型列表，已导入模型（按 `provider_model_id` 去重）标记"已导入"且不可选
5. 每个勾选的模型可配置：映射到全局模型（必选，支持搜索）、协议多选（openai/anthropic/responses）、上下文长度（默认 256000）；也可通过"新建模型"按钮先创建全局模型
6. 确认导入 → 调用 `POST /admin/providers/{id}/models/import`，成功提示导入数量

### 3. Models（模型页）

文件：[Models.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Models.tsx)

**模型列表（model_config）：** 表格列为 **名称、优先级、超时、上下文长度、启用、操作**，**无供应商列**。优先级列可点击切换升/降序。

| 字段 | 说明 |
|------|------|
| 名称 | 模型对外名称 |
| 优先级 | 数值越小越优先 |
| 超时 | 超时秒数 |
| 上下文长度 | context_length 值 |
| 启用 | Toggle 开关（`is_active`） |
| 操作 | 编辑 / 供应商映射 / 删除 |

**创建/编辑表单：** 名称、优先级、超时秒数、上下文长度、启用（`is_active`）。

**供应商映射子页（ModelMappingsPage）：** 管理 `provider_model_map` 记录：

| 功能 | 实现 |
|------|------|
| 映射列表 | 表格：供应商、Provider Model ID、协议、优先级、上下文长度、超时、状态、启用、操作 |
| 映射 CRUD | 新增/编辑/删除，调用 `/admin/provider_model_maps` 系列端点 |
| 协议 | `DoodleCheckbox.Group` 多选：`openai` / `anthropic` / `responses`（选项标签为英文原值，无中文标签），逗号分隔存储 |
| 状态 | 下拉选择：可用 / 不可用 / 废弃（`available` / `unavailable` / `deprecated`），以彩色标签展示 |
| 启用 | Toggle 开关（`is_active`） |

### 4. Stats（统计页）

文件：[Stats.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Stats.tsx)

调用 `GET /admin/usage_log/stats`（Tauri 命令 `fetch_usage_log_stats`），支持多维分组与时间筛选：

| 功能 | 说明 |
|------|------|
| 分组维度 | 前端可选：按供应商 / 凭证 / 模型 / API Key / 按天（`provider` / `credential` / `model` / `api_key` / `day`） |
| 服务端白名单 | 后端仅接受：`provider`、`model`、`api_key`、`day`、`credential`、`provider_model`（6 维，`provider_model` 供概览页使用） |
| 时间筛选 | 可选开始日期（`start_time`）与结束日期（`end_time`），未选则查全部 |
| 汇总行 | 总请求数、总 Token 数、Prompt Tokens、Completion Tokens、平均耗时 |
| 明细表 | 维度名、请求数、Prompt/Completion/总 Tokens、Cache Hit、Cache Miss、缓存命中率（>60% 高亮）、平均耗时、最大耗时 |
| 排序 | 按"天"分组时服务端按日期**倒序**返回；其余维度按对应 id 升序 |

工具条提供"查询"与"重置筛选"按钮。

### 5. ApiKeys（API Key 页）

文件：[ApiKeys.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/ApiKeys.tsx)

| 功能 | 实现 |
|------|------|
| 列表 | 表格：名称、Key（掩码显示 + 复制按钮）、启用、创建时间、操作 |
| 创建 | 抽屉表单仅填名称与状态，`key_value` 由服务端自动生成（`fm-` 前缀），**创建成功后弹出 DoodleModal 展示完整 Key**，提示"请立即复制保存此 Key，关闭后将不再显示" |
| 编辑 | 可修改名称与状态；Key Value 只读展示，可切换显示/隐藏 |
| 删除 | 二次确认弹框 |
| 复制 | 列表行内复制图标，点击写入剪贴板并提示"已复制到剪贴板" |

### 6. Settings（设置页）

文件：[Settings.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Settings.tsx)

| 功能 | 实现 |
|------|------|
| 服务地址 | 配置 Admin API 后端服务地址，保存到 `localStorage` |
| Ed25519 密钥配置 | 填写私钥/公钥文件路径，点击"保存密钥"调用 `load_keypair` 命令（加载到内存 + 存入 `localStorage`），并显示对应公钥的 SHA256 指纹 |
| 错误提示 | 密钥加载失败时显示错误卡片，可关闭 |
| 提示 | 使用 `ssh-keygen -t ed25519` 生成密钥对，将公钥插入服务端 `admin_key` 表 |

---

## 通用组件

### Sidebar（侧边栏）

文件：[Sidebar.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Sidebar.tsx)

- 品牌区："✏️ Free Models / 模型中转 · 使用手册"
- 垂直导航项：概览 🎏、供应商 🧭、模型 🤖、API Keys 🗝️、统计 📈、设置 🖊️
- 当前选中项高亮
- 底部服务连接状态指示（"服务已连接" / "服务未连接"）

### Drawer（抽屉）

文件：[Drawer.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Drawer.tsx)

自研抽屉组件：遮罩 + 侧滑面板，头部标题与关闭按钮，底部"取消/保存"按钮，用于各页新增/编辑表单。

### Toolbar（工具栏）

文件：[Toolbar.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Toolbar.tsx)

顶部工具栏：手绘下划线标题 + 可选搜索框 + 操作按钮区。

### Toggle（开关）

文件：[Toggle.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Toggle.tsx)

自研开关组件（滑块 + 手绘样式），用于状态启停。

### Doodle 组件库（doodle/）

自研手绘风格组件，全部从 [index.ts](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/doodle/index.ts) 统一导出：

| 组件 | 说明 |
|------|------|
| `DoodleButton` | 按钮，类型：primary / teal / danger / ghost / default |
| `DoodleInput` | 输入框 |
| `DoodleSelect` | 下拉选择（支持搜索） |
| `DoodleCheckbox` | 复选框（支持半选态） |
| `DoodleCheckboxGroup` | 复选框组（多选，逗号拼接值） |
| `DoodleTag` | 标签（available / unavailable / deprecated / success / error 等 12 色） |
| `DoodleModal` | 模态框（createPortal + 焦点管理），提供 `confirm` / `info` / `error` 静态方法 |
| `DoodleMessage` | 消息提示（success / error / warning） |
| `DoodleEmpty` | 空状态占位 |

---

## 样式系统

### 变量定义（variables.less）

[variables.less](file:///d:/workspace/trae/free_models_token/free_models_manager/src/styles/variables.less) 是一套"手绘涂鸦 Design Token System"（暖纸 / 墨线 / 蜡笔色）：

```less
@bg-layout: #FBF5E7;        // 暖纸背景
@primary: #EB8A2F;          // 蜡笔橙主色
@on-surface: #37332B;       // 墨色文字
@radius-md: 14px 6px 16px 6px/6px 16px 6px 14px;  // 涂鸦异形圆角
@shadow-hard: 0 6px 0 -2px rgba(82, 63, 42, 0.16); // 手绘硬投影
@font-hand: 'Ma Shan Zheng', 'ZCOOL KuaiLe', 'Comic Neue', cursive; // 手写字体
```

手写字体由 `assets/fonts/` 内嵌加载（Ma Shan Zheng、ZCOOL KuaiLe、Gochi Hand、Comic Neue）。

### 全局样式（global.less）

- 页面布局（flex 布局）
- 卡片、表格统一样式
- 滚动条美化、字体设置

### 页面样式

每个页面有独立的 Less 文件（如 `Providers.less`、`Models.less`），配合页面级 class 使用，避免样式冲突。

---

## API 调用 & 签名

前端通过 Tauri Rust 后端 (`api.rs`) 的 `AdminClient` 进行所有 Admin API 调用，每次调用自动执行 Ed25519 签名流程。

### AdminClient 端点（api.rs）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/admin/service/status` | 服务状态与统计 |
| POST | `/admin/cache/refresh` | 刷新缓存 |
| GET/POST | `/admin/providers` | 供应商列表 / 创建 |
| GET/PUT/DELETE | `/admin/providers/{id}` | 供应商查询 / 更新 / 删除 |
| GET | `/admin/providers/{id}/models` | 拉取供应商模型列表 |
| POST | `/admin/providers/{id}/models/import` | 批量导入模型映射 |
| GET/POST | `/admin/models` | 模型列表 / 创建 |
| GET/PUT/DELETE | `/admin/models/{id}` | 模型查询 / 更新 / 删除 |
| GET/POST | `/admin/api_keys` | API Key 列表 / 创建 |
| GET/PUT/DELETE | `/admin/api_keys/{id}` | API Key 查询 / 更新 / 删除 |
| GET | `/admin/provider_credentials?provider_id={pid}` | 凭证列表（可按供应商过滤） |
| POST | `/admin/provider_credentials` | 创建凭证 |
| GET/PUT/DELETE | `/admin/provider_credentials/{id}` | 凭证查询 / 更新 / 删除 |
| POST | `/admin/provider_credentials/{id}/reset_status` | 重置凭证额度耗尽状态 |
| POST | `/admin/test_credential` | 凭证连通性测试 |
| GET | `/admin/provider_model_maps?model_id=&provider_id=` | 映射列表（可按模型/供应商过滤） |
| POST | `/admin/provider_model_maps` | 创建映射 |
| GET/PUT/DELETE | `/admin/provider_model_maps/{id}` | 映射查询 / 更新 / 删除 |
| GET | `/admin/usage_log/stats?group_by=&start_time=&end_time=` | 用量统计（group_by 白名单见 Stats 章节） |

### 签名流程

`AdminClient::signed_request`（[api.rs](file:///d:/workspace/trae/free_models_token/free_models_manager/src-tauri/src/api.rs)）构造请求：

1. 读取内存中的 Ed25519 密钥对（未加载时返回错误提示）
2. 取当前 Unix 时间戳
3. 生成 16 个 hex 字符的随机 `nonce`（8 字节）
4. 若有请求体，计算其 SHA-256 摘要（hex 编码）作为 `body_hash`
5. 构造签名负载 `METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH`（`PATH` 为纯路径，不含 query string）
6. 使用 `crypto.rs` 中的 `ed25519-dalek` 对负载签名，结果 Base64 标准编码
7. 添加请求头：`X-Admin-Fingerprint`、`X-Admin-Timestamp`、`X-Admin-Nonce`、`X-Admin-Signature`，有请求体时另加 `X-Admin-Body-Hash` 与 `Content-Type: application/json`

服务端校验（[admin_auth.rs](file:///d:/workspace/trae/free_models_token/free_models_server/src/middleware/admin_auth.rs)）：

- 时间戳与服务器时间差超过 300 秒 → 拒绝
- 同一 `nonce` 在 300 秒窗口内重复使用 → 拒绝（防重放）
- 用 `X-Admin-Fingerprint` 在 `admin_key` 表查找对应公钥（OpenSSH 格式），重建 `METHOD:PATH:TIMESTAMP:NONCE:BODY_HASH` 负载并验签

**指纹计算**：Ed25519 公钥（32 字节）→ SHA256 哈希 → Base64 URL 安全无填充编码 → 添加 `SHA256:` 前缀（与 [crypto.rs](file:///d:/workspace/trae/free_models_token/free_models_manager/src-tauri/src/crypto.rs) 中 `compute_fingerprint` 一致）。

此机制确保 Admin API 调用的安全性和不可抵赖性（详见 [authentication.md](./authentication.md)）。
