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
//!    config from here instead of reading their own env. Each namespace is
//!    version-controlled: edits land in a `draft`, and consumers only ever see
//!    the latest *published* version. A reserved `_global` namespace holds
//!    shared defaults; each service/environment namespace layers overrides on
//!    top. Consumers authenticate with a service token and receive the merged
//!    *published* values in plaintext via `resolve`. The self settings in
//!    `vars` are never exposed to consumers — the two roles are isolated.
//!
//! Everything is persisted to `DATA_DIR/config.json`.

use std::collections::{BTreeMap, BTreeSet};
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
    /// Invalid input (locked key, bad namespace/name, nothing to publish) → 400.
    BadRequest(String),
    /// Target does not exist (e.g. unknown client / version) → 404.
    NotFound(String),
    /// Persistence / IO failure → 500. The in-memory state is left unchanged.
    Internal(String),
}

/// One published version of a namespace's config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVersion {
    pub version: u64,
    pub vars: BTreeMap<String, Value>,
    pub published_at: u64,
    pub author: String,
    pub note: String,
}

/// A version-controlled namespace: a `draft` being edited plus the history of
/// published versions. Consumers see `versions.last()`.
///
/// Note: no `deny_unknown_fields` — an older binary must tolerate a newer
/// on-disk field rather than fail the whole load (see `RuntimeConfig::load`).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    #[serde(default)]
    pub draft: BTreeMap<String, Value>,
    #[serde(default)]
    pub versions: Vec<ConfigVersion>,
}

impl NamespaceConfig {
    /// Wrap a bare key→value map (the pre-versioning on-disk format) as an
    /// already-published v1 with a matching draft.
    fn from_published(map: BTreeMap<String, Value>) -> Self {
        Self {
            draft: map.clone(),
            versions: vec![ConfigVersion {
                version: 1,
                vars: map,
                published_at: 0,
                author: String::new(),
                note: "imported".into(),
            }],
        }
    }

    fn published_ref(&self) -> Option<&BTreeMap<String, Value>> {
        self.versions.last().map(|v| &v.vars)
    }

    /// The published values consumers see (empty until first publish).
    fn published(&self) -> BTreeMap<String, Value> {
        self.published_ref().cloned().unwrap_or_default()
    }

    fn current_version(&self) -> u64 {
        self.versions.last().map(|v| v.version).unwrap_or(0)
    }

    /// True when the draft differs from the latest published version.
    fn dirty(&self) -> bool {
        match self.published_ref() {
            Some(p) => &self.draft != p,
            None => !self.draft.is_empty(),
        }
    }
}

/// Backward-compatible deserialization: accept either the new `NamespaceConfig`
/// shape or the old bare `{key: value}` map (migrated to a published v1).
///
/// The `versions` key is the discriminator: a `NamespaceConfig` is *always*
/// serialized with a `versions` field (even `[]`), so its presence marks the
/// new format. Any object without it is treated as an old bare map — including
/// one whose keys happen to be named `draft`, which would otherwise be
/// misread. (A legacy config key literally named `versions` is the one
/// unsupported corner; `load` fails loudly rather than silently in that case.)
fn de_namespaces<'de, D>(d: D) -> Result<BTreeMap<String, NamespaceConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let raw: BTreeMap<String, Value> = BTreeMap::deserialize(d)?;
    let mut out = BTreeMap::new();
    for (k, v) in raw {
        let nc = if v.get("versions").is_some() {
            serde_json::from_value::<NamespaceConfig>(v).map_err(D::Error::custom)?
        } else {
            let map: BTreeMap<String, Value> = serde_json::from_value(v).map_err(D::Error::custom)?;
            NamespaceConfig::from_published(map)
        };
        out.insert(k, nc);
    }
    Ok(out)
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
    /// Config center: namespace name → versioned config. `_global` is shared base.
    #[serde(default, deserialize_with = "de_namespaces")]
    pub namespaces: BTreeMap<String, NamespaceConfig>,
    /// Config center: client id → service-token grant.
    #[serde(default)]
    pub clients: BTreeMap<String, ClientGrant>,
}

impl RuntimeConfig {
    /// Load persisted config. A missing file is a fresh start (default). A file
    /// that exists but fails to read/parse is a hard error: we panic (refusing
    /// to start) rather than silently returning an empty config that the next
    /// write would persist over the real data.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("config.json");
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                panic!(
                    "failed to parse {}: {e}\n\
                     Refusing to start so the existing config is not overwritten. \
                     Fix or move the file, then restart.",
                    path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => panic!("failed to read {}: {e}", path.display()),
        }
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

