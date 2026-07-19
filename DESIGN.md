# Skill Shelf — 架构设计文档

> 一个「Skill 注册中心 + 版本管理 + 智能路由 + 反馈优化」服务。像 Git 一样管理 Claude Agent Skill 的版本，按自然语言需求返回最匹配的 skill，并通过反馈驱动 skill 持续优化。后端 Rust，前端 TS + Tauri，可同时发布到桌面和 Web。

---

## 1. 这个项目是什么

Skill Shelf 有三根支柱，合起来是一个**自我改进的技能中心**：

1. **版本管理** —— 同时管理两类单元(`kind`)：**skill**(多文件目录包 `SKILL.md` + `scripts/` + `references/`)和 **prompt**(单文件文本，本质是"单文件 skill")。两者共用同一套类 Git 版本控制：分支、提交、diff、回滚。skill 支持 **zip 导入/导出**(上传 zip 自动解压、解析 `SKILL.md` frontmatter 填 name/description；预览展示 description)。
2. **智能路由** —— 给定一段自然语言需求（"帮我把 PDF 转成文本"），返回最匹配的**单个 skill 或 skill 列表**及其版本。让 agent 在运行期按需取用技能。
3. **反馈驱动优化** —— 提供反馈入口，针对某个 skill（及其版本）收集使用反馈，再由 AI 基于反馈提出改进草稿，人工审核后发布新版本。

闭环：

```
      ┌──────────────────────────────────────────────┐
      ▼                                                │
  路由(需求→skill) ──▶ 使用 ──▶ 反馈(有用?哪里不对?)     │
      ▲                              │                 │
      │                              ▼                 │
  索引更新 ◀── 发布新版本 ◀── 人工审核 ◀── AI 优化(据反馈提改进草稿)
```

一句话：**Skill 的 Git + 按需分发的路由器 + 越用越好的反馈闭环。**

---

## 2. 核心概念与数据模型

### 2.1 版本模型（类 Git，内容寻址）

skill 是多文件目录包，采用**内容寻址存储（CAS）**，未改动的文件在多次提交间自动去重。

| 概念 | 说明 |
|------|------|
| **Skill** | 顶层版本化单元。`id`、`name`、`description`、**`kind`**(`skill`/`prompt`)、`created_at` + **缓存的 SKILL.md frontmatter**(`license`/`compatibility`/`metadata`/`allowed-tools`)。prompt = 单文件 skill |
| **Branch** | 命名开发线（`main`、`v2`），指向 head commit |
| **Commit** | 整个目录的不可变快照。`id` = 内容哈希；含 `tree`/`parent`/`author`/`message`/`timestamp` |
| **Tree** | 目录清单：`path → blob 哈希` 有序映射 |
| **Blob** | 按内容哈希存储的单个文件内容 |
| **Feedback** | 针对某 skill+版本的反馈（见 2.3） |

### 2.2 存储分层

- **对象库（CAS，文件系统）**：blob / tree / commit，键为 SHA-256。
- **索引库（关系型）**：skills、branches（可变 head 指针）、feedback、users、路由索引。

### 2.3 反馈模型（新增）

| 字段 | 说明 |
|------|------|
| `id` | 反馈 ID |
| `skill_id` / `commit_id` | 针对哪个 skill 的哪个版本 |
| `source` | 来源：`human`（人在面板里提交）/ `agent`（运行时回传） |
| `rating` | 评分（如 -1/0/+1 或 1~5） |
| `content` | 文字反馈：哪里不对、期望怎样 |
| `query` | 可选：当初路由到这个 skill 的需求原文（把路由效果也纳入反馈） |
| `status` | `open` / `applied`（已被某次优化采纳） / `dismissed` |
| `created_at` | 时间 |

> 反馈同时服务两件事：**优化 skill 内容**，以及**改进路由质量**（`query` + `rating` 记录了"什么需求路由到它、好不好用"）。

### 2.4 Agent Skill 规范一致性（skill kind）

