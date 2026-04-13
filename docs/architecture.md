# SubRouter 架构设计文档 v0.1

## Summary
- 将以下内容作为 `docs/architecture.md` 的首版定稿。
- 项目目标：做一个面向 Codex 的透明代理，统一管理多个 OpenAI OAuth 订阅账号，提供 token 统计、5h/7d 配额展示，以及会话级无感切换。
- 首版边界已锁定：`OpenAI OAuth only`、`Codex CLI 核心协议 only`、`API + 简单 Web`、`单机自托管`、`会话粘性切换`、`Rust + Vue`。
- 明确不做：Anthropic/Gemini、多租户权限体系、Chat Completions 全兼容、多实例协调、sub2api 级别的复杂计费与路由规则。

## Core Architecture
- 后端采用 `Rust + Axum + Tokio + Reqwest + tokio-tungstenite + sqlx`，前端采用 `Vue 3 + Vite + TypeScript + Pinia + Vue Router`。
- 仓库采用 workspace 结构：`apps/router-api` 放代理与管理 API，`apps/admin-web` 放管理页，`crates/*` 放领域、存储、上游协议与统计模块。
- 持久层默认只引入 `PostgreSQL`；不在 v1 引入 Redis。热状态用内存实现，并通过 trait 抽象出 `SessionStateStore`、`QuotaSnapshotStore`，为后续 Redis 适配留口。
- 核心模块拆分为 5 个子系统：`UpstreamAuth` 负责 OpenAI OAuth 凭据与刷新，`ProxyIngress` 负责 HTTP/WS 透明转发，`AccountPool` 负责账号健康与选路，`UsageMeter` 负责 token/请求/延迟统计，`AdminAPI` 提供账号和仪表盘接口。
- 管理端安全采用单管理员模式：后端用 `SUBROUTER_ADMIN_TOKEN` 保护管理 API，Web 端登录后换成本地 session cookie；不引入用户体系。

## Proxy And Switching Design
- 客户端入口固定支持 3 个面：`POST /v1/responses`、`GET /v1/responses` 的 WebSocket Upgrade、`POST /v1/responses/compact`。
- 透明代理必须透传或重建这些关键头与字段：`originator`、`user-agent`、`openai-beta`、`session_id`、`conversation_id`、`x-codex-turn-state`、`x-codex-turn-metadata`、`previous_response_id`。
- 账号选路规则固定为：先命中会话亲和，其次过滤掉 `disabled / refresh_failed / cooldown / quota exhausted` 的账号；若 `7d` 重置剩余时间不超过 `3 小时`，则该账号先进入“优先用完”队列；随后再按综合分数选最优账号。综合分数会同时考虑 `7d 剩余`、`7d 重置节奏` 和 `5h 剩余`，再以当前活跃会话数升序和 `last_selected_at` 升序兜底。
- 会话键固定为 `session_id > conversation_id > previous_response_id > request_id` 的优先级组合；一旦 WebSocket 会话建立或某条响应链已经绑定账号，后续 turn 默认不迁移。
- “无感切换”在 v1 的定义是安全边界切换，不做中途迁移：新会话可换账号；HTTP 请求或 WS turn 在首个 token 前若遇到可恢复错误，可换账号重试一次；若命中 `previous_response_not_found`，允许丢弃 `previous_response_id` 后按新 turn 重放一次。
- 429 或配额信号统一从 `x-codex-*` 响应头归一化为 `5h` 和 `7d` 快照；若某窗口达到 100%，将账号标记 `cooldown_until = reset_at`，并在管理页显示原因和剩余时间。

## Data Model And Public Interfaces
- 持久化实体固定为 5 张主表：`accounts`、`account_secrets`、`quota_snapshots`、`usage_events`、`session_affinity`。
- `accounts` 保存展示与调度字段：名称、状态、权重、cooldown、最后选择时间、最近错误、最近成功时间，以及兼容保留的旧会话上限字段。
- `account_secrets` 单独保存敏感信息：`access_token`、`refresh_token`、过期时间、指纹/UA 元数据；全部用 `SUBROUTER_MASTER_KEY` 做应用层加密。
- `quota_snapshots` 只存观察到的上游配额：`account_id`、`window_type(5h|7d)`、`used_percent`、`reset_at`、`source(header|probe)`、`updated_at`。
- `usage_events` 记录每个 turn 或请求的统计：`account_id`、`transport(http|ws)`、`model`、`input_tokens`、`output_tokens`、`usage_source(exact|estimated)`、`latency_ms`、`response_id`、`session_key`、`created_at`。
- token 统计策略固定为“上游 usage 优先，缺失时本地估算兜底”：HTTP 非流式取最终 JSON 的 `usage`，流式/WS 取终止事件中的 `usage`，没有时仅对纯文本输入输出使用 `tiktoken-rs` 估算，并明确标记 `estimated`。
- 管理 API 首版固定提供：账号 CRUD、手动启停账号、手动刷新 token、手动 probe 配额、查询账号列表、查询单账号 usage、查询仪表盘汇总、清除 cooldown。
- 管理页首版固定提供 3 个页面：`订阅账号列表`、`账号详情`、`配额与 token 仪表盘`。账号详情页必须能看到 5h/7d 配额、最近错误、当前会话数、最近 token 用量。

## Delivery Plan
- Phase 1：搭好 Rust workspace、数据库迁移、账号模型、加密存储、管理 API 骨架、Vue 管理台骨架。
- Phase 2：完成 `POST /v1/responses` 透明代理、OAuth token 刷新、账号池选路、HTTP usage 记录。
- Phase 3：完成 `GET /v1/responses` WebSocket 透明转发、会话粘性，以及 `previous_response_id` 恢复策略。
- Phase 4：完成 `x-codex-*` 配额解析、手动 probe、仪表盘、账号详情页与冷却状态展示。
- Phase 5：补齐 CLI 烟测脚本、Docker Compose、自托管部署文档与运维手册。

## Test Plan
- 单元测试覆盖：账号选路排序、配额头归一化、cooldown 计算、token 估算回退、会话键生成。
- 集成测试覆盖：HTTP `/v1/responses` 成功/流式/429、WebSocket 首条 `response.create`、多 turn `previous_response_id` 链接、可恢复错误重试一次。
- 端到端测试覆盖：两到三个账号场景下的新会话分流、某账号 5h 打满后的自动避让、某账号 refresh 失败后的摘除、管理页配额与 token 数据一致性。
- 验收标准：Codex CLI 改 base URL 后可正常使用；同一会话保持账号粘性；账号异常时新会话能切走；管理页能看到账号状态、5h/7d 配额、token 趋势。

## Assumptions
- OpenAI 上游仍会返回可用于配额展示的 `x-codex-*` 头；若没有，则管理页展示“未知”并允许手动 probe。
- v1 不做多实例，因此内存态会话与 DB 持久化并存即可；后续若扩多实例，只替换 store 实现，不改领域接口。
- v1 不承诺 mid-stream 跨账号迁移；“无感切换”只保证会话边界与安全重试边界内的切换。
- v1 只做 Codex 核心代理，不追求与所有 OpenAI-compatible 客户端完全兼容。
