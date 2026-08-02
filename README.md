# Skill Shelf

English | [简体中文](./README.zh-CN.md)

**A config center for the AI era.**

A traditional service's behavior is defined by its config; an AI agent's behavior is defined by its skills and prompts. They are the same thing — behavior that lives outside the code and is fetched at runtime. Skill Shelf manages both in one service:

- **For agents**: manage [Agent Skills](https://agentskills.io) and prompts — git-like versioning, natural-language routing to the best-fit skill, an AI feedback loop, and an **MCP server** any agent can consume directly. Skills follow the open Agent Skills spec, **with no lock-in to any particular agent or vendor**.
- **For services**: a lightweight **config center** — namespace isolation, draft/publish versioning, service-token authorization, one-click `.env` import. Services fetch their config from here at startup instead of each reading its own env vars.

Ships with a React + Tauri frontend (one codebase for desktop and web).

## Features

- **Versioning**: content-addressed storage (CAS, SHA-256) + commits / branches / diffs / rollback — manage skill content like git.
- **Routing**: `exact` (by name) / `fuzzy` (BM25 recall) / `smart` (BM25 → LLM rerank; degrades to BM25 when no AI is configured).
- **Spec validation**: `SKILL.md` frontmatter is hard-validated against the Agent Skills spec on commit; prompts are exempt.
- **Feedback loop**: -1/0/+1 ratings + AI drafts a revision from feedback (on a `refine` branch, main untouched) → merge.
- **MCP access**: `skill-shelf-mcp` exposes route / browse / load / read-file / feedback tools over stdio, proxying to the REST API.
- **Hot-reloadable settings**: AI / GitHub settings can be changed online by an admin — effective immediately, no restart.
- **Config center**: two-layer merge of a shared `_global` namespace and per-service namespaces; service tokens (hash-only storage, per-namespace grants) pull the merged **plaintext** config.
- **.env import**: paste or pick an existing `.env` into any namespace, preview added / overwritten / skipped / invalid lines before merging into the draft; consumers are unaffected until publish.
- **Pluggable backend**: SQLite (local / small) or Postgres. **Optional auth**: set `JWT_SECRET` to enable JWT + Argon2; the first user becomes admin.
- **Bilingual UI**: one-click zh/en toggle in the header, follows the system language, dependency-free i18n layer; light and dark themes.

## Screenshots

The Cobalt design system (dark theme, Chinese UI shown; light/dark and zh/en both supported):

| Skills | Skill detail (editor · feedback · history) |
| --- | --- |
| ![Skills](docs/screenshots/skills.png) | ![Skill detail](docs/screenshots/skill-detail.png) |

| Natural-language routing | Config center (draft diff · masked secrets) |
| --- | --- |
| ![Route](docs/screenshots/route.png) | ![Config center](docs/screenshots/config-center.png) |

**One-click .env import** — paste or pick a file, per-key preview before anything lands:

![.env import](docs/screenshots/env-import.png)

## Architecture

```
crates/
  core     skill-shelf-core   CAS storage + version model + Index abstraction (SQLite/Postgres) + validation
  server   skill-shelf-server Axum REST API, auth, settings (own config + the config center)
  mcp      skill-shelf-mcp    MCP server (stdio), proxies to REST
app/       React 19 + Vite + TanStack Router/Query + shadcn/ui + Tailwind; Tauri desktop shell
```

Design and API details in [DESIGN.md](./DESIGN.md); milestones in [PLAN.md](./PLAN.md).

## Quick start

### Backend

```bash
# SQLite (default). No JWT_SECRET = open mode (fine for local/desktop use)
cargo run -p skill-shelf-server
# → http://127.0.0.1:8080   data lives in ./data

# Enable auth + choose data dir/port
JWT_SECRET=change-me DATA_DIR=./data PORT=8080 cargo run -p skill-shelf-server

# Postgres (CAS blobs stay in DATA_DIR, the index moves to PG)
DB=postgres://user:pass@localhost/skillshelf cargo run -p skill-shelf-server
```

### Frontend

```bash
cd app
pnpm install
pnpm dev            # http://localhost:5173, talks to 127.0.0.1:8080 by default
```

The backend address can be switched at runtime under "Settings → Backend" (one build can point at any server).

### MCP (for agents)

```bash
# Skills only:
SKILL_SHELF_URL=http://127.0.0.1:8080 cargo run -p skill-shelf-mcp
# Also let the agent fetch config from the config center (needs a service token):
SKILL_SHELF_URL=http://127.0.0.1:8080 \
SKILL_SHELF_CONFIG_TOKEN=shelf_… \
  cargo run -p skill-shelf-mcp
```

Runs over stdio and exposes: `route`, `list_skills`, `load_skill`, `read_skill_file`, `feedback`, plus `get_config` (fetch a namespace's published, merged config from the config center instead of reading env vars; requires `SKILL_SHELF_CONFIG_TOKEN`).

## Configuration

Config = env vars; values may be strings or JSON. Two classes:

| Class | Keys | When changeable |
|-------|------|-----------------|
| Structural (startup) | `PORT` · `DATA_DIR` · `DB` · `JWT_SECRET` · `ADMIN_USERNAME` / `ADMIN_PASSWORD` (optional; provisions the admin, only while no user exists yet) · `LOG_FORMAT=json` (JSON log lines; default is human-readable) · `RUST_LOG` (filter levels) | Restart required |
| Runtime (hot-reload) | `AI_BASE_URL` · `AI_API_KEY` · `AI_MODEL` · `GITHUB_TOKEN` · `GITHUB_API_BASE` · any custom key | Admin edits under "Settings → Service settings", effective immediately |

Precedence: **stored value > env var > default**; keys containing `KEY/TOKEN/SECRET/PASSWORD` are masked on read endpoints.

### Config center

Other services fetch their config from here. An admin creates namespaces + KV and issues service tokens on the "Config center" page (the plaintext token is shown exactly once); consumers pull the merged plaintext with an `X-Config-Token` header:

```bash
curl -H "X-Config-Token: shelf_…" \
  "http://127.0.0.1:8080/config/resolve?namespace=service-a/prod"
# → plaintext JSON of merge(_global, service-a/prod), latest published version
```

- **Versioning**: each namespace has draft / published / history. Edits touch the draft; consumers' `resolve` always reads the **latest published version**. Changes go live only when an admin reviews the field-level diff and hits **Publish**. Past versions can be viewed and **rolled back** (loaded into the draft, then published) — "roll back, then publish".
- **.env import**: "Import .env" in the namespace toolbar — paste or pick a file, parsed client-side (comments, `export` prefix, quote escapes all handled), per-key preview of added / overwritten / skipped before merging into the draft. Values are always imported as strings, no type guessing.
- `_global` is merged into every resolve — keep **only non-secret shared defaults** there.
- Skill Shelf's own settings are **fully isolated** from the config center; its own secrets can never leak through resolve.
- With auth disabled (no `JWT_SECRET`), the whole config center under `/config/*` refuses service.

## Deployment

```bash
# Web: server (SQLite volume) + nginx serving the frontend
docker-compose up -d
# With Postgres:
docker-compose --profile pg up -d
```

Desktop: `cd app && pnpm tauri build` — bundles `server` as a sidecar (local SQLite, port 8765).

## License

[MIT](./LICENSE)
