# Skill Shelf — 实施计划 (PLAN)

> 配套设计见 [`DESIGN.md`](./DESIGN.md)。决策已全部冻结（DESIGN §10）。
> 本文件是可执行的任务清单，按阶段推进；每阶段有明确交付与验收标准。

## 冻结的技术选型

- **后端**：Rust workspace = `crates/core`（版本管理+路由+反馈+存储）+ `crates/server`（Axum HTTP，唯一后端）。
- **前端**：React 19 + Vite 7 + TypeScript + TanStack Router/Query + shadcn/ui + Tailwind v4 + CodeMirror 6 + axios + Zustand。**只有一份 HTTP api 层。**
- **桌面**：Tauri v2 纯壳（`app/src-tauri`），打包 `server` 二进制为 sidecar，本地 SQLite，可离线。
- **数据库**：server=Postgres；桌面 sidecar=SQLite（存储 trait 隔离）。
- **路由**：`Router` trait，A(FTS5)→B(embedding 向量)→C(LLM 重排) 渐进。
- **反馈**：-1/0/+1 三态 + 文本，驱动 AI `refine`。

---

## P0 — core 骨架（版本管理）  ✅ 完成

**目标**：`crates/core` 跑通「创建 skill → 提交 → 历史 → diff」本地闭环，单测绿。

- [x] workspace `Cargo.toml` + `core/{Cargo.toml, error.rs, models.rs, store.rs(CAS)}`
- [x] `core/src/shelf.rs`：`Shelf::open(root)` + SQLite 迁移（`skills`、`branches` 表）
- [x] Skill CRUD：`create_skill / list_skills / get_skill / delete_skill`
- [x] 分支：`create_branch / list_branches / get_branch`（默认分支 `main`）
- [x] 提交：`commit()`（写 blob→tree→commit 对象，更新 branch head）、`get_commit`、`list_commits`（沿 parent 回溯）
- [x] 读取：`read_tree(commit)`、`read_file(commit, path)`
- [x] 回滚：`rollback(skill, branch, to_commit)`
- [x] `core/src/diff.rs`：`diff(commit_a, commit_b) -> Vec<FileDiff>`（`similar` 行级 diff）
- [x] `core/src/lib.rs`：模块导出
- [x] `core/tests/loop_test.rs`：3 个测试（主闭环 + 增删文件检测 + 空名校验）

**验收**：✅ `cargo test -p skill-shelf-core` — 3 passed。

---

## P1 — server + 前端 + 路由基线（档 A）

**目标**：HTTP 打通端到端；前端能建 skill、编辑提交、看历史/diff、用路由测试台。

### 后端 `crates/server`  ✅ 完成
- [x] Axum 应用骨架（`main.rs` + `handlers.rs` + `error.rs`），配置走 env（`DATA_DIR`、`PORT`）
- [x] `Shelf` 经 `Arc<Mutex<>>` 共享（core 同步，锁内快操作、不跨 await）
- [x] REST 端点（对应 DESIGN §7）：skill CRUD、branch、commit（JSON+base64 文件）、commits、tree、file、diff、rollback
- [x] `GET /status` 健康检查、CORS(permissive)、Trace 层
- [x] **路由档 A**：core `route()`——SQLite FTS5(BM25) 索引 skill `name+description`；create/delete 时维护索引
- [x] `POST /route { query, top_k }` → 排序结果（top_k=1 单个 / N 列表）
- [x] 端到端冒烟测试通过：建 skill→提交×2→历史→tree/file→diff→路由(pdf/csv 命中)→回滚→404
- [x] **prompt/skill 双类型**：`kind` 字段(`skill`/`prompt`)——prompt = 单文件 skill，共用版本/路由机制
- [x] **zip 导入/导出**（P4 提前）：`POST /skill/import`(解压+校验 SKILL.md+剥离顶层目录+防 zip-slip) · `GET /skill/{id}/export`(打包某版本为 zip)
- [x] **Agent Skill 规范一致性**(见 DESIGN §2.4)：`core/skillmd.rs` 完整解析(serde_yaml_ng)+校验(name/description/compatibility 规则)；`kind=skill` 的 import/commit 硬拦截；`POST /validate` 预检；frontmatter 缓存进 `skills` 表(license/compatibility/metadata/allowed-tools)+刷新 FTS；prompt kind 豁免
- [ ] （later）core 调用改 `spawn_blocking`；发布指针 + 版本变更时重建 FTS（当前按 skill.description 索引）

