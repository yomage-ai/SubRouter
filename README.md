# SubRouter

SubRouter 是一个面向 Codex CLI 的自托管透明代理，用来统一管理多个 OpenAI OAuth 订阅账号，并提供会话粘性切换、配额观测与基础管理后台。

当前仓库已完成 Phase 5 的首版落地：

- Rust workspace，包含 `router-api`、domain、storage、upstream、usage 模块
- PostgreSQL 初始迁移与账号加密存储
- 管理 API 骨架与单管理员登录会话
- Vue 3 管理台骨架，包含账号列表、详情和仪表盘页面
- `POST /v1/responses`、`POST /v1/responses/compact` 和 `GET /v1/responses` WebSocket 的透明代理，包含会话亲和选路、401 后 OAuth refresh 重试、WS 会话粘性与 usage 落库
- `x-codex-*` 配额头归一化、手动 quota probe、cooldown 展示
- 源码部署文档、运维手册和 smoke 脚本

## 目录

- `apps/router-api`：Axum 管理 API 与后续透明代理入口
- `apps/admin-web`：Vue 3 管理页
- `crates/subrouter-domain`：领域模型、会话键与账号排序逻辑
- `crates/subrouter-storage`：PostgreSQL 仓储与应用层加密
- `crates/subrouter-upstream`：上游 OAuth 与 Responses 协议占位模块
- `crates/subrouter-usage`：用量汇总接口与扩展点
- `docs/architecture.md`：当前架构定稿
- `docs/deployment.md`：自托管部署说明
- `docs/operations.md`：运维手册
- `scripts/smoke.ps1`：管理链路与透明代理烟测脚本

## 快速开始

1. 复制 `.env.example` 为 `.env`
2. 至少填写 `SUBROUTER_ADMIN_TOKEN`、`SUBROUTER_MASTER_KEY`
3. OpenAI ChatGPT OAuth 默认已经内置 `auth.openai.com/oauth/token` 和 Codex CLI `client_id`，通常只需要保留 `.env.example` 里的默认值；`SUBROUTER_OAUTH_CLIENT_SECRET` 对这条链路可留空
   默认的 `SUBROUTER_QUOTA_PROBE_MODEL` 已调整为 `gpt-5-mini`，用于更快地刷新 5h/7d 配额快照
4. 启动后端

```powershell
cargo run -p router-api
```

5. 启动前端：

```powershell
cd apps/admin-web
npm install
npm run dev
```

源码模式下：

- 前端地址是 `http://127.0.0.1:5173`
- 后端地址是 `http://127.0.0.1:8080`
- Vite 已代理 `/api`、`/healthz` 和 `/v1`

启动后先跑一次烟测：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:8080 -AdminToken <your-admin-token>
```

如果已经录入账号并想顺便验证透明代理：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:8080 -AdminToken <your-admin-token> -ExerciseProxy
```

更多细节见：

- [deployment.md](F:/service/SubRouter/docs/deployment.md)
- [operations.md](F:/service/SubRouter/docs/operations.md)
