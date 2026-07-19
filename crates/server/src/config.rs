//! Hot-reloadable runtime config, in two roles:
//!
//! 1. **Skill Shelf's own settings** (`vars`): a generic key→value store where
//!    each value is a string or arbitrary JSON, overriding process env at
//!    runtime (no restart). Drives this service's AI / GitHub integrations.
//!    Precedence for any key: stored value > process env var > default.
//!    Structural settings (PORT, DATA_DIR, DB, JWT_SECRET) are read only at
//!    startup and are intentionally NOT hot-reloadable.
//!
//! 2. **Config center** (`namespaces` + `clients`): other services fetch their
//!    config from here instead of reading their own env. A reserved `_global`
//!    namespace holds shared defaults; each service/environment namespace layers
//!    overrides on top. Consumers authenticate with a service token and receive
//!    the merged values *in plaintext* via `resolve`. The self settings in
//!    `vars` are never exposed to consumers — the two roles are isolated.
//!
//! Everything is persisted to `DATA_DIR/config.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Startup-only keys that must not be settable at runtime (self settings only).
const LOCKED_KEYS: &[&str] = &["PORT", "DATA_DIR", "DB", "JWT_SECRET"];

/// Reserved namespace holding shared defaults merged into every `resolve`.
pub const GLOBAL_NS: &str = "_global";

fn is_secret(key: &str) -> bool {
    let k = key.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD"].iter().any(|s| k.contains(s))
}

/// A JSON value rendered as a string (for use as an env-var value).
fn value_to_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// A random opaque id (hex). Used for client ids and token bodies.
fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    format!("{:x}", h.finalize())
}

/// Validate a namespace name: non-empty, no whitespace, and limited to a safe
/// charset (letters, digits, `-`, `_`, `/`, `.`).
pub fn valid_namespace(ns: &str) -> bool {
    !ns.is_empty()
        && ns.len() <= 128
        && !ns.contains("..") // no path-traversal-ish names, even though ns is only a map key today
        && ns.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

/// A config write error, mapped to the right HTTP status by the handler layer.
pub enum ConfigError {
    /// Invalid input (locked key, bad namespace/name) → 400.
    BadRequest(String),
    /// Target does not exist (e.g. unknown client) → 404.
    NotFound(String),
    /// Persistence / IO failure → 500. The in-memory state is left unchanged.
    Internal(String),
}

/// A service-token grant: which namespaces this token may read. The token
/// itself is stored only as a hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGrant {
    pub name: String,
    pub token_hash: String,
    /// Namespaces this token may resolve (the `_global` layer is always merged
    /// in regardless).
    pub namespaces: Vec<String>,
    pub created_at: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Skill Shelf's own settings (self AI / GitHub / custom). Never exposed to
    /// config-center consumers.
    #[serde(default)]
    pub vars: BTreeMap<String, Value>,
    /// Config center: namespace name → (key → value). `_global` is shared base.
    #[serde(default)]
    pub namespaces: BTreeMap<String, BTreeMap<String, Value>>,
    /// Config center: client id → service-token grant.
    #[serde(default)]
    pub clients: BTreeMap<String, ClientGrant>,
}

