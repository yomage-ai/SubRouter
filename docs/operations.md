# SubRouter 运维手册

## 日常检查

推荐按这个顺序检查：

1. 确认 PostgreSQL 正常可连
2. 确认 `router-api` 进程仍在运行
3. 运行烟测脚本
4. 登录管理台，看仪表盘是否还能拉到最新 usage 与 quota

后端烟测：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:8080 -AdminToken <token>
```

如果想把前端代理链路也一起验证：

```powershell
powershell -NoProfile -File .\scripts\smoke.ps1 -BaseUrl http://127.0.0.1:5173 -AdminToken <token>
```

## 常用命令

启动后端：

```powershell
cargo run -p router-api
```

启动前端：

```powershell
cd apps/admin-web
npm run dev
```

检查后端编译状态：

```powershell
cargo check --workspace
```

检查前端构建：

```powershell
cd apps/admin-web
npm run build
```

## 典型故障与处理

### 管理台能打开，但接口都 401

优先检查：

- `.env` 里的 `SUBROUTER_ADMIN_TOKEN` 是否和登录输入一致
- `SUBROUTER_WEB_ORIGIN` 是否和实际访问地址一致
- 如果是 HTTPS，`SUBROUTER_COOKIE_SECURE` 是否已经设为 `true`

### 新会话全部返回 `no eligible upstream account is currently available`

通常是这几类问题：

- 没有录入账号
- 账号被手动停用
- refresh 失败后状态变成 `refresh_failed`
- 5h 或 7d 配额打满，账号处于 `cooldown_until`
- 某些账号虽然活跃会话很多，但当前版本不会再因为会话数直接拒绝新会话

处理顺序建议：

1. 进入账号列表，先看状态列和 cooldown
2. 进入账号详情，看最近错误
3. 必要时执行“手动刷新 Token”或“手动 Probe 配额”
4. 确认后再决定是否“清除 cooldown”

### 管理台显示 quota 未知

这不一定是故障，可能只是最近没有从上游拿到 `x-codex-*` 头。

建议：

1. 先跑一次真实请求
2. 再执行“手动 Probe 配额”
3. 如果仍未知，说明当前上游没有返回可识别的 quota 头，需要抓包确认头格式

### WebSocket 正常，HTTP 正常，但管理台不刷新

优先检查：

- 浏览器里请求是否打到了 `/api/*`
- Vite dev server 是否仍在运行
- `router-api` 的 `SUBROUTER_WEB_ORIGIN` 是否匹配当前访问地址

## 备份

备份 PostgreSQL：

```powershell
pg_dump -U postgres -d subrouter > subrouter-backup.sql
```

恢复 PostgreSQL：

```powershell
psql -U postgres -d subrouter -f .\subrouter-backup.sql
```

如果你修改了数据库名或用户名，把命令里的值同步替换掉。

## 升级

标准升级步骤：

1. 备份数据库
2. 拉取最新代码
3. 重新执行依赖安装与构建验证
4. 重启后端和前端进程

建议顺序：

```powershell
cargo check --workspace
cd apps/admin-web
npm install
npm run build
```

`router-api` 启动时会自动执行 SQL migration，所以正常情况下不需要再单独跑迁移命令。

## 回滚

如果升级后出现明显回归：

1. 停掉当前进程
2. 切回上一个稳定 commit
3. 重新启动后端和前端
4. 如果数据库结构也已经变化，按备份恢复 PostgreSQL

## 观测重点

建议重点盯这几项：

- `refresh_failed_accounts`
- `accounts_in_cooldown`
- 单账号最近错误
- 5h 和 7d 峰值
- 活跃 WS 会话数