    /// Merge the `_global` layer with namespace `ns` (ns wins), using each
    /// namespace's *published* version. Returns owned plaintext values — this
    /// is what a consumer receives.
    fn resolve_merged(&self, ns: &str) -> BTreeMap<String, Value> {
        let mut out = self.namespaces.get(GLOBAL_NS).map(|n| n.published()).unwrap_or_default();
        if ns != GLOBAL_NS {
            if let Some(layer) = self.namespaces.get(ns) {
                for (k, v) in layer.published() {
                    out.insert(k, v);
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

/// A namespace's DRAFT keys for the admin UI (secrets masked), plus its
/// publish state.
#[derive(Serialize)]
pub struct NamespaceView {
    pub namespace: String,
    pub vars: Vec<VarView>,
    /// Latest published version number (0 = never published).
    pub version: u64,
    /// True when the draft has unpublished changes.
    pub dirty: bool,
}

/// Summary of a namespace in the list.
#[derive(Serialize)]
pub struct NamespaceInfo {
    pub name: String,
    /// Number of draft keys.
    pub keys: usize,
    pub version: u64,
    pub dirty: bool,
}

/// Metadata for one published version (no values).
#[derive(Serialize)]
pub struct VersionInfo {
    pub version: u64,
    pub published_at: u64,
    pub author: String,
    pub note: String,
    pub keys: usize,
}

/// One field-level change between two states of a namespace.
#[derive(Serialize)]
pub struct FieldChange {
    pub key: String,
    /// added | removed | modified
    pub status: &'static str,
    pub secret: bool,
    /// Old/new values — omitted for secrets (only the fact of a change shows).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new: Option<Value>,
}

/// Field-level diff between the published version and the draft.
#[derive(Serialize)]
pub struct ConfigDiff {
    pub namespace: String,
    pub changes: Vec<FieldChange>,
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
        let mut keys: BTreeSet<String> = cfg.vars.keys().cloned().collect();
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
        let mut names: BTreeSet<String> = cfg.namespaces.keys().cloned().collect();
        names.insert(GLOBAL_NS.to_string());
        names
            .into_iter()
            .map(|name| {
                let nc = cfg.namespaces.get(&name);
                NamespaceInfo {
                    keys: nc.map(|n| n.draft.len()).unwrap_or(0),
                    version: nc.map(|n| n.current_version()).unwrap_or(0),
                    dirty: nc.map(|n| n.dirty()).unwrap_or(false),
                    name,
                }
            })
            .collect()
    }

    /// A namespace's DRAFT KV (secrets masked) plus publish state.
    pub fn namespace_view(&self, ns: &str) -> NamespaceView {
        let cfg = self.inner.read().unwrap();
        let nc = cfg.namespaces.get(ns);
        NamespaceView {
            namespace: ns.to_string(),
            vars: nc.map(|n| mask_vars(&n.draft)).unwrap_or_default(),
            version: nc.map(|n| n.current_version()).unwrap_or(0),
            dirty: nc.map(|n| n.dirty()).unwrap_or(false),
        }
    }

    /// Merge a patch into a namespace's DRAFT (null value deletes a key). Does
    /// NOT affect what consumers resolve until `publish_namespace`.
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
            let entry = next.namespaces.entry(ns.to_string()).or_default();
            for (k, v) in patch {
                if v.is_null() {
                    entry.draft.remove(&k);
                } else {
                    entry.draft.insert(k, v);
                }
            }
            // Drop a namespace that has neither a draft nor any published history.
            if entry.draft.is_empty() && entry.versions.is_empty() && ns != GLOBAL_NS {
                next.namespaces.remove(ns);
            }
            self.persist(&next)?;
            *g = next;
        }
        Ok(self.namespace_view(ns))
    }

    /// Publish a namespace's draft as a new version (what consumers then see).
    /// Rejected when the draft matches the latest published version.
    pub fn publish_namespace(
        &self,
        ns: &str,
        author: &str,
        note: &str,
    ) -> Result<NamespaceView, ConfigError> {
        if !valid_namespace(ns) {
            return Err(ConfigError::BadRequest(format!("invalid namespace name: {ns:?}")));
        }
        {
            let mut g = self.inner.write().unwrap();
            let mut next = g.clone();
            let entry = next.namespaces.entry(ns.to_string()).or_default();
            if !entry.dirty() {
                return Err(ConfigError::BadRequest("no unpublished changes to publish".into()));
            }
            let version = entry.current_version() + 1;
            entry.versions.push(ConfigVersion {
                version,
                vars: entry.draft.clone(),
                published_at: now_secs(),
                author: author.to_string(),
                note: note.to_string(),
            });
            self.persist(&next)?;
            *g = next;
        }
        Ok(self.namespace_view(ns))
    }

    /// Published version history (metadata only), newest first.
    pub fn list_versions(&self, ns: &str) -> Vec<VersionInfo> {
        let cfg = self.inner.read().unwrap();
        let Some(nc) = cfg.namespaces.get(ns) else { return Vec::new() };
        nc.versions
            .iter()
            .rev()
            .map(|v| VersionInfo {
                version: v.version,
                published_at: v.published_at,
                author: v.author.clone(),
                note: v.note.clone(),
                keys: v.vars.len(),
            })
            .collect()
    }

    /// One published version's KV (secrets masked).
    pub fn version_view(&self, ns: &str, version: u64) -> Result<Vec<VarView>, ConfigError> {
        let cfg = self.inner.read().unwrap();
        let nc = cfg.namespaces.get(ns).ok_or_else(|| ConfigError::NotFound(format!("no such namespace: {ns}")))?;
        let v = nc
            .versions
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| ConfigError::NotFound(format!("no version {version} in {ns}")))?;
        Ok(mask_vars(&v.vars))
    }

    /// Load a published version's values back into the draft (does NOT publish).
    /// The admin reviews and then publishes to make it live.
    pub fn rollback_namespace(&self, ns: &str, version: u64) -> Result<NamespaceView, ConfigError> {
        {
            let mut g = self.inner.write().unwrap();
            let mut next = g.clone();
            let entry = next
                .namespaces
                .get_mut(ns)
                .ok_or_else(|| ConfigError::NotFound(format!("no such namespace: {ns}")))?;
            let vars = entry
                .versions
                .iter()
                .find(|v| v.version == version)
                .ok_or_else(|| ConfigError::NotFound(format!("no version {version} in {ns}")))?
                .vars
                .clone();
            entry.draft = vars;
            self.persist(&next)?;
            *g = next;
        }
        Ok(self.namespace_view(ns))
    }

    /// Field-level diff of the draft against the latest published version.
    /// Only changed keys are returned (added / removed / modified).
    pub fn diff_namespace(&self, ns: &str) -> ConfigDiff {
        let cfg = self.inner.read().unwrap();
        let nc = cfg.namespaces.get(ns);
        let old = nc.map(|n| n.published()).unwrap_or_default();
        let new = nc.map(|n| n.draft.clone()).unwrap_or_default();
        let mut keys: BTreeSet<&String> = old.keys().collect();
        keys.extend(new.keys());
        let mut changes = Vec::new();
        for key in keys {
            let o = old.get(key);
            let n = new.get(key);
            let status = match (o, n) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(a), Some(b)) if a != b => "modified",
                _ => continue, // unchanged
            };
            let secret = is_secret(key);
            changes.push(FieldChange {
                key: key.clone(),
                status,
                secret,
                old: if secret { None } else { o.cloned() },
                new: if secret { None } else { n.cloned() },
            });
        }
        ConfigDiff { namespace: ns.to_string(), changes }
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