impl RuntimeConfig {
    pub fn load(data_dir: &Path) -> Self {
        std::fs::read(data_dir.join("config.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::write(data_dir.join("config.json"), serde_json::to_vec_pretty(self)?)
    }

    // ---- self settings (role 1) --------------------------------------------

    /// Effective string value for a self key: stored > env. Empty = unset.
    pub fn get_str(&self, key: &str) -> Option<String> {
        self.vars
            .get(key)
            .map(value_to_str)
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var(key).ok().filter(|s| !s.is_empty()))
    }

    pub fn ai(&self) -> Option<(String, String, String)> {
        Some((self.get_str("AI_BASE_URL")?, self.get_str("AI_API_KEY")?, self.get_str("AI_MODEL")?))
    }

    pub fn github_token(&self) -> Option<String> {
        self.get_str("GITHUB_TOKEN")
    }

    pub fn github_api_base(&self) -> String {
        self.get_str("GITHUB_API_BASE").unwrap_or_else(|| "https://api.github.com".to_string())
    }

    // ---- config center (role 2) --------------------------------------------

    /// Merge the `_global` layer with namespace `ns` (ns wins). Returns owned
    /// plaintext values — this is what a consumer receives.
    fn resolve_merged(&self, ns: &str) -> BTreeMap<String, Value> {
        let mut out = self.namespaces.get(GLOBAL_NS).cloned().unwrap_or_default();
        if ns != GLOBAL_NS {
            if let Some(layer) = self.namespaces.get(ns) {
                for (k, v) in layer {
                    out.insert(k.clone(), v.clone());
                }
            }
        }
        out
    }

    /// Find the client whose token hash matches, if any.
    fn client_for_token(&self, token: &str) -> Option<&ClientGrant> {
        let h = hash_token(token);
        self.clients.values().find(|c| c.token_hash == h)
    }
}

/// One entry for the admin UI (secret values are hidden, only `secret`+`set`).
#[derive(Serialize)]
pub struct VarView {
    pub key: String,
    pub secret: bool,
    /// true when a value is set (stored or via env).
    pub set: bool,
    pub is_json: bool,
    /// The value — omitted for secrets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// stored | env — where the effective value comes from.
    pub source: &'static str,
}

#[derive(Serialize)]
pub struct ConfigView {
    pub vars: Vec<VarView>,
    /// Whether AI (base+key+model) resolves — drives refine / smart routing.
    pub ai_ready: bool,
    /// Keys that cannot be changed at runtime.
    pub locked_keys: Vec<&'static str>,
}

/// A namespace's keys for the admin UI (secrets masked).
#[derive(Serialize)]
pub struct NamespaceView {
    pub namespace: String,
    pub vars: Vec<VarView>,
}

/// Summary of a namespace in the list.
#[derive(Serialize)]
pub struct NamespaceInfo {
    pub name: String,
    pub keys: usize,
}

/// A service-token client for the admin UI (never includes the token/hash).
#[derive(Serialize)]
pub struct ClientView {
    pub id: String,
    pub name: String,
    pub namespaces: Vec<String>,
    pub created_at: u64,
}

/// Result of creating a client: the plaintext token is shown exactly once.
#[derive(Serialize)]
pub struct NewClient {
    pub id: String,
    pub name: String,
    pub namespaces: Vec<String>,
    /// Plaintext token — only returned here, never stored or shown again.
    pub token: String,
}

/// Build masked VarViews from a raw key→value map (no env fallback).
fn mask_vars(map: &BTreeMap<String, Value>) -> Vec<VarView> {
    map.iter()
        .map(|(key, v)| {
            let secret = is_secret(key);
            VarView {
                source: "stored",
                set: true,
                is_json: !v.is_string(),
                value: if secret { None } else { Some(v.clone()) },
                secret,
                key: key.clone(),
            }
        })
        .collect()
}

/// Shared, hot-swappable config + where to persist it.
#[derive(Clone)]
pub struct ConfigStore {
    inner: Arc<RwLock<RuntimeConfig>>,
    data_dir: PathBuf,
}

impl ConfigStore {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { inner: Arc::new(RwLock::new(RuntimeConfig::load(&data_dir))), data_dir }
    }

    pub fn snapshot(&self) -> RuntimeConfig {
        self.inner.read().unwrap().clone()
    }

    fn persist(&self, cfg: &RuntimeConfig) -> Result<(), ConfigError> {
        cfg.save(&self.data_dir).map_err(|e| ConfigError::Internal(format!("persist failed: {e}")))
    }

    // ---- self settings view/update (role 1) --------------------------------

    pub fn view(&self) -> ConfigView {
        let cfg = self.inner.read().unwrap();
        let mut keys: std::collections::BTreeSet<String> = cfg.vars.keys().cloned().collect();
        // Surface the well-known keys even if only set via env.
        for k in ["AI_BASE_URL", "AI_API_KEY", "AI_MODEL", "GITHUB_TOKEN", "GITHUB_API_BASE"] {
            keys.insert(k.to_string());
        }
        let vars = keys
            .into_iter()
            .map(|key| {
                let stored = cfg.vars.get(&key);
                let effective = cfg.get_str(&key);
                let secret = is_secret(&key);
                VarView {
                    source: if stored.is_some() { "stored" } else { "env" },
                    set: effective.is_some(),
                    is_json: stored.map(|v| !v.is_string()).unwrap_or(false),
                    value: if secret { None } else { stored.cloned() },
                    secret,
                    key,
                }
            })
            .filter(|v| v.set || !is_secret(&v.key) || v.source == "stored")
            .collect();
        ConfigView { vars, ai_ready: cfg.ai().is_some(), locked_keys: LOCKED_KEYS.to_vec() }
    }

    /// Merge a patch into self settings (null value deletes). Rejects locked keys.
    /// Atomic: changes are persisted first, then committed to memory, so a disk
    /// failure leaves the running config untouched.
    pub fn update(&self, patch: BTreeMap<String, Value>) -> Result<ConfigView, ConfigError> {
        for k in patch.keys() {
            if LOCKED_KEYS.iter().any(|l| l.eq_ignore_ascii_case(k)) {
                return Err(ConfigError::BadRequest(format!(
                    "{k} is a startup-only setting and cannot be changed at runtime"
                )));
            }
        }
        {
            let mut g = self.inner.write().unwrap();
            let mut next = g.clone();
            for (k, v) in patch {
                if v.is_null() {
                    next.vars.remove(&k);
                } else {
                    next.vars.insert(k, v);
                }
            }
            self.persist(&next)?;
            *g = next;
        }
        Ok(self.view())
    }

