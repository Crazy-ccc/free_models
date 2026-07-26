# 管理员界面文档

管理员界面基于 Tauri v2 + React + TypeScript + Ant Design 5 构建，提供供应商、模型、API Key 等资源的可视化管理功能。

---

## 架构概览

```
free_models_manager/
├── src/
│   ├── App.tsx                  # 应用根组件，路由与全局布局
│   ├── main.tsx                 # 入口
│   ├── types/index.ts           # TypeScript 类型定义
│   ├── components/              # 通用 UI 组件
│   │   ├── Sidebar.tsx/less     # 侧边栏导航
│   │   ├── Drawer.tsx/less      # 抽屉面板（设置/编辑）
│   │   ├── Toolbar.tsx/less     # 顶部工具栏
│   │   └── Toggle.tsx/less      # 开关组件
│   ├── pages/                   # 页面
│   │   ├── Overview.tsx/less    # 概览仪表盘
│   │   ├── Stats.tsx/less       # 使用统计
│   │   ├── Providers.tsx/less   # 供应商管理
│   │   ├── Models.tsx/less      # 模型管理
│   │   ├── ApiKeys.tsx/less     # API Key 管理
│   │   └── Settings.tsx/less    # 设置页
│   └── styles/
│       ├── global.less          # 全局样式
│       └── variables.less       # 样式变量
└── src-tauri/
    └── src/
        ├── main.rs              # Tauri 应用入口
        ├── api.rs               # 网络请求 + 签名工具
        └── crypto.rs            # Ed25519 签名生成
```

### Tauri 后端（src-tauri）

Tauri 后端提供三项核心能力：

1. **网络请求封装（api.rs）：** 封装 fetch 调用，自动添加 Admin 鉴权签名头
2. **签名生成（crypto.rs）：** 使用 `ed25519-dalek` 库生成 Ed25519 签名
3. **进程管理（main.rs）：** Tauri 应用生命周期管理

### 前端路由

| 路径 | 页面 | 功能 |
|------|------|------|
| `/` | Overview | 概览仪表盘，显示统计信息 |
| `/stats` | Stats | 使用统计，多维度分组查询 |
| `/providers` | Providers | 供应商 CRUD + 一键导入模型 + 凭证管理 |
| `/models` | Models | 模型列表（只读状态 + 筛选） |
| `/api-keys` | ApiKeys | API Key CRUD（自动生成密钥） |
| `/settings` | Settings | SSH 公钥管理 |

---

## 页面详解

### 1. Overview（概览页）

文件：[Overview.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Overview.tsx)

调用 `GET /admin/service/status` 获取统计数据，并以卡片形式展示。

**展示指标：**

| 指标 | 颜色 | 统计方式 |
|------|------|---------|
| 模型总数 | 蓝色 | 全部模型 |
| 活跃模型数 | 绿色 | `status = 'available'` |
| 不活跃模型数 | 红色 | 总数 - 活跃数 |
| 供应商总数 | 蓝色 | 全部供应商 |
| 活跃供应商数 | 绿色 | 有活跃凭证的唯一供应商 |
| 不活跃供应商数 | 红色 | 总数 - 活跃数 |
| API Key 总数 | 蓝色 | 全部 API Key |
| 活跃 API Key 数 | 绿色 | `is_active = true` |
| 不活跃 API Key 数 | 红色 | 总数 - 活跃数 |

**缓存刷新：** 页面上提供"刷新缓存"按钮，调用 `POST /admin/cache/refresh`。

### 2. Providers（供应商页）

文件：[Providers.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Providers.tsx)

**功能列表：**

| 功能 | 实现 |
|------|------|
| 列出供应商 | 调用 `GET /admin/providers` |
| 创建供应商 | 弹出表单，调用 `POST /admin/providers` |
| 编辑供应商 | 抽屉编辑，调用 `PUT /admin/providers/{id}` |
| 删除供应商 | 二次确认弹框，调用 `DELETE /admin/providers/{id}` |
| 一键导入模型 | 调用 `{baseUrl}/models` 获取模型列表，与已有模型去重，弹框选择导入 |
| 凭证管理 | 每个供应商可管理多组凭证（CRUD） |
| 凭证测试 | 选择模型发送测试请求 |

**一键导入模型流程：**
1. 点击"一键添加模型"按钮
2. 前端调用 `{baseUrl}/models` 获取供应商的模型列表
3. 获取失败 → 提示"该供应商不支持一键导入模型"
4. 获取成功 → 与已导入模型去重 → 弹框展示可导入模型列表（支持搜索）
5. 用户勾选需要导入的模型 → 批量创建