skill 遵循 [Agent Skills 规范](https://agentskills.io/specification)：根级 `SKILL.md` + 可选 `scripts/`/`references/`/`assets/`。frontmatter 字段：`name`(1-64,小写字母数字连字符,不首尾/连续连字符)、`description`(1-1024,非空)、可选 `license`/`compatibility`(≤500)/`metadata`(map)/`allowed-tools`。

- **校验(硬拦截)**：`kind=skill` 的 import 与 commit 都强制校验——缺 `SKILL.md`、frontmatter 非法、或 `name` 与 skill 身份不符,一律拒绝(4xx + 明确原因)。`POST /validate` 供前端提交前预检。
- **单一真相 + 缓存**：`SKILL.md` frontmatter 是权威源(可移植、符合规范);commit 成功后把 frontmatter 缓存进 `skills` 表并刷新路由索引,预览"读一行 DB"即得全部基本信息,无第三份文件、无漂移。
- **prompt kind 豁免**:prompt 是单文件文本,不要求 SKILL.md,name 也可自由。
- 解析/校验在 `core::skillmd`(纯函数);校验时机(import/commit)由 server 层决定,core 的 `commit` 保持通用。

---

## 3. 路由器设计

路由信号 = 每个 skill 的 **`description`（Claude Skill 约定描述"何时使用"）+ name**，作用于每个 skill 的**已发布版本**（默认 = `main` head，后续可加显式 `publish`）。

```
需求(自然语言) ──▶ [路由器] ──▶ 排序结果 [{skill_id, name, score, matched_version}, ...]
                                top_k=1 → 单个 skill ；top_k=N → skill 列表
```

**检索-重排(retrieve-then-rerank)**,能力随配置逐级增强（桌面离线也能用，配了 LLM key 自动升级）：

```
Stage 1 召回  BM25 (SQLite FTS5) 取 Top-N 候选              ← 永远可用、离线、零依赖
             └ 小库捷径:skill 总数 ≤ 阈值时跳过召回,全量入 Stage 2(召回率 100%)
                    │ 候选(name+description)
Stage 2 重排  LLM 一次性从候选里选出/排序真正相关项(结构化输出) ← 配了 LLM API 才启用
                    ▼ 未配 AI → 直接返回 Stage 1 的 BM25 排序(优雅降级)
```

> **决策:用"BM25 召回 + LLM 重排",不上向量/embedding。** 理由:BM25(FTS5)已是很好的词法召回;LLM 只看 N 条短描述(几百 token、一次调用)就能补上"用词不同但意思相近";省掉向量库/embedding API/重算向量的全部运维,并复用已有的 `ai` 模块(与 refine 共用 `AI_*` 配置)。

| 档位 | 依赖 | 离线 | 说明 |
|------|------|:---:|------|
| **A. BM25 召回** | 无（rusqlite 自带） | ✅ | 基线;`fast` 模式 |
| **C. A + LLM 重排** | LLM API(`AI_*`) | ⚠️ | `smart` 模式;未配置自动降级为 A |

> **权衡(要认清)**:LLM 只能从 BM25 召回的候选里选——若某 skill 与 query **零词法重叠**,BM25 没召回它,LLM 也救不回(这正是向量能补而本方案不能的场景)。缓解:① 召回 N 放大;② **小库全量**捷径。反馈里的 `query/rating` 后续可作为重排的额外信号。

路由 API：`POST /route { query, top_k, mode?, rerank? }`。`rerank:true`(或 `mode:"smart"`)启用 LLM 重排;`top_k` 决定最终返回条数;`score` 是相关度(重排后按 LLM 排序给序位分)。

### 3.0 两种取用模式：精确指定 vs 模糊搜索

调用方有两类意图,都要支持:

| 模式 | 场景 | 接口 |
|------|------|------|
| **精确指定 (exact)** | "我就要 `pdf-parse` 这个 skill" —— 已知名字/ID,直取,不排序 | `GET /skill/by-name/{name}` 或 `POST /route { query, mode:"exact" }`(按 name 精确匹配) |
| **模糊搜索 (fuzzy)** | "帮我把 PDF 转文本" —— 自然语言需求,返回排序候选 | `POST /route { query, mode:"fuzzy", rerank? }`(BM25;`rerank:true` 加 LLM) |

> 精确取用是"消费通路"的一部分(P3.5):agent 有时明确知道要哪个 skill,不该被迫走模糊排序。fuzzy 才是上面档位 A/C 要增强的部分。

### 3.1 打分与后端可移植性（重要）

档位 A 的 `score` 来自 **SQLite FTS5 内置的 `bm25()`**——由 SQLite 计算,不是我们实现的。`skill_fts MATCH` + `bm25()` 都是 **SQLite 专有,不可移植**。

- **分数只在同一次查询内相对有意义**：BM25 的 IDF 依赖语料规模,库小时绝对值噪声大(可能显示 `0.0000` 即极小值)。可靠的是**排序**,不是绝对分。
- **换后端 = 换算法**:关键词排序函数各库不同,分数不可跨库比较。
  - SQLite → `bm25()`(正宗 BM25)
  - Postgres 原生 → `ts_rank`/`ts_rank_cd`(另一套,非 BM25);要真 BM25 需 `pg_search`/ParadeDB 扩展
  - MySQL → `MATCH…AGAINST`(自有相关度)
- 因此召回抽象成可替换实现,**每后端一个**(`SqliteFtsRouter` / 未来 `PgFtsRouter`)。因为分数本就相对,算法不同影响可控,关键是排序合理。
- **LLM 重排(档 C)天然跨库**:它只吃候选的 name+description,与数据库无关——所以换后端只冲击召回阶段(档 A),重排不受影响。

> 现状:纯 SQLite(server 默认 + 桌面 sidecar)。上 Postgres(P4)时,只需为 Postgres 重写召回(`ts_rank`/`pg_search`),重排层照用。

---

## 4. 反馈驱动的优化闭环

**入口**（两种来源）：
- **人工**：管理面板 skill 详情页的「反馈」按钮，或列表项快捷入口。
- **Agent 运行时**：`POST /skill/{id}/feedback`，agent 用完技能后回传是否好用、问题描述、以及当初的 `query`。

**优化**（AI 辅助）：
- `POST /skill/{id}/refine`：取该 skill 当前版本内容 + 该版本累积的 `open` 反馈 → 调 LLM 产出**改进后的 `SKILL.md`/资源草稿**。
- 草稿落到一个 `refine/*` 分支的新 commit（不直接动 main），人工在 diff 界面审核。
- 采纳并合并后：相关反馈标记 `applied`，路由索引对该 skill 重建。

这样反馈不是死数据，而是直接驱动下一版 skill。参考 prompt-shelf 的 `refine` 服务（OpenAI 兼容 `/chat/completions`），此处扩展为「内容 + 反馈」双输入。

---

## 5. 系统架构（Workspace 布局）

**核心思路(已按解耦要求调整)：前端永远只通过 HTTP API 访问后端——桌面和 web 走完全相同的一条代码路径。Tauri 只是"套壳"，不内嵌 core、不用 `invoke()` 访问业务逻辑。这样发布 web 与桌面共用同一份前端，互不影响。**

```
skill-shelf/                      (Cargo workspace)
├── crates/
│   ├── core/     ← 版本管理 + 路由 + 反馈 + 存储抽象（无 HTTP/UI 依赖，可单测）
│   │             CAS 对象库 · 索引 · Router trait · 反馈存储 · SKILL.md 解析/校验
│   └── server/   ← Axum HTTP API：JWT 认证 + REST + 路由/反馈/管理 (唯一后端)
├── app/
│   ├── src/          ← 前端 (React + Vite + TS)，只有一份 HTTP api 层
│   └── src-tauri/    ← Tauri v2 纯壳：加载前端 + (桌面)以 sidecar 拉起 server 二进制
└── docker-compose.yml
```

**前端 ↔ 后端只有一种关系：HTTP。**

- **Web**：前端(Vite 产物) → HTTP → 远程 `server`(Postgres)。
- **桌面**：Tauri 壳加载同一份前端 → HTTP → **本地 sidecar `server`**(Tauri 启动时拉起，监听 `127.0.0.1:<port>`，用本地 SQLite)。也可在设置里改为指向远程 server。
- Tauri 不依赖 `crates/core`；桌面/离线能力由"打包进壳的 server 二进制"提供，而非把 core 编进 Tauri。前端对二者无感——都是打**运行期可配置**的 HTTP 地址（默认值不同，用户可在设置里改，见 §6）。

- **core** 存储层用 trait 抽象：sidecar/桌面 = 本地 FS + SQLite；服务器 = FS/对象存储 + Postgres。
- **core** 同步实现（CAS + SQLite 天然同步），server 里用 `spawn_blocking` 包一层。
- **AI 能力**（refine、embedding、LLM 重排）走 server 内可选的 `ai` 模块，配置 API key 才启用。

### 数据库选型（已实现 ✅ 可切换）
- **索引抽象**：`core` 的 `Index` trait 隔离元数据/路由;CAS 文件对象库(blobs/trees/commits)两后端通用,放 `DATA_DIR/objects`。
- **SQLite**(`SqliteIndex`)：本地/桌面/小项目默认;FTS5 `bm25()` 路由;零依赖。
- **Postgres**(`PgIndex`)：共享 web 部署;`tsvector`+`ts_rank` 路由(原生,无扩展)。同步 `postgres` 客户端跑在**专用 worker 线程**上(避开 `#[tokio::main]` 的运行时嵌套 panic)。
- 同一份 `server` 二进制按 `DB` env 选:`postgres://…` → Postgres,否则 SQLite。两后端各跑通 e2e 20/20。
- (later,多实例)CAS 换共享对象存储 + PG 连接池。

---

## 6. 前端（同一套，双端适配）

- **技术栈**：React 19 + Vite 7 + TanStack Router/Query + shadcn/ui（与 prompt-shelf 一致）。
- **api 层单实现（HTTP）**：只有一份 axios 客户端，base URL **运行期可配置**（不写死）。同一个构建产物可随时切换后端。
- **后端地址运行期配置**：
  - 优先级：**用户在设置里填的地址 > 持久化存储 > `VITE_API_BASE` 默认值 > 内置兜底**。
  - 持久化：web 存 `localStorage`；桌面存 Tauri Store（`tauri-plugin-store`）。
  - axios 通过请求拦截器动态读取当前 base URL，改地址即时生效、无需重新构建。
  - 桌面默认填本地 sidecar 地址，用户可改成任意远程 server；web 默认空/同源，用户可填任意 server。
- **Tauri 只用于原生壳能力**（窗口、系统托盘、拉起/守护 sidecar、文件选择对话框、持久化 Store 等），不承载业务逻辑。
- **核心界面**：
  1. Skill 列表 / 详情
  2. 文件树 + 代码编辑器（CodeMirror）编辑 `SKILL.md` 与资源
  3. frontmatter 表单编辑器
  4. 提交历史 · 多文件 diff · 版本对比 · 回滚
  5. **路由测试台**：输入需求 → 实时看命中 skill 与分数
  6. **反馈入口 + 优化评审**：提交反馈；查看某 skill 的反馈列表；一键 refine → diff 审核 → 合并
  7. 导入/导出标准 skill 包
  8. **设置：后端地址配置**（填写/切换 server URL + 连接测试 + 持久化）
  9. 管理后台（server 模式）：用户 / 角色

---

## 7. API 概览（server）

| 分组 | 端点 |
|------|------|
| 认证 | `POST /user/signup` · `POST /user/signin` (JWT + Argon2) |
| Skill | `POST /skill` · `GET /skill?q=&kind=&limit=&offset=`(搜索/过滤/分页) · `GET /skill/{id}` · `GET /skill/by-name/{name}`(精确) · `DELETE /skill/{id}` |
| 消费 | `POST /route {mode:exact\|fuzzy}` · `GET /skill/{id}/bundle`(运行时取用,含全部文件) |
| 版本 | `POST /skill/{id}/branch` · `GET /skill/{id}/branches` |
| 提交 | `POST /skill/{id}/commit` · `GET /skill/{id}/commits` · `GET /commit/{cid}/tree` · `GET /commit/{cid}/file` |
| 差异 | `GET /diff?a=&b=` · `POST /rollback` |
| **路由** | `POST /route` 按需求返回单个/列表 |
| **校验** | `POST /validate` 按 Agent Skill 规范预检一组文件(不提交) |
| **反馈** | `POST/GET /skill/{id}/feedback` 提交/列表 · `POST /skill/{id}/feedback/{fid}/status` 改状态 |
| **优化** | `POST /skill/{id}/refine` 据反馈生成草稿(refine 分支,不动 main) · `POST /skill/{id}/refine/merge` 合并到 main(+重建索引+反馈标 applied) |
| 导入导出 | `POST /skill/import` (zip) · `GET /skill/{id}/export?version=` (zip) |
| 管理 | `/control/*` 用户管理（super_admin） |
| **自身设置** | `GET /config`(读,secret 打码) · `PUT /config {key:值}`(热更新;值为 str 或 JSON;`null` 删除;仅 admin) —— Skill Shelf 自用(AI/GitHub) |
| **配置中心** | `GET /config/namespaces` · `GET/PUT /config/namespace?namespace=X`(namespace KV,admin,secret 打码) · `GET/POST /config/clients`·`DELETE /config/clients/{id}`(service token,admin) · `GET /config/resolve?namespace=X`(消费方,`X-Config-Token` 头,返回**明文**合并配置) |
| 系统 | `GET /status` |

桌面端**不额外定义业务 Tauri command**——直接复用上面这套 HTTP API（打本地 sidecar）。sidecar 默认可跳过 JWT、以本地单用户运行。

---

## 8. 部署形态

- **Web**：`docker-compose`(已提供) —— server(Axum,**SQLite 卷**) + web(nginx 托管 Vite 产物,`/api` 代理到 server)。认证用 `JWT_SECRET`、AI 用 `AI_*` env 开启。Postgres 后端为 later(见 §11/PLAN P4)。
- **桌面**：`tauri build` 产出 macOS/Windows/Linux 安装包，**把 `server` 二进制作为 sidecar 打包进去**；启动时拉起 sidecar(本地 SQLite)，前端打 `127.0.0.1`。
- 同一份 `app/src` 前端 + 同一个 `server` 二进制，双目标构建;前端只认 HTTP，不区分桌面/web。

### 8.1 运行时配置（热更新，已实现 ✅）

配置 = 环境变量。值支持字符串或 JSON（如 `{"a":1}`）。分两类:

- **结构性(启动期,只读)**：`PORT` / `DATA_DIR` / `DB` / `JWT_SECRET` —— 决定进程绑定、存储与后端，改动需重启;`PUT /config` 拒绝(400)。
- **运行期(可热改)**：`AI_*`(base/model/key)、`GITHUB_*`(api base/token)、任意自定义 KV。admin 在 UI「Settings → Runtime config」改后**即时生效、无需重启**。

实现：`ConfigStore = Arc<RwLock<RuntimeConfig>>`(泛型 `BTreeMap<String, serde_json::Value>`),持久化到 `DATA_DIR/config.json`;取值优先级 **stored > env > default**。含 `KEY/TOKEN/SECRET/PASSWORD` 的键在读取时打码(`GET /config` 只返回是否已设置)。refine/route-rerank/GitHub 导入均在请求时从 `ConfigStore` 快照解析，AI 未配置时 refine 返回 400、rerank 优雅回退 BM25。写操作**先落盘再提交内存**(快照-persist-commit),磁盘失败则运行态不变、返回 500。

### 8.2 配置中心（其他服务的配置源，已实现 ✅）

Skill Shelf 兼作配置中心：其他服务启动时来这里取配置，而不各自读环境变量。数据模型在同一个 `RuntimeConfig` 里,但与 §8.1 的**自身设置完全隔离**:

- `vars` = Skill Shelf 自身设置(§8.1),**永不**经配置中心下发。
- `namespaces` = 配置中心。保留 `_global` 层放共享默认;每个服务/环境一个 namespace 叠加覆盖。消费方取 namespace X 得到 **merge(`_global`, X)**(X 覆盖全局)的**明文**。
- `clients` = service token。仅存 **SHA-256 哈希**;每个 grant 记录可读哪些 namespace。

**消费流程**:admin 在「Config center」页建 namespace + KV、签发 client(明文 token 只展示一次)→ 消费服务 `GET /config/resolve?namespace=X` 带 `X-Config-Token: shelf_…` 头 → 拿到合并明文。

**鉴权/安全**:
- `/config/resolve` 不走 admin JWT,由 handler 内校验 service token(坏 token 401、无该 namespace 授权 403)。其余 `/config/*`(namespaces/clients)admin-only。
- **关闭鉴权(无 `JWT_SECRET`)时,整个配置中心 `/config/*` 拒绝服务(403)**——否则攻击者可自助签发 token 读全部明文;`/config`(桌面自用)仍开放。
- `_global` 会合并进每一次 resolve,故**只放非敏感共享默认值**(任何有效 token 都能读到)。
- namespace 层的 `null` 语义是"从该层删除 key",无法覆盖式屏蔽 `_global` 的某个 key(v1 已知限制,可覆盖值、暂不能删)。

---

## 9. 分阶段实施计划

| 阶段 | 目标 | 交付 |
|------|------|------|
| **P0 骨架** ⏳部分已起 | 本地闭环 | workspace(core+server) · **core**(CAS+SQLite+版本管理) · 单测跑通「创建 skill→提交→历史→diff」 |
| **P1 server + 前端 + 路由基线** | HTTP 打通 | server crate: Axum REST(SQLite) · **FTS5 关键词路由(档A)** · 前端(列表/编辑器/历史/路由测试台)走 HTTP 跑通 |
| **P2 桌面壳** | 桌面可用 | `app/src-tauri` 纯壳 · 打包 server 为 sidecar · 启动拉起本地 server · `tauri build` 出包 |
| **P3 反馈闭环** | 反馈+优化 | 反馈模型/存储 · 反馈入口 UI · `refine`(内容+反馈→草稿) · refine 分支 + diff 审核 |
| **P4 Web 部署 + 管理** | 生产完善 | Postgres 支持 · JWT 认证 · 用户/角色管理 · docker-compose · 导入/导出标准 skill 包 · frontmatter 校验 |
| **P5 路由增强** | 语义路由 | 档 B(embedding+向量) → 档 C(LLM 重排) · 反馈信号纳入重排 · 路由质量评测 |

> 顺序说明：因为前端只走 HTTP，**必须先有 server（P1）前端才能联调**，桌面壳（P2）只是把同一个 server 当 sidecar 打包，所以挪到反馈/Web 之前。

> P0 已开工：`crates/core` 下已有 `Cargo.toml`、`error.rs`、`models.rs`、`store.rs`（CAS）。待补 `shelf.rs`（版本管理主逻辑）+ 反馈存储 + 测试。

---

## 10. 决策（已全部冻结 ✅）

| # | 决策 | 结论 |
|---|------|------|
| 1 | 前端 ↔ 后端 | 只走 HTTP；Tauri 纯壳 + server sidecar |
| 2 | Tauri 目录 | `app/src-tauri`（壳，不入 workspace crates） |
| 3 | 路由档位 | **A(BM25 召回)+ C(LLM 重排)**;**不上向量/embedding**(B 砍掉)。检索-重排,小库全量,未配 AI 降级为纯 BM25 |
| 4 | 前端框架 | **React 19** 全家桶（Vite + TanStack + shadcn/ui + CodeMirror） |
| 5 | 服务器数据库 | **Postgres**（桌面 sidecar 用 SQLite） |
| 6 | 反馈评分 | **-1 / 0 / +1** 三态 |
| 7 | 桌面离线 | **sidecar + 本地 SQLite**（可离线） |
| 8 | 阶段顺序 | P0 core → P1 server+前端+路由 → P2 桌面壳 → P3 反馈 → P4 Web/管理 → P5 路由增强 |

> 落地任务清单见 [`PLAN.md`](./PLAN.md)。

---

## 11. 未来功能与路线图

已建成"**管理 + 路由 + 反馈优化**"这条线(P0-P3)。作为一个完整的 **skill 服务**,还需补齐"**给 agent 消费**"与信任/协作等能力。按价值排序:

### 🔴 面向 agent 的消费通路(最高杠杆——让它从"管理工具"变"服务")
- **MCP server**:把注册中心暴露成 MCP,任何兼容 agent 都能 `list / route / fetch` skill 当工具。是"skill 服务"核心价值的兑现。
- **运行时取用 API**:`GET /skill/{id}/bundle` 按**渐进披露**返回(先 name+desc,再全文 + `scripts/`/`references/`/`assets/`),配瘦客户端/SDK 让 agent 直接加载。
- **搜索/浏览**:语义路由之外的关键词搜索 + 按 kind/标签过滤 + 分页。

### 🟠 质量与信任(skill 含可执行代码,尤为重要)
- **skill 测试/校验**:校验 SKILL.md 引用的 `scripts/`、相对链接是否存在;沙箱冒烟跑脚本。
- **安全扫描**:扫描脚本危险操作(skill 供应链安全);导入外部 skill 前必做。
- **使用分析**:路由/采用次数、命中率、反馈趋势——喂给"越用越好"闭环。
- **评分聚合**:反馈 → 每个 skill 的质量分/热度。

### 🟡 生命周期与协作
- **版本 tag / 发布通道**:给 commit 打 `v1.2.0`、`latest`/`stable` 通道(当前只有 commit,无语义版本)。
- **认证 + 组织 + RBAC**(见 P4)。
- **发布审批流**:draft → review → publish 门禁(复用分支 + publish 指针)。
- **可见性/访问控制**:private / org / public + 分享。
- **弃用/归档**:deprecated + supersede-by。
- **skill 间依赖**:依赖声明 + compatibility。

### 🟢 分发与集成
- **从 GitHub 仓库拉取 skill**(用户需求,优先):`POST /skill/import/github { url, ref?, subpath? }`。
  - 实现:解析 `owner/repo`,下 GitHub **zipball**(`https://api.github.com/repos/{o}/{r}/zipball/{ref}`,本身就是 zip)→ **复用现有 zip 导入 + 规范校验 + 剥离顶层目录**;私有库带 `GITHUB_TOKEN`。
  - 支持两种布局:整库即一个 skill(根有 `SKILL.md`),或库内多目录多 skill(遍历含 `SKILL.md` 的子目录,逐个导入)。
  - `subpath` 只导入某子目录;`ref` 指定分支/tag/commit。
  - 可选:记录来源 `source_url`+`ref` 到 skill metadata,便于后续"同步更新"。
- **CLI**:`push / pull / search / route`。
- **agentskills.io 生态同步**;**webhooks**(新版本/反馈通知);**审计日志**。

### 路线调整
> **MCP server + 运行时取用**比"部署/认证"更能定义产品,故新增 **P3.5** 前置:
> `… P3 反馈 ✅ → **P3.5 MCP + 运行时取用** → P5 路由增强(BM25 召回 + LLM 重排,不上向量) → P4 Web/部署/管理`
> 其余上述条目进入 backlog(见 PLAN),按需拉入具体阶段。