    /// Authenticate a service token and return the merged *published* config for
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_old_bare_map_namespace() {
        // Old on-disk format: namespace value is a bare {key: value} map.
        let json = r#"{"namespaces": {"svc": {"DB_URL": "x", "N": 5}}}"#;
        let cfg: RuntimeConfig = serde_json::from_str(json).unwrap();
        let svc = cfg.namespaces.get("svc").expect("svc migrated");
        // Migrated to published v1 with a matching draft — no data lost.
        assert_eq!(svc.current_version(), 1);
        assert_eq!(svc.published().get("DB_URL"), Some(&Value::from("x")));
        assert_eq!(svc.draft.get("N"), Some(&Value::from(5)));
        assert!(!svc.dirty());
    }

    #[test]
    fn new_format_roundtrips() {
        let json = r#"{"namespaces": {"svc": {"draft": {"A": "1"}, "versions": []}}}"#;
        let cfg: RuntimeConfig = serde_json::from_str(json).unwrap();
        let svc = cfg.namespaces.get("svc").unwrap();
        assert_eq!(svc.current_version(), 0);
        assert!(svc.dirty()); // draft non-empty, nothing published
    }

    #[test]
    fn old_map_with_reserved_key_name_not_misread() {
        // An OLD bare map that happens to have a key literally named "draft"
        // must migrate as an old map (published v1), not be mistaken for the
        // new struct — the "versions" marker is absent.
        let json = r#"{"namespaces": {"svc": {"draft": "some-value", "OTHER": 2}}}"#;
        let cfg: RuntimeConfig = serde_json::from_str(json).unwrap();
        let svc = cfg.namespaces.get("svc").unwrap();
        assert_eq!(svc.current_version(), 1);
        assert_eq!(svc.published().get("draft"), Some(&Value::from("some-value")));
        assert_eq!(svc.published().get("OTHER"), Some(&Value::from(2)));
    }

    #[test]
    fn new_format_tolerates_unknown_future_field() {
        // Forward compat: an older binary reading a file written by a newer one
        // (extra field) must not fail the whole load.
        let json = r#"{"namespaces": {"svc": {"draft": {}, "versions": [], "future": true}}}"#;
        let cfg: RuntimeConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.namespaces.contains_key("svc"));
    }

    #[test]
    fn full_config_roundtrips_through_save_format() {
        // A published namespace serializes with "versions" and reloads identically.
        let json = r#"{"namespaces": {"svc": {"draft": {"A": "2"}, "versions": [
            {"version": 1, "vars": {"A": "1"}, "published_at": 5, "author": "me", "note": "init"}
        ]}}}"#;
        let cfg: RuntimeConfig = serde_json::from_str(json).unwrap();
        let reserialized = serde_json::to_vec(&cfg).unwrap();
        let cfg2: RuntimeConfig = serde_json::from_slice(&reserialized).unwrap();
        let svc = cfg2.namespaces.get("svc").unwrap();
        assert_eq!(svc.current_version(), 1);
        assert_eq!(svc.published().get("A"), Some(&Value::from("1")));
        assert!(svc.dirty()); // draft A=2 differs from published A=1
    }
}