### 前端 `app/`
- [x] Vite + React 19 + TS 脚手架（Tauri 壳留到 P2）
- [x] **shadcn/ui + Tailwind v4**（radix base + nova）：全部页面用 Card/Field/Input/Select/Button/Badge/Alert/Empty/Skeleton/Sonner 重写；语义色、无原始色；4 页面 browse 实测无 console 错误
- [x] **规范校验 UI + 完整 frontmatter 展示**：详情页 Manifest 面板显示 license/compatibility/allowed-tools/metadata；skill kind 有 Validate 按钮(调 `/validate` 预检);commit 被规范拒绝时 toast 明确原因
- [x] TanStack Router（代码式路由，无 codegen 插件）+ TanStack Query + axios + CodeMirror
- [x] **后端地址运行期可配置**：axios 拦截器每次请求读 base URL；优先级 `localStorage > VITE_API_BASE > 兜底`；**设置页**可填写/切换 + 连接测试 + 持久化（换后端免重构建）
- [x] 页面：skill 列表(+创建 skill/prompt + import zip + export)、skill 详情（文件树 + CodeMirror + 提交 + 历史 + diff + rollback）、**路由测试台**、**设置**
- [x] 浏览器端到端验证：列表渲染后端数据、路由返回排序、详情提交→历史+1、diff 面板、无 console 错误

**验收**：✅ 起 server + `pnpm dev`；建 skill→编辑→提交→历史/diff、路由查询均通过（browse 实测）。
- [ ] （later）score 显示精度（BM25 小数值 toFixed(4) 显示 0.0000）；样式打磨阶段处理

---

## P2 — 桌面壳（Tauri sidecar）  ✅ 完成（macOS 本机验证）

**目标**：`tauri build` 出桌面包，离线可用。

- [x] `app/src-tauri` 接入 Tauri v2（纯壳,不依赖 core/server;独立 `[workspace]` 不入主 workspace）
- [x] `server` 编译为 sidecar 二进制(`binaries/skill-shelf-server-<triple>`),`tauri.conf.json` `externalBin`
- [x] Rust `setup()` 拉起 sidecar（`127.0.0.1:8765`,`DATA_DIR`=app_data_dir + 本地 SQLite,`tauri-plugin-shell`;stdout/stderr 进 app log）
- [x] 前端检测 Tauri(`__TAURI_INTERNALS__`)→ 默认打 `127.0.0.1:8765`(仍可在设置页改)
- [x] 安全(least privilege):限制性 CSP(XSS 主防线);webview 无 fs/shell 业务权限(前端只走 HTTP)
  - ⚠️ **`freezePrototype` 必须关**:开启会白屏——前端某依赖在 import 期改原型,冻结导致模块初始化抛错、JS 完全不执行。CSP 保留即可。(诊断方式:probe server 占 8765 看 webview 是否发 `GET /skill`)
- [x] `tauri build --bundles app` 产出 `Skill Shelf.app`,sidecar 打包进 `Contents/MacOS/`
- [x] **运行时验证**:`open` 启动 app → 自动拉起 sidecar → `127.0.0.1:8765/status` OK → 全新本地 SQLite(与 demo 隔离)
- [ ] （later）动态端口(避免 8765 占用);桌面配置改用 `tauri-plugin-store`(当前用 localStorage,够用);sidecar rebuild 脚本;其它平台交叉编译

**验收**：✅ 桌面 app 启动即拉起本地 sidecar,离线可用(本地 SQLite)。

---

## P3 — 反馈闭环  ✅ 完成

**目标**：反馈入口 + AI 据反馈优化 skill。

- [x] `core`：`feedback` 表 + `add_feedback / list_feedback / open_feedback / set_feedback_status / mark_feedback_applied`（-1/0/+1）；分支支持 `point_branch / merge_branch`
- [x] `server`：`POST/GET /skill/{id}/feedback`、`POST /skill/{id}/feedback/{fid}/status`
- [x] `server/ai.rs`：`refine`——取 main head SKILL.md + `open` 反馈 → OpenAI 兼容 `/chat/completions`（env 配置,未配置明确 400）→ 生成草稿落 `refine` 分支（**不动 main**）；LLM 调用不持锁
- [x] `POST /skill/{id}/refine`（返回 commit + diff）· `POST /skill/{id}/refine/merge`（合并→main + `set_skill_meta` 重建索引 + 反馈标 `applied`）
- [x] 前端 `FeedbackPanel`：👎/—/👍 ToggleGroup + 反馈列表 + 一键 refine → diff 审核 → Merge/Discard
- [x] agent 回传通道：`source=agent` + `query`