**凭证测试流程：**
1. 选择凭证和模型
2. 点击"测试"按钮（防重复点击控制）
3. 调用 `POST /admin/test_credential`
4. 显示测试结果：成功（含响应时间）或失败（含错误信息）

### 3. Models（模型页）

文件：[Models.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Models.tsx)

**功能列表：**

| 功能 | 实现 |
|------|------|
| 模型列表 | Ant Design Table，含 sticky 表头 |
| 创建模型 | 弹出表单，调用 `POST /admin/models` |
| 编辑模型 | 弹出表单，调用 `PUT /admin/models/{id}` |
| 删除模型 | 二次确认弹框，调用 `DELETE /admin/models/{id}` |
| 状态字段 | 只读显示（标签），仅在编辑页修改 |
| 状态筛选 | 表格内置筛选器，支持 `available` / `disabled` |
| 供应商筛选 | 表格内置筛选器，按供应商名称过滤 |

**模型列表字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| ID | int | 主键 |
| 名称 | string | 模型对外名称 |
| 供应商 | string | 所属供应商名称 |
| 优先级 | int | 数值越小越优先 |
| 协议 | tag | `OpenAI` / `Anthropic` / 两者 |
| 上下文窗口 | int | context_length 值 |
| 状态 | tag | `available`（绿色）/ `disabled`（红色） |
| 操作 | buttons | 编辑 / 删除 |

### 4. ApiKeys（API Key 页）

文件：[ApiKeys.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/ApiKeys.tsx)

**功能列表：**

| 功能 | 实现 |
|------|------|
| 列表 | Ant Design Table |
| 创建 | 弹出表单，`key_value` 由服务端自动生成（`fm-` + 64 位随机字符） |
| 编辑 | 弹出表单，可修改名称和状态 |
| 删除 | 二次确认弹框 |
| 创建结果 | 无弹框提示，静默完成 |

### 5. Settings（设置页）

文件：[Settings.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/pages/Settings.tsx)

**功能列表：**

| 功能 | 实现 |
|------|------|
| 保存私钥 | 将 Ed25519 私钥保存到本地（Tauri 文件系统） |
| 显示指纹 | 显示对应公钥的 SHA256 指纹 |
| 服务器地址配置 | 配置 Admin API 地址 |

---

## 通用组件

### Sidebar（侧边栏）

文件：[Sidebar.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Sidebar.tsx)

- 垂直导航布局
- 导航项：概览、供应商、模型、API Keys、统计、设置
- 当前选中项高亮

### Drawer（抽屉）

文件：[Drawer.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Drawer.tsx)

基于 Ant Design Drawer 的封装，用于编辑表单。

### Toolbar（工具栏）

文件：[Toolbar.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Toolbar.tsx)

顶部工具栏，包含页面标题和操作按钮。

### Toggle（开关）

文件：[Toggle.tsx](file:///d:/workspace/trae/free_models_token/free_models_manager/src/components/Toggle.tsx)

Ant Design Switch 的封装，用于状态切换。

---

## 样式系统

### 变量定义（variables.less）

```less
@primary-color: #1677ff;      // 主色
@success-color: #52c41a;      // 成功色
@warning-color: #faad14;      // 警告色
@error-color: #ff4d4f;        // 错误色
@font-size-base: 14px;        // 基础字号
@border-radius-base: 6px;     // 圆角
@box-shadow-base: 0 1px 2px rgba(0,0,0,0.06);  // 阴影
```

### 全局样式（global.less）

- 页面布局（flex 布局）
- 卡片样式
- 表格统一样式
- 滚动条美化
- 字体设置

### 页面样式

每个页面有独立的 Less 文件（如 `Providers.less`、`Models.less`），使用 CSS Modules 避免样式冲突。

---

## API 调用 & 签名

前端通过 Tauri Rust 后端 (`api.rs`) 进行所有 Admin API 调用。每次调用自动执行 Ed25519 签名流程：

1. 加载本地存储的 Ed25519 私钥
2. 构造 `METHOD:PATH:TIMESTAMP` 签名负载
3. 使用 `crypto.rs` 中的 `ed25519-dalek` 生成签名
4. 在请求头中添加 `X-Admin-Fingerprint`、`X-Admin-Timestamp`、`X-Admin-Signature`
5. 发送 HTTP 请求

此机制确保 Admin API 调用的安全性和不可抵赖性（详见 [authentication.md](./authentication.md)）。
