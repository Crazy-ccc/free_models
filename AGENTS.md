# AGENTS.md

两个独立包在同一个仓库中——它们之间没有 monorepo 工具链。

## free_models_server (Rust)

- **技术栈**: Rust edition 2024, actix-web 4, SeaORM 2 (MySQL), Redis, reqwest 0.12, tiktoken-rs, ed25519-dalek, AES-256-GCM
- **架构**: 单 crate 内模块化数据访问层 —— `src/db/` 包含 SeaORM 实体（`entities/`）、实现（`impls/`）、类型（`types.rs`）以及 `Database` 结构体/`StoreError`/`init_db`/`build_database`；`src/cache.rs` 包含 `RedisManager`。业务代码直接调用具体 store 结构体。
- **入口**: `src/main.rs` —— 路由包含 `/health`、`/v1/models`、`/v1/chat/completions`、`/v1/messages`、`/v1/responses`（公开，Bearer token）以及 `/admin/*`（Ed25519 签名）
- **迁移**: `migrations/` 中的手动 SQL 文件，按顺序执行（`001`–`005`）。**非** SeaORM 迁移。
- **配置**: 通过 `dotenv` 加载 `.env`。必需项：`DATABASE_URL`、`ENCRYPTION_KEY`（64 位 hex = 32 字节，用 `openssl rand -hex 32` 生成）。可选项：`SERVER_HOST`、`SERVER_PORT`、`REDIS_*`、`RUST_LOG`、`DB_MAX_CONNECTIONS`、`CIRCUIT_BREAKER_*`。
- **Redis**: 可选（`REDIS_ENABLED=false` 禁用它）。透明降级到内存缓存。
- **缓存**: 多级 —— Redis（可配置 TTL：模型 30s、供应商 600s、惩罚 1800s）+ 内存 fallback。Redis key 统一使用 `app:free_models:` 命名空间前缀。
- **Admin 鉴权**: Ed25519 签名。请求头：`X-Admin-Fingerprint`、`X-Admin-Timestamp`、`X-Admin-Signature`（签名内容 `METHOD:PATH:TIMESTAMP`，时间戳允许 ±300 秒防重放）。公钥存储在 `admin_key` 表中，fingerprint 构建方式：Ed25519 raw 公钥(32 字节)→SHA256 哈希→Base64 无填充编码→添加 `SHA256:` 前缀。
- **SSRF 防护**: provider URL 经 `proxy_ssrf::validate_url_safe` 校验，拦截私网 IP，DNS 解析失败时 fail-closed。校验失败对代理转发返回 `Fail`（不重试），对 admin 查询返回通用错误信息（不泄露内网 IP）。
- **HTTP client**: `app::build_client` 构建的 reqwest Client 统一设置 `User-Agent: FreeModelsServer/1.0`，对所有出站请求（模型转发、凭证测试、模型列表拉取）生效。
- **模型可用性判断**: 三重校验 —— `model_config.is_active = true`、存在活跃的 `provider_model_map`、对应 `provider_credential.is_active = true`，三者均满足才视为可用。
- **命令**:
  ```bash
  cargo run                         # 开发
  cargo run --release               # 生产
  cargo build --release --target x86_64-unknown-linux-musl  # 交叉编译静态二进制
  docker build -t free_models_server . && docker run --rm -p 8080:8080 --env-file .env free_models_server
  ```
- **快速验证**: `curl http://localhost:8080/health` → `{"status":"ok"}`
- **后台任务**: 优雅关闭（SIGTERM/Ctrl-C），每日 00:05 UTC 自动归档 `usage_log`（指数退避重试）。

## free_models_manager (Tauri + React)

- **技术栈**: Tauri v2, React 18, TypeScript, Ant Design 6, LESS, Vite 5
- **入口**: `src/main.tsx`，路由在 `src/App.tsx`
- **命令**:
  ```bash
  pnpm install          # 安装依赖（pnpm 11.16.0）
  pnpm dev              # vite dev server，端口 1420
  pnpm build            # tsc && vite build
  pnpm tauri dev        # 开发模式：桌面窗口 + HMR
  pnpm tauri build      # 生产构建
  ```
- **TypeScript 检查**: `npx tsc --noEmit`
- **CSS**: LESS（vite 配置中已开启 `javascriptEnabled: true`）
- **页面**: 概览、供应商（含凭证管理 + 测试 + 一键导入模型，已导入模型基于 `provider_model_map` 表标注禁用）、模型、API Keys、统计、设置
- **注意事项**:
  - `pnpm build` 会执行 `tsc && vite build` —— TypeScript 错误会导致构建失败。
  - Dev server 忽略 `src-tauri/` 变更（在 `vite.config.ts` 中配置）。
  - `pnpm-workspace.yaml` 仅有 `allowBuilds: { esbuild: true }` —— 并非 monorepo workspace。

## 通用

- 仓库中未发现测试。
- `.gitignore` 排除了 `.trae/`、`.idea/`、`target/`、`node_modules/`、`dist/`、`.env`。
- `docx/` 中的文档涵盖：架构、API 端点、数据库表结构、鉴权、缓存、代理转发/Token。
- `free_models_server/DEPLOYMENT.md` 详细说明了交叉编译和远程 Docker 工作流。