**验收**：✅ 端到端跑通（LLM 用 stub）——提交反馈→refine 草稿(main 不动)→diff 审核→merge→desc 同步/路由更新/反馈变 `applied`;UI browse 实测全流程;未配置 AI 返回明确 400。core 6 测试通过。

---

## P3.5 — MCP + 运行时取用（让 agent 真正能用）  ⬅️ 新增前置

**目标**：把服务变成 agent 可消费的 skill 供给端；路由支持**精确指定 + 模糊搜索**两种模式。

- [x] **精确取用**：`GET /skill/by-name/{name}`（按名字直取,不排序）· `POST /route { query, mode:"exact" }`（命中返回单条 score=1.0,未命中空）
- [x] **模糊路由**：`POST /route`(BM25,`mode:"fuzzy"` 默认;P5 再加向量)
- [x] **运行时 bundle**：`GET /skill/{id}/bundle?commit=` → `{id,name,description,kind,commit,files[]}`(全部文件 base64),供 agent 加载
- [x] **搜索/浏览**：`GET /skill?q=&kind=&limit=&offset=`(substring + kind 过滤 + 分页;`search_skills`)
- [x] 后端冒烟通过：exact/fuzzy/by-name/bundle/search 全部验证
- [x] **MCP server**（`crates/mcp` → `skill-shelf-mcp`,rmcp 0.16 stdio,瘦 HTTP 桥接,`SKILL_SHELF_URL` 配置）：5 工具 `route_skill / search_skills / fetch_skill / read_skill_file / submit_feedback`（对应发现→激活→执行→闭环）
- [x] MCP 端到端验证(stdio JSON-RPC)：initialize / tools/list(5) / route_skill / fetch_skill / submit_feedback 均通过,反馈以 `source=agent` 落库
- [x] 前端：路由测试台加"精确/模糊"切换(ToggleGroup;exact 隐藏 top_k)

**验收**：✅ MCP agent 可 `route_skill(need)` → `fetch_skill` 加载 SKILL.md → `submit_feedback` 反哺;`by-name` 精确直取;关键词搜索可用;前端可切精确/模糊。

**集成**：`claude mcp add skill-shelf --env SKILL_SHELF_URL=http://127.0.0.1:8765 -- /path/to/skill-shelf-mcp`

---

## P4 — Web 部署 + 管理

**目标**：Postgres + 认证 + Docker，生产可部署。