    // ---- config center: namespaces (role 2) --------------------------------

    /// All namespaces (the reserved `_global` is always listed).
    pub fn list_namespaces(&self) -> Vec<NamespaceInfo> {
        let cfg = self.inner.read().unwrap();
        let mut names: std::collections::BTreeSet<String> = cfg.namespaces.keys().cloned().collect();
        names.insert(GLOBAL_NS.to_string());
        names
            .into_iter()
            .map(|name| {
                let keys = cfg.namespaces.get(&name).map(|m| m.len()).unwrap_or(0);
                NamespaceInfo { name, keys }
            })
            .collect()
    }

    /// A namespace's stored KV, secrets masked.
    pub fn namespace_view(&self, ns: &str) -> NamespaceView {
        let cfg = self.inner.read().unwrap();
        let vars = cfg.namespaces.get(ns).map(mask_vars).unwrap_or_default();
        NamespaceView { namespace: ns.to_string(), vars }
    }

    /// Merge a patch into namespace `ns` (null value deletes a key). Removes the
    /// namespace entirely once empty (except `_global`, which is kept).
    pub fn update_namespace(
        &self,
        ns: &str,
        patch: BTreeMap<String, Value>,
    ) -> Result<NamespaceView, ConfigError> {
        if !valid_namespace(ns) {
            return Err(ConfigError::BadRequest(format!("invalid namespace name: {ns:?}")));
        }
        {
            let mut g = self.inner.write().unwrap();
            let mut next = g.clone();
            let layer = next.namespaces.entry(ns.to_string()).or_default();
            for (k, v) in patch {
                if v.is_null() {
                    layer.remove(&k);
                } else {
                    layer.insert(k, v);
                }
            }
            if layer.is_empty() && ns != GLOBAL_NS {
                next.namespaces.remove(ns);
            }
            self.persist(&next)?;
            *g = next;
        }
        Ok(self.namespace_view(ns))
    }

    // ---- config center: clients / resolve (role 2) -------------------------

    pub fn list_clients(&self) -> Vec<ClientView> {
        let cfg = self.inner.read().unwrap();
        cfg.clients
            .iter()
            .map(|(id, c)| ClientView {
                id: id.clone(),
                name: c.name.clone(),
                namespaces: c.namespaces.clone(),
                created_at: c.created_at,
            })
            .collect()
    }

    /// Create a client and return its one-time plaintext token.
    pub fn create_client(&self, name: &str, namespaces: Vec<String>) -> Result<NewClient, ConfigError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ConfigError::BadRequest("client name is required".into()));
        }
        for ns in &namespaces {
            if !valid_namespace(ns) {
                return Err(ConfigError::BadRequest(format!("invalid namespace name: {ns:?}")));
            }
        }
        let id = random_hex(6);
        let token = format!("shelf_{}", random_hex(24));
        let grant = ClientGrant {
            name: name.to_string(),
            token_hash: hash_token(&token),
            namespaces: namespaces.clone(),
            created_at: now_secs(),
        };
        {
            let mut g = self.inner.write().unwrap();
            let mut next = g.clone();
            next.clients.insert(id.clone(), grant);
            self.persist(&next)?;
            *g = next;
        }
        Ok(NewClient { id, name: name.to_string(), namespaces, token })
    }

    pub fn delete_client(&self, id: &str) -> Result<(), ConfigError> {
        let mut g = self.inner.write().unwrap();
        if !g.clients.contains_key(id) {
            return Err(ConfigError::NotFound(format!("no such client: {id}")));
        }
        let mut next = g.clone();
        next.clients.remove(id);
        self.persist(&next)?;
        *g = next;
        Ok(())
    }

    /// Authenticate a service token and return the merged plaintext config for
    /// `ns`. Errors distinguish "bad token" from "not allowed this namespace".
    pub fn resolve(&self, token: &str, ns: &str) -> Result<BTreeMap<String, Value>, ResolveError> {
        let cfg = self.inner.read().unwrap();
        let grant = cfg.client_for_token(token).ok_or(ResolveError::Unauthorized)?;
        let allowed = ns == GLOBAL_NS || grant.namespaces.iter().any(|n| n == ns);
        if !allowed {
            return Err(ResolveError::Forbidden);
        }
        Ok(cfg.resolve_merged(ns))
    }
}

/// Why a `resolve` was rejected.
pub enum ResolveError {
    /// Token missing or unrecognized.
    Unauthorized,
    /// Valid token, but not granted this namespace.
    Forbidden,
}
