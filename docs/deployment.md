# SubRouter 自托管部署说明

## 目标

本文档对应 `2026-03-23` 的 Phase 5 交付，目标是在没有 Docker 环境的前提下，用源码方式把 SubRouter 跑起来，并同时提供：

- 管理后台
- 管理 API
- `/v1/responses` 与 `/v1/responses/compact` 透明代理

## 依赖

- Rust 工具链
- Node.js 20+
- PostgreSQL
- 至少一个有效的 OpenAI OAuth 订阅账号

## 环境变量

从仓库根目录复制 `.env.example` 为 `.env`，至少修改这些值：

- `DATABASE_URL`
- `SUBROUTER_ADMIN_TOKEN`
- `SUBROUTER_MASTER_KEY`

如果要启用 OpenAI ChatGPT OAuth refresh，默认值通常已经够用：

- `SUBROUTER_OAUTH_TOKEN_URL`
- `SUBROUTER_OAUTH_CLIENT_ID`
- `SUBROUTER_OAUTH_CLIENT_SECRET`

说明：

- `SUBROUTER_OAUTH_TOKEN_URL` 默认可用值是 `https://auth.openai.com/oauth/token`
- `SUBROUTER_OAUTH_CLIENT_ID` 默认可用值是 `app_EMoamEEZ73f0CkXaXp7hrann`
- `SUBROUTER_OAUTH_CLIENT_SECRET` 对当前 ChatGPT Codex 链路通常留空

本地源码模式建议保持：

- `SUBROUTER_WEB_ORIGIN=http://127.0.0.1:5173`
- `SUBROUTER_COOKIE_SECURE=false`
- `SUBROUTER_FORCE_PRIORITY_SERVICE_TIER=false`

如果将来挂到 HTTPS 域名，再把：

- `SUBROUTER_WEB_ORIGIN` 改成真实访问地址
- `SUBROUTER_COOKIE_SECURE` 改成 `true`

如果希望代理统一给 OpenAI Responses 请求加上 `service_tier: "priority"`，再把：

- `SUBROUTER_FORCE_PRIORITY_SERVICE_TIER` 改成 `true`

## 启动 PostgreSQL

确保 PostgreSQL 已经启动，并且 `DATABASE_URL` 指向可用实例。默认示例值是：

```text
postgres://postgres:postgres@localhost:5432/subrouter
```

## 启动后端

在仓库根目录执行：

```powershell
rtk cargo run -p router-api
```

后端启动后会：

- 绑定到 `http://127.0.0.1:8080`
- 自动执行 `apps/router-api/migrations` 下的 SQL migration

## 启动前端

在另一个终端执行：

```powershell
cd apps/admin-web
rtk npm install
rtk npm run dev
```

前端入口是：

- `http://127.0.0.1:5173`

说明：

- Vite 已代理 `/api`、`/healthz` 和 `/v1`
- 本地开发时，Codex CLI 可以直接指向 `http://127.0.0.1:5173`
- 如果你只想排查后端，也可以直接访问 `http://127.0.0.1:8080`

## 首次登录与录入账号

1. 打开 `http://127.0.0.1:5173`
2. 使用 `SUBROUTER_ADMIN_TOKEN` 登录
3. 在“订阅账号”页录入至少一个账号
4. 回到账号详情页，先执行一次“手动刷新 Token”或“手动 Probe 配额”

## Codex CLI 接入

本地开发模式下有两个常用入口：

- 推荐入口: `http://127.0.0.1:5173`
- 后端直连: `http://127.0.0.1:8080`

如果你希望管理台和透明代理走同一个入口，优先使用 `http://127.0.0.1:5173`。因为前端开发服务器已经代理了 `/v1/*`，并支持 WebSocket upgrade。

## 烟测

只检查管理链路：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:8080 -AdminToken <your-admin-token>
```

连透明代理一起检查：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:8080 -AdminToken <your-admin-token> -ExerciseProxy
```

说明：

- `-ExerciseProxy` 需要库里已经有至少一个可用账号
- 如果你想把“没有账号”视为失败，可额外加 `-RequireProxyReady`

如果你要通过 `5173` 验证前端代理链路，也可以把 `BaseUrl` 改成 `http://127.0.0.1:5173`