- [x] **JWT（`jsonwebtoken`）+ Argon2 认证**（env `JWT_SECRET` 开关;未设=开放/桌面无摩擦）：`users` 表 + `/user/signup|signin`(首个用户=admin) + 中间件保护"创作类"写操作;读/route/validate/agent 反馈开放。实测:无 token 401、读开放、带 token 200、错误凭证 401、关闭时全开放
- [x] `SKILL.md` frontmatter 校验 —— 已在 P3.5 前(spec 一致性)落地
- [x] 导入/导出标准 skill 包（zip）——已在 P1 提前实现
- [x] **可选数据库后端**：`core` 索引抽象为 `Index` trait(CAS 文件对象库两后端通用),`SqliteIndex`(FTS5 BM25)+ `PgIndex`(Postgres,`tsvector`/`ts_rank`,专用 worker 线程避开 tokio 运行时冲突);server 按 `DB` env 选(postgres:// → PG,否则 SQLite);blobs 仍在 `DATA_DIR`
- [x] **部署配置**：`Dockerfile.server` + `app/Dockerfile`(nginx) + `docker-compose.yml`(默认 SQLite;`--profile pg` 起 Postgres);compose config 校验通过
- [x] **运行时配置热更新**(见 DESIGN §8.1)：`ConfigStore`(泛型 KV,值 str|json)持久化 `DATA_DIR/config.json`,优先级 stored>env>default;结构性键(PORT/DATA_DIR/DB/JWT_SECRET)锁定为启动期;secret 读取打码;admin `/config` get/put,AI/GitHub 即时生效无需重启;refine/rerank/import 请求时读快照;写操作快照-落盘-提交(磁盘失败→500 且运行态不变)。前端「Settings → Service settings」面板(AI/GitHub 分组 + 自定义 KV)。实测:refine 配置前 400→PUT 后 200(无重启)、secret 打码、锁定键 400、JSON 值、持久化
- [x] **配置中心**(见 DESIGN §8.2)：其他服务从这里取配置。`namespaces`(保留 `_global` + 各服务两级合并)与自身设置 `vars` 隔离(自身密钥不外泄);service token 仅存 SHA-256、按 namespace 授权;`GET /config/resolve`(`X-Config-Token` 头)返回合并明文;admin CRUD namespace/client(token 一次性展示)。关闭鉴权时 `/config/*` 全拒绝(403)。前端独立「Config center」页(仅 admin,与 Skill 服务设置分开)。经独立 review 修复 HIGH/MEDIUM(鉴权关闭敞开、持久化原子性、前端 401 风暴/新建 namespace 选中/token 复制)。实测 config-center e2e **27/27**(明文下发/越权 403/无效&撤销 token 401/self 隔离/`..` 拒绝/锁定键 400/删不存在 404/鉴权关闭 403/持久化)
- [x] **配置版本管理**(见 DESIGN §8.3)：namespace 改为 `draft + versions[]`,编辑改草稿、`resolve` 读已发布,`publish`/`versions`/`version`/`rollback`(载草稿)/`diff`(逐字段,secret 打码);旧格式按 `versions` 键识别迁移为 v1;加载解析失败 panic 不覆盖、去 `deny_unknown_fields` 向前兼容。前端草稿编辑 + 未发布徽标/圆点 + diff + Publish(备注)+ History(查看/回退)。经独立 review 修复(后端加载静默清空 HIGH、迁移键名碰撞/向前兼容 MEDIUM;前端 diff 头 v0、切 ns 重置 History 等)。实测 versioning e2e **25/25**(草稿不影响 resolve/publish 生效/no-op 400/diff/历史/回退需再发布/`_global` 已发布合并)+ 迁移单测 5 项
- [ ] （later,多实例）CAS 改共享对象存储(当前 blobs 在本地 FS,单实例可用);连接池(当前 PG 单连接 worker);`/control/*` 用户/角色管理

**验收**：✅ **两后端各跑通完整 e2e 20/20**(SQLite + 真实 dockerized Postgres);Postgres 认证实测(首用户 admin/无 token 401);docker compose(SQLite + pg profile)config 校验通过。

---

## P5 — 路由增强（BM25 召回 + LLM 重排，不上向量）

**目标**：检索-重排;LLM 补语义,复用 `ai` 模块;未配 AI 优雅降级。

- [x] `ai::rerank(query, candidates)`——候选 name+description 交 LLM,结构化返回相关 id(含"都不相关→空",过滤幻觉 id);抽出共享 `chat()` helper
- [x] `/route` 加 `rerank:bool` / `mode:"smart"`:小库全量 or BM25 Top-20 → LLM 重排 → top_k
- [x] **小库全量捷径**:skill 总数 ≤ 15 跳过召回,全量入重排
- [x] **降级**:未配 `AI_*` 或 AI 报错时 `rerank` 请求回退纯 BM25(200,不报错)
- [x] stub 验证:rerank 覆盖 BM25(选中指定候选)+ 全量路径 + 优雅降级
- [x] 前端:路由台加 Fuzzy / Smart / Exact 三档
- [ ] （later）反馈信号(`query`+`rating`)纳入;路由质量评测集(命中率/MRR)
- [x] ~~档 B 向量/embedding~~（已决策砍掉,见 DESIGN §3）

**验收**：✅ `rerank`/`smart` 时 LLM 决策驱动排序;未配 AI 降级为 BM25;stub 全绿。

---

## Backlog（按需拉入具体阶段，见 DESIGN §11）

- **质量/信任**：
  - ✅ **引用校验(lint)**:`missing_references` 检查 SKILL.md 引用的 `scripts/`/`references/`/`assets/` 与 markdown 链接是否存在;`/validate` 返回**非致命 warnings**(不硬拦截);前端 Validate 按钮 toast 警告。(later:沙箱冒烟跑脚本)
  - 安全扫描(脚本危险操作/供应链) · 使用分析(路由/采用/命中率/反馈趋势) · 评分聚合(质量分/热度)
- **生命周期/协作**：版本 tag + `latest`/`stable` 通道 · 发布审批流(draft→review→publish 门禁) · 可见性(private/org/public)+分享 · 弃用/归档(deprecated + supersede-by) · skill 间依赖
- **分发/集成**：
  - ✅ **从 GitHub 拉取 skill**(已实现):`POST /skill/import/github { url, ref?, subpath? }` → 下 zipball(复用 `unzip_bytes`+`import_files_as_skill`+规范校验+剥顶层目录);支持 URL 多形态(全 URL/`/tree/ref/path`/`owner/repo` 简写)、整库单 skill / 库内多 skill(遍历含 SKILL.md 的子目录)、`subpath` 限定;私有库/限流用 `GITHUB_TOKEN`;`GITHUB_API_BASE` 覆盖(企业版/测试)。stub 验证:单/多/subpath/文件落位/坏库 400 全绿。
  - CLI(`push/pull/search/route`) · agentskills.io 同步 · webhooks · 审计日志 · (later)记录 `source_url`+`ref` 供同步更新

---

## Review & Test（2026-07-19，三轮：GitHub 导入）

- 独立 review 确认**无 SSRF**(host 由 env 固定,路径段无法改 authority;跨域重定向 reqwest 剥离 Authorization,token 不泄露)、zip 遍历安全、锁纪律正确。修复:
  - **[Medium] 下载/解压无上限 → 内存 DoS**:`fetch_url` 加 60s 超时 + 分块读 100MB 上限;`unzip_bytes` 加 200MB 解压上限(`take` 限制单条目内存)——同时保护 zip 上传路径的 zip-bomb。
  - **[Low] 嵌套 skill 双重导入**:改为 `top_most_dirs` 只保留最上层 skill 目录(root/子目录嵌套统一处理)。
  - **[Low] `ref` 未校验**:拒绝含 `..`/空白/控制字符。
  - **[Low] `/tree/<ref>/<path>` 中带斜杠的 ref 歧义**:文档标注,建议显式传 `ref`。
- 复测:single/multi/subpath 导入正常、invalid ref 400、e2e 20/20、全量单测通过。

## Review & Test（2026-07-19，二轮：auth + rerank）

- **独立 code-review(agent) 复审新代码**(auth/rerank):检出 1 项 Medium 已修 + 验证:
  - **signup 首用户 admin 竞态(TOCTOU)**:原 `count_users()` 与 `create_user()` 两次独立加锁 → 并发注册可能都成 admin。**已改为单锁作用域内 count+create**(实测:首=admin、次=user、重名 400)。
  - 附带修 rerank 路径 `top_k==0` 返回 1 条的小不一致 → 现返回空。
  - 其余(auth 中间件放行清单、JWT 校验无 alg 混淆、Argon2、rerank 不持锁跨 await、幻觉 id 过滤、malformed 输出不 panic)复审均正确。
- 认证实测 7 项通过;e2e 20/20(release);全量单测通过。

## Review & Test（2026-07-19，一轮）

- **独立 code-review(agent)**:检出 4 项,其中 2 项 Medium 已修 + 验证:
  - refine 草稿提交前现做**规范校验**(拒绝 LLM 非法输出,不入版本库)
  - refine/merge **合并前校验**(main 永不指向非法 commit;实测 garbage→400 且 main 不动)
  - 附带硬化:mutex 中毒不再级联 panic(`into_inner` 优雅降级)
  - 其余 2 项(单用户下 refine TOCTOU、merge 为整树覆盖)记为可接受/后续。
- **全量测试**:core 6 + skillmd 4 单测通过;三 crate release 编译干净。
- **端到端 e2e**(server+MCP+refine stub):**20/20 通过**——创建/规范校验/提交/diff/精确+模糊路由/by-name/bundle/搜索/zip 导入导出/回滚/反馈+refine+merge/ MCP stdio 全绿。

## 里程碑顺序

```
P0 core ✅  →  P1 HTTP端到端 ✅  →  P2 桌面出包 ✅  →  P3 反馈闭环 ✅
  →  P3.5 MCP+运行时取用(精确/模糊)  →  P5 语义路由  →  P4 Web/部署/管理
```
> P3.5 前置理由:"给 agent 消费"比"部署/认证"更能定义产品价值(见 DESIGN §11)。

> 下一步：完成 **P0** —— 补 `shelf.rs` + `diff.rs` + `lib.rs` + 测试，`cargo test` 绿后进入 P1。
