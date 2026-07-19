//! Skill Shelf MCP server: a thin stdio bridge that exposes the registry to any
//! MCP-compatible agent. Each tool proxies to the REST API at `SKILL_SHELF_URL`
//! (default: the desktop sidecar on 127.0.0.1:8765), keeping the HTTP server the
//! single source of truth.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{schemars, tool, tool_handler, tool_router, ErrorData, ServerHandler, ServiceExt};
use serde::Deserialize;
use serde_json::{json, Value};

const DEFAULT_URL: &str = "http://127.0.0.1:8765";

#[derive(Clone)]
struct Shelf {
    base: String,
    http: reqwest::Client,
    /// Service token for the config center (`X-Config-Token`). When None, the
    /// `get_config` tool is still advertised but errors clearly at call time.
    config_token: Option<String>,
    tool_router: ToolRouter<Self>,
}

fn oops(msg: impl Into<String>) -> ErrorData {
    ErrorData::internal_error(msg.into(), None)
}

fn ok_json(v: &Value) -> Result<CallToolResult, ErrorData> {
    Ok(CallToolResult::success(vec![Content::text(
        serde_json::to_string_pretty(v).unwrap_or_default(),
    )]))
}

impl Shelf {
    fn new() -> Self {
        let base = std::env::var("SKILL_SHELF_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        Self {
            base,
            http: reqwest::Client::new(),
            config_token: std::env::var("SKILL_SHELF_CONFIG_TOKEN").ok().filter(|s| !s.is_empty()),
            tool_router: Self::tool_router(),
        }
    }

    /// Fetch a namespace's published, merged config from the config center.
    /// Sends the service token as `X-Config-Token`.
    async fn resolve_config(&self, namespace: &str) -> Result<Value, ErrorData> {
        let Some(token) = &self.config_token else {
            return Err(oops(
                "config center access requires the SKILL_SHELF_CONFIG_TOKEN environment variable",
            ));
        };
        // The token and the resolved secrets both travel in this request, so
        // refuse to send them in cleartext to anything but a loopback host.
        if !is_loopback_or_https(&self.base) {
            return Err(oops(format!(
                "refusing to send the config token in cleartext to a non-loopback host ({}); \
                 use an https:// SKILL_SHELF_URL",
                self.base
            )));
        }
        let r = self
            .http
            .get(format!("{}/config/resolve?namespace={}", self.base, urlencode(namespace)))
            .header("X-Config-Token", token)
            .send()
            .await
            .map_err(|e| oops(format!("request failed: {e}")))?;
        if !r.status().is_success() {
            return Err(oops(format!("backend {}: {}", r.status(), r.text().await.unwrap_or_default())));
        }
        r.json().await.map_err(|e| oops(format!("bad response: {e}")))
    }

