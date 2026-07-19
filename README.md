# Skill Shelf

管理 [Agent Skills](https://agentskills.io) 与 prompt 的服务:类 Git 的版本管理、按自然语言需求路由最合适的 skill、AI 反馈优化闭环,并可作为 **MCP server** 供各类 agent 直接取用。skill 遵循开放的 Agent Skills 规范,**不绑定任何特定 agent 或厂商**——任何支持该规范的 agent 都能消费。附带一个 React + Tauri 前端(桌面 / Web 同一套)。

> 额外能力:同一个服务还兼作**配置中心**——其他服务启动时来这里取配置,而不各自读环境变量。

## 特性

- **版本管理**:内容寻址存储(CAS,SHA-256)+ 提交 / 分支 / diff / 回滚,像 Git 一样管理 skill 内容。
- **路由**:`exact`(按名精确)/ `fuzzy`(BM25 召回)/ `smart`(BM25 → LLM 重排;未配 AI 自动降级为 BM25)。
- **规范校验**:提交 skill 时按 Agent Skills 规范硬校验 `SKILL.md` frontmatter;prompt 类型豁免。
- **反馈优化闭环**:-1/0/+1 评分 + AI 依反馈生成草稿(`refine` 分支,不动 main)→ 合并。
- **MCP 取用**:`skill-shelf-mcp` 以 stdio 暴露 route / 浏览 / 加载 / 读文件 / 反馈等工具,代理到 REST。
- **运行时配置热更新**:AI / GitHub 等设置 admin 可在线改、即时生效、无需重启。
- **配置中心**:`_global` 共享层 + 各服务 namespace 两级合并;service token(仅存哈希、按 namespace 授权)拉取合并后的**明文**配置。
- **可选后端**:SQLite(本地 / 小项目)或 Postgres;**可选认证**:设 `JWT_SECRET` 即开启 JWT + Argon2,首个用户为 admin。

## 架构

```
crates/
  core     skill-shelf-core   CAS 存储 + 版本模型 + Index 抽象(SQLite/Postgres)+ 校验
  server   skill-shelf-server Axum REST API、认证、配置(自身设置 + 配置中心)
  mcp      skill-shelf-mcp    MCP server(stdio),代理到 REST
app/       React 19 + Vite + TanStack Router/Query + shadcn/ui + Tailwind;Tauri 桌面壳
```

设计与 API 细节见 [DESIGN.md](./DESIGN.md);阶段进展见 [PLAN.md](./PLAN.md)。

## 快速开始

### 后端

```bash
# SQLite(默认)。不设 JWT_SECRET = 开放模式(适合本地/桌面)
cargo run -p skill-shelf-server
# → http://127.0.0.1:8080   数据落在 ./data

# 开启认证 + 指定数据目录/端口
JWT_SECRET=change-me DATA_DIR=./data PORT=8080 cargo run -p skill-shelf-server

# 用 Postgres(CAS blob 仍在 DATA_DIR,索引走 PG)
DB=postgres://user:pass@localhost/skillshelf cargo run -p skill-shelf-server
```

### 前端

```bash
cd app
pnpm install
pnpm dev            # http://localhost:5173,默认连后端 127.0.0.1:8080
```

后端地址可在前端「Settings → Backend」运行时切换(同一份构建能指向任意服务器)。

### MCP(供 agent 取用)

```bash
SKILL_SHELF_URL=http://127.0.0.1:8080 cargo run -p skill-shelf-mcp
```

以 stdio 运行,暴露工具:`route`、`list_skills`、`load_skill`、`read_skill_file`、`feedback`。

## 配置

配置 = 环境变量,值支持字符串或 JSON。分两类:

| 类别 | 键 | 何时可改 |
|------|-----|---------|
| 结构性(启动期) | `PORT` · `DATA_DIR` · `DB` · `JWT_SECRET` | 改动需重启 |
| 运行期(热更新) | `AI_BASE_URL` · `AI_API_KEY` · `AI_MODEL` · `GITHUB_TOKEN` · `GITHUB_API_BASE` · 任意自定义 | admin 在「Settings → Service settings」在线改,即时生效 |

优先级:**已存值 > 环境变量 > 默认**;含 `KEY/TOKEN/SECRET/PASSWORD` 的键在读取接口打码。

### 配置中心

其他服务从这里取配置。admin 在「Config center」页建 namespace + KV、签发 service token(明文只展示一次);消费方带 `X-Config-Token` 头拉取合并明文:

```bash
curl -H "X-Config-Token: shelf_…" \
  "http://127.0.0.1:8080/config/resolve?namespace=service-a/prod"
# → merge(_global, service-a/prod) 已发布版本的明文 JSON
```

- **版本管理**:每个 namespace 有「草稿 / 已发布 / 历史」。编辑改草稿,消费方 `resolve` 始终读**最新已发布版本**;admin 复核逐字段 diff 后点 **Publish** 才生效。可查看历史版本、**回退**(载入草稿再发布)——"回退了再发布"。
- `_global` 会合并进每一次 resolve,**只放非敏感共享默认值**。
- Skill Shelf 自身设置与配置中心**完全隔离**,自身密钥不会经 resolve 外泄。
- 关闭认证(无 `JWT_SECRET`)时,整个配置中心 `/config/*` 拒绝服务。

## 部署

```bash
# Web:server(SQLite 卷)+ nginx 托管前端
docker-compose up -d
# 用 Postgres:
docker-compose --profile pg up -d
```

桌面:`cd app && pnpm tauri build`,把 `server` 作为 sidecar 打包(本地 SQLite,端口 8765)。

## 许可证

[MIT](./LICENSE)