    async fn get(&self, path: &str) -> Result<Value, ErrorData> {
        let r = self
            .http
            .get(format!("{}{path}", self.base))
            .send()
            .await
            .map_err(|e| oops(format!("request failed: {e}")))?;
        if !r.status().is_success() {
            return Err(oops(format!("backend {}: {}", r.status(), r.text().await.unwrap_or_default())));
        }
        r.json().await.map_err(|e| oops(format!("bad response: {e}")))
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, ErrorData> {
        let r = self
            .http
            .post(format!("{}{path}", self.base))
            .json(&body)
            .send()
            .await
            .map_err(|e| oops(format!("request failed: {e}")))?;
        if !r.status().is_success() {
            return Err(oops(format!("backend {}: {}", r.status(), r.text().await.unwrap_or_default())));
        }
        r.json().await.map_err(|e| oops(format!("bad response: {e}")))
    }

    /// Resolve a name-or-id to a skill object (tries exact name, then id).
    async fn resolve(&self, name_or_id: &str) -> Result<Value, ErrorData> {
        if let Ok(v) = self.get(&format!("/skill/by-name/{name_or_id}")).await {
            return Ok(v);
        }
        self.get(&format!("/skill/{name_or_id}")).await
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
struct RouteParams {
    /// Natural-language need (fuzzy), or the exact skill name (exact).
    query: String,
    /// "fuzzy" (default, ranked) or "exact" (resolve by name).
    mode: Option<String>,
    /// Max results for fuzzy mode.
    top_k: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SearchParams {
    /// Substring to match in name/description.
    q: Option<String>,
    /// Filter by kind: "skill" or "prompt".
    kind: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SkillRef {
    /// Skill name or id.
    skill: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct FileRef {
    /// Skill name or id.
    skill: String,
    /// File path within the skill (e.g. "scripts/run.py").
    path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ConfigParams {
    /// Namespace to fetch (e.g. "service-a/prod"). Defaults to "_global" (shared
    /// defaults). The result already includes the merged "_global" layer.
    namespace: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct FeedbackParams {
    /// Skill name or id.
    skill: String,
    /// -1 (bad) / 0 / +1 (good).
    rating: i32,
    /// What worked or what to fix.
    content: String,
    /// The need/query that led here (improves routing).
    query: Option<String>,
}

#[tool_router]
impl Shelf {
    #[tool(description = "Find skills for a need. mode=fuzzy ranks by relevance; mode=exact resolves a skill by name. Returns name, description, score.")]
    async fn route_skill(
        &self,
        Parameters(p): Parameters<RouteParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let body = json!({
            "query": p.query,
            "mode": p.mode.unwrap_or_else(|| "fuzzy".into()),
            "top_k": p.top_k.unwrap_or(5),
        });
        ok_json(&self.post("/route", body).await?)
    }

    #[tool(description = "Browse/search skills by substring and/or kind. Returns metadata only (discovery).")]
    async fn search_skills(
        &self,
        Parameters(p): Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut qs = Vec::new();
        if let Some(q) = p.q {
            qs.push(format!("q={}", urlencode(&q)));
        }
        if let Some(k) = p.kind {
            qs.push(format!("kind={}", urlencode(&k)));
        }
        let path = if qs.is_empty() {
            "/skill".to_string()
        } else {
            format!("/skill?{}", qs.join("&"))
        };
        ok_json(&self.get(&path).await?)
    }

    #[tool(description = "Load a skill for use: returns the full SKILL.md text plus the list of bundled files (activation). Use read_skill_file to read a specific script/reference.")]
    async fn fetch_skill(
        &self,
        Parameters(p): Parameters<SkillRef>,
    ) -> Result<CallToolResult, ErrorData> {
        let skill = self.resolve(&p.skill).await?;
        let id = skill["id"].as_str().ok_or_else(|| oops("skill has no id"))?;
        let bundle = self.get(&format!("/skill/{id}/bundle")).await?;
        let empty = vec![];
        let files = bundle["files"].as_array().unwrap_or(&empty);
        let skill_md = files
            .iter()
            .find(|f| f["path"] == "SKILL.md")
            .and_then(|f| f["content"].as_str())
            .map(decode_text)
            .unwrap_or_default();
        let paths: Vec<&str> = files.iter().filter_map(|f| f["path"].as_str()).collect();
        ok_json(&json!({
            "name": bundle["name"],
            "description": bundle["description"],
            "kind": bundle["kind"],
            "commit": bundle["commit"],
            "skill_md": skill_md,
            "files": paths,
        }))
    }

    #[tool(description = "Read one bundled file's content from a skill (execution) — e.g. a script or reference.")]
    async fn read_skill_file(
        &self,
        Parameters(p): Parameters<FileRef>,
    ) -> Result<CallToolResult, ErrorData> {
        let skill = self.resolve(&p.skill).await?;
        let id = skill["id"].as_str().ok_or_else(|| oops("skill has no id"))?;
        let bundle = self.get(&format!("/skill/{id}/bundle")).await?;
        let empty = vec![];
        let content = bundle["files"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .find(|f| f["path"].as_str() == Some(p.path.as_str()))
            .and_then(|f| f["content"].as_str())
            .map(decode_text)
            .ok_or_else(|| oops(format!("no file {} in skill", p.path)))?;
        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Fetch this service's published config for a namespace from the config center (already merged with the shared _global layer). Requires SKILL_SHELF_CONFIG_TOKEN. Returns a flat key→value JSON — use it instead of reading environment variables.")]
    async fn get_config(
        &self,
        Parameters(p): Parameters<ConfigParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let ns = p.namespace.unwrap_or_else(|| "_global".into());
        ok_json(&self.resolve_config(&ns).await?)
    }

    #[tool(description = "Give feedback on a skill after using it (-1/0/+1 + note). Closes the improvement loop.")]
    async fn submit_feedback(
        &self,
        Parameters(p): Parameters<FeedbackParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let skill = self.resolve(&p.skill).await?;
        let id = skill["id"].as_str().ok_or_else(|| oops("skill has no id"))?;
        let body = json!({
            "source": "agent",
            "rating": p.rating,
            "content": p.content,
            "query": p.query,
        });
        ok_json(&self.post(&format!("/skill/{id}/feedback"), body).await?)
    }
}

#[tool_handler]
impl ServerHandler for Shelf {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Skill Shelf: route to skills for a need, fetch/load them, give feedback, and \
                 fetch service config from the config center (get_config)."
                    .into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "skill-shelf".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

fn decode_text(b64: &str) -> String {
    STANDARD
        .decode(b64)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| "<binary>".to_string())
}

/// True when `base` is safe to send a credential over: https, or a loopback
/// host (localhost / 127.0.0.0/8 / ::1) where cleartext http stays on the box.
fn is_loopback_or_https(base: &str) -> bool {
    if base.starts_with("https://") {
        return true;
    }
    let host = base.strip_prefix("http://").unwrap_or(base);
    let host = host.split(['/', ':']).next().unwrap_or("");
    host == "localhost" || host == "::1" || host == "[::1]" || host.starts_with("127.")
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = Shelf::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
