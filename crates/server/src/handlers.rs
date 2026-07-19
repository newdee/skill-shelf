use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use skill_shelf_core::{
    skillmd, Branch, Commit, Feedback, FileDiff, FileInput, RouteResult, Shelf, Skill, SkillMeta,
    TreeEntry, DEFAULT_BRANCH, KIND_SKILL,
};

use crate::ai;
use crate::config::{
    valid_namespace, ClientView, ConfigDiff, ConfigStore, ConfigView, NamespaceInfo, NamespaceView,
    NewClient, ResolveError, VarView, VersionInfo, GLOBAL_NS,
};
use crate::error::{ApiError, ApiResult};

const REFINE_BRANCH: &str = "refine";

/// Shared application state. The core [`Shelf`] is synchronous and holds a
/// SQLite connection (not `Sync`), so it lives behind a mutex; handlers lock,
/// run the quick op, and drop the guard before returning (never across await).
#[derive(Clone)]
pub struct AppState {
    pub shelf: Arc<Mutex<Shelf>>,
    /// JWT secret; `Some` enables auth (from `JWT_SECRET`), `None` = open.
    auth_secret: Option<Arc<String>>,
    config: ConfigStore,
}

impl AppState {
    pub fn new(shelf: Shelf, config: ConfigStore) -> Self {
        let auth_secret = std::env::var("JWT_SECRET")
            .ok()
            .filter(|s| !s.is_empty())
            .map(Arc::new);
        Self {
            shelf: Arc::new(Mutex::new(shelf)),
            auth_secret,
            config,
        }
    }

    fn shelf(&self) -> MutexGuard<'_, Shelf> {
        // Degrade gracefully rather than cascade-panicking if a prior request
        // panicked while holding the guard.
        self.shelf.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn auth_secret(&self) -> Option<Arc<String>> {
        self.auth_secret.clone()
    }

    pub fn config(&self) -> &ConfigStore {
        &self.config
    }
}

// --------------------------------------------------------------------- status

pub async fn status(State(st): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "skill-shelf-server",
        "version": env!("CARGO_PKG_VERSION"),
        // When true, mutating actions require an admin JWT.
        "auth_enabled": st.auth_secret().is_some(),
    }))
}

// --------------------------------------------------------------------- skills

#[derive(Deserialize)]
pub struct CreateSkillReq {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// "skill" (default) | "prompt"
    pub kind: Option<String>,
}

pub async fn create_skill(
    State(st): State<AppState>,
    Json(req): Json<CreateSkillReq>,
) -> ApiResult<Json<Skill>> {
    let kind = req.kind.as_deref().unwrap_or(KIND_SKILL);
    // A skill's name must be spec-valid up front (prompts are freeform).
    if kind == KIND_SKILL {
        skillmd::validate_name(&req.name)?;
    }
    Ok(Json(
        st.shelf().create_skill_with_kind(&req.name, &req.description, kind)?,
    ))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_skills(
    State(st): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Skill>>> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    Ok(Json(st.shelf().search_skills(
        q.q.as_deref(),
        q.kind.as_deref(),
        limit,
        offset,
    )?))
}

pub async fn get_skill(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Skill>> {
    Ok(Json(st.shelf().get_skill(&id)?))
}

/// Exact retrieval by name (precise mode).
pub async fn get_skill_by_name(
    State(st): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<Json<Skill>> {
    Ok(Json(st.shelf().get_skill_by_name(&name)?))
}

pub async fn delete_skill(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    st.shelf().delete_skill(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------- branches

pub async fn list_branches(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Branch>>> {
    Ok(Json(st.shelf().list_branches(&id)?))
}

#[derive(Deserialize)]
pub struct CreateBranchReq {
    pub name: String,
    pub from: Option<String>,
}

pub async fn create_branch(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateBranchReq>,
) -> ApiResult<Json<Branch>> {
    Ok(Json(
        st.shelf().create_branch(&id, &req.name, req.from.as_deref())?,
    ))
}

// -------------------------------------------------------------------- commits

/// A file in a commit request/response. `content` is base64 so binary skill
/// assets round-trip safely.
#[derive(Deserialize, Serialize)]
pub struct FilePayload {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct CommitReq {
    pub branch: Option<String>,
    pub author: String,
    pub message: String,
    pub files: Vec<FilePayload>,
}

pub async fn commit(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CommitReq>,
) -> ApiResult<Json<Commit>> {
    let branch = req.branch.as_deref().unwrap_or(DEFAULT_BRANCH);
    let files = decode_files(&req.files)?;

    let shelf = st.shelf();
    let skill = shelf.get_skill(&id)?;

    // Spec enforcement: every skill-kind version must carry a valid SKILL.md
    // whose name matches this skill.
    let meta = if skill.kind == KIND_SKILL {
        let m = validate_skill_files(&files, &skill.name)?;
        Some(m)
    } else {
        None
    };

    let inputs: Vec<FileInput> = files
        .into_iter()
        .map(|(path, content)| FileInput { path, content })
        .collect();
    let commit = shelf.commit(&id, branch, &inputs, &req.author, &req.message)?;
    if let Some(meta) = meta {
        shelf.set_skill_meta(&id, &meta)?; // refresh cached frontmatter + routing
    }
    Ok(Json(commit))
}

#[derive(Deserialize)]
pub struct BranchQuery {
    pub branch: Option<String>,
}

pub async fn list_commits(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<BranchQuery>,
) -> ApiResult<Json<Vec<Commit>>> {
    let branch = q.branch.as_deref().unwrap_or(DEFAULT_BRANCH);
    Ok(Json(st.shelf().list_commits(&id, branch)?))
}

#[derive(Deserialize)]
pub struct RollbackReq {
    pub branch: Option<String>,
    pub to_commit: String,
    pub author: String,
}

pub async fn rollback(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RollbackReq>,
) -> ApiResult<Json<Commit>> {
    let branch = req.branch.as_deref().unwrap_or(DEFAULT_BRANCH);
    Ok(Json(
        st.shelf().rollback(&id, branch, &req.to_commit, &req.author)?,
    ))
}

// ----------------------------------------------------------- commit contents

pub async fn get_tree(
    State(st): State<AppState>,
    Path(cid): Path<String>,
) -> ApiResult<Json<Vec<TreeEntry>>> {
    Ok(Json(st.shelf().read_tree(&cid)?))
}

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: String,
}

pub async fn get_file(
    State(st): State<AppState>,
    Path(cid): Path<String>,
    Query(q): Query<FileQuery>,
) -> ApiResult<Json<FilePayload>> {
    let bytes = st.shelf().read_file(&cid, &q.path)?;
    Ok(Json(FilePayload {
        path: q.path,
        content: STANDARD.encode(bytes),
    }))
}

#[derive(Deserialize)]
pub struct DiffQuery {
    pub a: String,
    pub b: String,
}

pub async fn diff(
    State(st): State<AppState>,
    Query(q): Query<DiffQuery>,
) -> ApiResult<Json<Vec<FileDiff>>> {
    Ok(Json(st.shelf().diff(&q.a, &q.b)?))
}

// ---------------------------------------------------------------------- route

#[derive(Deserialize)]
pub struct RouteReq {
    pub query: String,
    pub top_k: Option<usize>,
    /// "fuzzy" (default, ranked) | "exact" (match a skill by name) | "smart" (BM25 + LLM rerank)
    pub mode: Option<String>,
    /// Enable LLM rerank over BM25 candidates (same as mode "smart").
    pub rerank: Option<bool>,
}

/// Small registries skip recall and rerank ALL skills (100% recall).
const RERANK_ALL_THRESHOLD: usize = 15;
/// BM25 candidate pool size for larger registries before rerank.
const RERANK_RECALL_N: usize = 20;

pub async fn route(
    State(st): State<AppState>,
    Json(req): Json<RouteReq>,
) -> ApiResult<Json<Vec<RouteResult>>> {
    let top_k = req.top_k.unwrap_or(5);

    if req.mode.as_deref() == Some("exact") {
        // Precise: resolve by name, no ranking. Empty list if not found.
        let shelf = st.shelf();
        return Ok(Json(match shelf.get_skill_by_name(req.query.trim()) {
            Ok(s) => vec![RouteResult {
                skill_id: s.id,
                name: s.name,
                description: s.description,
                score: 1.0,
            }],
            Err(_) => vec![],
        }));
    }

    let want_rerank = req.rerank.unwrap_or(false) || req.mode.as_deref() == Some("smart");
    if !want_rerank {
        // Plain BM25.
        let shelf = st.shelf();
        return Ok(Json(shelf.route(&req.query, top_k)?));
    }
    if top_k == 0 {
        return Ok(Json(vec![]));
    }

    // Gather candidates under the lock (small registry → all; else BM25 recall).
    let candidates: Vec<(String, String, String)> = {
        let shelf = st.shelf();
        let all = shelf.search_skills(None, None, (RERANK_ALL_THRESHOLD + 1) as i64, 0)?;
        if all.len() <= RERANK_ALL_THRESHOLD {
            all.into_iter().map(|s| (s.id, s.name, s.description)).collect()
        } else {
            shelf
                .route(&req.query, RERANK_RECALL_N)?
                .into_iter()
                .map(|r| (r.skill_id, r.name, r.description))
                .collect()
        }
    };
    if candidates.is_empty() {
        return Ok(Json(vec![]));
    }

    // Resolve AI config; if unset, degrade to BM25 (routing never hard-fails).
    let Some(ai_cfg) = st.config().snapshot().ai() else {
        let shelf = st.shelf();
        return Ok(Json(shelf.route(&req.query, top_k)?));
    };

    // LLM rerank (no lock held). On any failure — network, bad output — degrade
    // gracefully to BM25.
    match ai::rerank(&ai_cfg, &req.query, &candidates).await {
        Ok(ids) => {
            let by_id: HashMap<&str, &(String, String, String)> =
                candidates.iter().map(|c| (c.0.as_str(), c)).collect();
            let n = ids.len().max(1);
            let mut out = Vec::new();
            for (i, id) in ids.iter().enumerate() {
                if let Some(c) = by_id.get(id.as_str()) {
                    out.push(RouteResult {
                        skill_id: c.0.clone(),
                        name: c.1.clone(),
                        description: c.2.clone(),
                        score: (n - i) as f64 / n as f64,
                    });
                }
                if out.len() >= top_k {
                    break;
                }
            }
            Ok(Json(out))
        }
        Err(_) => {
            let shelf = st.shelf();
            Ok(Json(shelf.route(&req.query, top_k)?))
        }
    }
}

/// A skill packaged for an agent to load (runtime consumption). Files carry the
/// full SKILL.md + bundled scripts/references/assets; `content` is base64.
#[derive(Serialize)]
pub struct SkillBundle {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub commit: String,
    pub files: Vec<FilePayload>,
}

pub async fn get_bundle(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Json<SkillBundle>> {
    let shelf = st.shelf();
    let skill = shelf.get_skill(&id)?;
    let commit = match q.commit {
        Some(c) => c,
        None => {
            let branch = q.branch.as_deref().unwrap_or(DEFAULT_BRANCH);
            shelf
                .get_branch(&id, branch)?
                .head
                .ok_or_else(|| ApiError::bad_request("no commits yet"))?
        }
    };
    let mut files = Vec::new();
    for e in shelf.read_tree(&commit)? {
        let bytes = shelf.read_file(&commit, &e.path)?;
        files.push(FilePayload {
            path: e.path,
            content: STANDARD.encode(bytes),
        });
    }
    Ok(Json(SkillBundle {
        id: skill.id,
        name: skill.name,
        description: skill.description,
        kind: skill.kind,
        commit,
        files,
    }))
}

// ------------------------------------------------------------- import/export

#[derive(Deserialize)]
pub struct ExportQuery {
    /// Export this exact commit; if absent, use `branch`'s head.
    pub commit: Option<String>,
    pub branch: Option<String>,
}

/// Export a skill version as a `.zip` of its files.
pub async fn export_zip(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let shelf = st.shelf();
    let skill = shelf.get_skill(&id)?;
    let commit_id = match q.commit {
        Some(c) => c,
        None => {
            let branch = q.branch.as_deref().unwrap_or(DEFAULT_BRANCH);
            shelf
                .get_branch(&id, branch)?
                .head
                .ok_or_else(|| ApiError::bad_request("branch has no commits yet"))?
        }
    };

    let entries = shelf.read_tree(&commit_id)?;
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
        for entry in &entries {
            let data = shelf.read_file(&commit_id, &entry.path)?;
            zip.start_file(&entry.path, opts)
                .map_err(|e| ApiError::internal(format!("zip: {e}")))?;
            zip.write_all(&data)
                .map_err(|e| ApiError::internal(format!("zip: {e}")))?;
        }
        zip.finish()
            .map_err(|e| ApiError::internal(format!("zip: {e}")))?;
    }
    drop(shelf);

    let filename = format!("{}.zip", sanitize_filename(&skill.name));
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        buf,
    )
        .into_response())
}

/// Import a `.zip` as a new skill: unzip, read SKILL.md frontmatter for
/// name/description, then create the skill and commit its files.
pub async fn import_zip(State(st): State<AppState>, body: Bytes) -> ApiResult<Json<Skill>> {
    let mut files = unzip_bytes(&body)?;
    if files.is_empty() {
        return Err(ApiError::bad_request("zip contains no files"));
    }
    strip_common_prefix(&mut files);
    let shelf = st.shelf();
    Ok(Json(import_files_as_skill(&shelf, files, "import from zip")?))
}

/// Unzip bytes into (path, content) pairs. `enclosed_name()` rejects absolute
/// paths and `..` traversal; directories are skipped.
fn unzip_bytes(bytes: &[u8]) -> ApiResult<Vec<(String, Vec<u8>)>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| ApiError::bad_request(format!("not a valid zip: {e}")))?;
    let mut files = Vec::new();
    let mut total = 0usize;
    for i in 0..archive.len() {
        let mut f = archive
            .by_index(i)
            .map_err(|e| ApiError::bad_request(format!("zip: {e}")))?;
        if f.is_dir() {
            continue;
        }
        let Some(path) = f.enclosed_name() else { continue };
        let path = path.to_string_lossy().replace('\\', "/");
        // Cap total decompressed size (zip-bomb guard); take() bounds memory
        // even for a single huge entry.
        let remaining = MAX_UNZIP_BYTES - total;
        let mut content = Vec::new();
        (&mut f)
            .take(remaining as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|e| ApiError::bad_request(format!("zip read: {e}")))?;
        if content.len() > remaining {
            return Err(ApiError::bad_request(
                "archive too large when decompressed (200MB limit)",
            ));
        }
        total += content.len();
        files.push((path, content));
    }
    Ok(files)
}

/// Validate a file set against the spec and import it as one new skill.
fn import_files_as_skill(
    shelf: &Shelf,
    files: Vec<(String, Vec<u8>)>,
    message: &str,
) -> ApiResult<Skill> {
    let meta = validate_skill_files(&files, "")?;
    let skill = shelf.create_skill_with_kind(&meta.name, &meta.description, KIND_SKILL)?;
    let inputs: Vec<FileInput> = files
        .into_iter()
        .map(|(path, content)| FileInput { path, content })
        .collect();
    shelf.commit(&skill.id, DEFAULT_BRANCH, &inputs, "import", message)?;
    shelf.set_skill_meta(&skill.id, &meta)?;
    Ok(shelf.get_skill(&skill.id)?)
}

// -------------------------------------------------------------- github import

#[derive(Deserialize)]
pub struct GithubReq {
    /// e.g. https://github.com/owner/repo, .../tree/main/path, or owner/repo
    pub url: String,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub subpath: Option<String>,
}

#[derive(Serialize)]
pub struct GithubResp {
    pub imported: Vec<Skill>,
    pub skipped: Vec<String>,
}

/// Import skill(s) from a GitHub repo: download its zipball, then reuse the zip
/// import + spec validation. Supports a whole-repo skill (root SKILL.md) or many
/// skills in subdirectories. Private repos via the `GITHUB_TOKEN` env var.
pub async fn import_github(
    State(st): State<AppState>,
    Json(req): Json<GithubReq>,
) -> ApiResult<Json<GithubResp>> {
    let (owner, repo, ref_from_url, subpath_from_url) = parse_github_url(&req.url)?;
    let git_ref = req.git_ref.or(ref_from_url);
    let subpath = req.subpath.or(subpath_from_url);
    if let Some(r) = &git_ref {
        if r.contains("..") || r.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(ApiError::bad_request("invalid ref"));
        }
    }

    // From runtime config (GitHub Enterprise base + optional token).
    let cfg = st.config().snapshot();
    let api_base = cfg.github_api_base();
    let api_base = api_base.trim_end_matches('/');
    let url = match &git_ref {
        Some(r) => format!("{api_base}/repos/{owner}/{repo}/zipball/{r}"),
        None => format!("{api_base}/repos/{owner}/{repo}/zipball"),
    };
    let bytes = fetch_url(&url, cfg.github_token().as_deref()).await?;

    // Unzip, drop GitHub's `owner-repo-sha/` top dir.
    let mut files = unzip_bytes(&bytes)?;
    if files.is_empty() {
        return Err(ApiError::bad_request("repo archive is empty"));
    }
    strip_common_prefix(&mut files);

    // Optional subpath scoping.
    if let Some(sp) = subpath.as_deref().map(|s| s.trim_matches('/')).filter(|s| !s.is_empty()) {
        let prefix = format!("{sp}/");
        files = files
            .into_iter()
            .filter_map(|(p, c)| p.strip_prefix(&prefix).map(|r| (r.to_string(), c)))
            .collect();
        if files.is_empty() {
            return Err(ApiError::bad_request(format!("subpath '{sp}' has no files")));
        }
    }

    // Find skill directories (dirs containing a SKILL.md), then keep only the
    // top-most ones so a nested SKILL.md isn't both absorbed into its parent
    // skill AND imported again. Root ("") thus collapses to a single skill.
    let dirs = top_most_dirs(find_skill_dirs(&files));
    if dirs.is_empty() {
        return Err(ApiError::bad_request("no SKILL.md found in the repo"));
    }

    let shelf = st.shelf();
    let mut imported = Vec::new();
    let mut skipped = Vec::new();
    for d in dirs {
        let subset = files_under(&files, &d);
        match import_files_as_skill(&shelf, subset, &format!("import from github:{owner}/{repo}")) {
            Ok(s) => imported.push(s),
            Err(e) => skipped.push(format!("{}: {}", if d.is_empty() { "<root>" } else { &d }, e.message)),
        }
    }
    if imported.is_empty() {
        return Err(ApiError::bad_request(format!(
            "no valid skill imported ({})",
            skipped.join("; ")
        )));
    }
    Ok(Json(GithubResp { imported, skipped }))
}

/// Max bytes to download (compressed archive) and to hold decompressed.
const MAX_ARCHIVE_BYTES: usize = 100 * 1024 * 1024;
const MAX_UNZIP_BYTES: usize = 200 * 1024 * 1024;

/// Fetch a URL as bytes with a timeout and a size cap (avoids memory DoS from a
/// giant/malicious repo). GitHub API needs a User-Agent; token is optional.
async fn fetch_url(url: &str, token: Option<&str>) -> ApiResult<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| ApiError::internal(format!("http client: {e}")))?;
    let mut r = client
        .get(url)
        .header("User-Agent", "skill-shelf")
        .header("Accept", "application/vnd.github+json");
    if let Some(tok) = token.filter(|t| !t.is_empty()) {
        r = r.bearer_auth(tok);
    }
    let mut resp = r
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("github fetch failed: {e}")))?;
    if !resp.status().is_success() {
        let code = resp.status();
        return Err(ApiError::bad_request(format!(
            "github returned {code}: {}",
            resp.text().await.unwrap_or_default()
        )));
    }
    if resp.content_length().is_some_and(|l| l as usize > MAX_ARCHIVE_BYTES) {
        return Err(ApiError::bad_request("repo archive exceeds size limit (100MB)"));
    }
    // Stream chunk-by-chunk so a chunked response without Content-Length is also capped.
    let mut buf = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ApiError::internal(format!("github read failed: {e}")))?
    {
        if buf.len() + chunk.len() > MAX_ARCHIVE_BYTES {
            return Err(ApiError::bad_request("repo archive exceeds size limit (100MB)"));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Parse owner/repo (+ optional ref/subpath) from common GitHub URL forms.
/// Limitation: for `/tree/<ref>/<path>` URLs a ref containing `/` (e.g. a
/// branch `feature/x`) is ambiguous with the subpath — pass `ref` explicitly in
/// the body for such branches.
fn parse_github_url(u: &str) -> ApiResult<(String, String, Option<String>, Option<String>)> {
    let s = u.trim();
    let s = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let s = s.strip_prefix("github.com/").unwrap_or(s);
    let s = s.strip_suffix(".git").unwrap_or(s);
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err(ApiError::bad_request("expected github.com/owner/repo"));
    }
    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    let mut git_ref = None;
    let mut subpath = None;
    if parts.len() >= 4 && (parts[2] == "tree" || parts[2] == "blob") {
        git_ref = Some(parts[3].to_string());
        if parts.len() > 4 {
            subpath = Some(parts[4..].join("/"));
        }
    }
    Ok((owner, repo, git_ref, subpath))
}

/// Drop any skill dir nested under another skill dir (root "" covers all).
/// Input assumed containing unique dirs; output keeps only the shallowest.
fn top_most_dirs(mut dirs: Vec<String>) -> Vec<String> {
    dirs.sort(); // "" and shallow paths sort first
    let mut top: Vec<String> = Vec::new();
    for d in dirs {
        let nested = top
            .iter()
            .any(|t| t.is_empty() || d.starts_with(&format!("{t}/")));
        if !nested {
            top.push(d);
        }
    }
    top
}

/// Directories that directly contain a `SKILL.md` (root = "").
fn find_skill_dirs(files: &[(String, Vec<u8>)]) -> Vec<String> {
    let mut dirs: Vec<String> = files
        .iter()
        .filter_map(|(p, _)| {
            if p == "SKILL.md" {
                Some(String::new())
            } else {
                p.strip_suffix("/SKILL.md").map(str::to_string)
            }
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Files under directory `d`, rebased to `d`'s root (d = "" → all files).
fn files_under(files: &[(String, Vec<u8>)], d: &str) -> Vec<(String, Vec<u8>)> {
    if d.is_empty() {
        return files.to_vec();
    }
    let prefix = format!("{d}/");
    files
        .iter()
        .filter_map(|(p, c)| p.strip_prefix(&prefix).map(|r| (r.to_string(), c.clone())))
        .collect()
}

// ---------------------------------------------------------------- validation

/// Decode base64 file payloads to raw bytes.
fn decode_files(files: &[FilePayload]) -> ApiResult<Vec<(String, Vec<u8>)>> {
    files
        .iter()
        .map(|f| {
            let content = STANDARD
                .decode(f.content.as_bytes())
                .map_err(|e| ApiError::bad_request(format!("invalid base64 for {}: {e}", f.path)))?;
            Ok((f.path.clone(), content))
        })
        .collect()
}

/// Enforce the Agent Skills spec on a file set: a root `SKILL.md` must exist,
/// parse, and validate. If `expected_name` is non-empty, the frontmatter name
/// must match it. Returns the parsed frontmatter.
fn validate_skill_files(files: &[(String, Vec<u8>)], expected_name: &str) -> ApiResult<SkillMeta> {
    let smd = files
        .iter()
        .find(|(p, _)| p == "SKILL.md")
        .map(|(_, c)| c)
        .ok_or_else(|| ApiError::bad_request("a skill must include a SKILL.md at its root"))?;
    let meta = skillmd::parse(smd)?;
    skillmd::validate(&meta)?;
    if !expected_name.is_empty() && meta.name != expected_name {
        return Err(ApiError::bad_request(format!(
            "SKILL.md name '{}' must match skill name '{}'",
            meta.name, expected_name
        )));
    }
    Ok(meta)
}

#[derive(Deserialize)]
pub struct ValidateReq {
    pub files: Vec<FilePayload>,
}

#[derive(Serialize)]
pub struct ValidateResp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<SkillMeta>,
    /// Non-fatal lint warnings (e.g. SKILL.md references a missing file).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Validate a candidate skill file set against the spec without committing.
/// Lets the UI check before a commit. Also lints references (missing files) as
/// non-fatal warnings.
pub async fn validate_files(Json(req): Json<ValidateReq>) -> Json<ValidateResp> {
    let decoded = match decode_files(&req.files) {
        Ok(d) => d,
        Err(e) => return Json(ValidateResp { ok: false, error: Some(e.message), meta: None, warnings: vec![] }),
    };
    match validate_skill_files(&decoded, "") {
        Ok(meta) => {
            let skill_md = decoded
                .iter()
                .find(|(p, _)| p == "SKILL.md")
                .map(|(_, c)| String::from_utf8_lossy(c).to_string())
                .unwrap_or_default();
            let paths: Vec<String> = decoded.iter().map(|(p, _)| p.clone()).collect();
            let warnings = skillmd::missing_references(&skill_md, &paths)
                .into_iter()
                .map(|p| format!("SKILL.md references a missing file: {p}"))
                .collect();
            Json(ValidateResp { ok: true, error: None, meta: Some(meta), warnings })
        }
        Err(e) => Json(ValidateResp { ok: false, error: Some(e.message), meta: None, warnings: vec![] }),
    }
}

/// Strip a shared top-level directory (e.g. `pdf-parse/…`) from all paths.
fn strip_common_prefix(files: &mut [(String, Vec<u8>)]) {
    let first_seg = |p: &str| p.split_once('/').map(|(a, _)| a.to_string());
    let Some(prefix) = files.first().and_then(|(p, _)| first_seg(p)) else {
        return;
    };
    let all_share = files
        .iter()
        .all(|(p, _)| p.starts_with(&format!("{prefix}/")));
    if all_share {
        let cut = prefix.len() + 1;
        for (p, _) in files.iter_mut() {
            *p = p[cut..].to_string();
        }
    }
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    if cleaned.is_empty() {
        "skill".to_string()
    } else {
        cleaned
    }
}

// -------------------------------------------------------------------- auth

#[derive(Deserialize)]
pub struct AuthReq {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResp {
    pub token: String,
    pub username: String,
    pub role: String,
}

/// Register a user. The first user becomes `admin`. Requires auth to be enabled.
pub async fn signup(
    State(st): State<AppState>,
    Json(req): Json<AuthReq>,
) -> ApiResult<Json<AuthResp>> {
    let secret = st
        .auth_secret()
        .ok_or_else(|| ApiError::bad_request("auth is disabled (no JWT_SECRET configured)"))?;
    if req.password.len() < 6 {
        return Err(ApiError::bad_request("password must be at least 6 characters"));
    }
    let hash = crate::auth::hash_password(&req.password)?;
    // Count + create under ONE lock so concurrent signups can't both become
    // admin (the first user is admin).
    let user = {
        let shelf = st.shelf();
        let role = if shelf.count_users()? == 0 {
            skill_shelf_core::ROLE_ADMIN
        } else {
            skill_shelf_core::ROLE_USER
        };
        shelf.create_user(&req.username, &hash, role)?
    };
    let token = crate::auth::issue_token(&secret, &user.id, &user.username, &user.role)?;
    Ok(Json(AuthResp { token, username: user.username, role: user.role }))
}

pub async fn signin(
    State(st): State<AppState>,
    Json(req): Json<AuthReq>,
) -> ApiResult<Json<AuthResp>> {
    let secret = st
        .auth_secret()
        .ok_or_else(|| ApiError::bad_request("auth is disabled (no JWT_SECRET configured)"))?;
    let user = st
        .shelf()
        .get_user_by_username(&req.username)
        .map_err(|_| ApiError {
            code: StatusCode::UNAUTHORIZED,
            message: "invalid credentials".into(),
        })?;
    if !crate::auth::verify_password(&req.password, &user.password_hash) {
        return Err(ApiError {
            code: StatusCode::UNAUTHORIZED,
            message: "invalid credentials".into(),
        });
    }
    let token = crate::auth::issue_token(&secret, &user.id, &user.username, &user.role)?;
    Ok(Json(AuthResp { token, username: user.username, role: user.role }))
}

// ------------------------------------------------------------------ config

/// Skill Shelf's own settings (admin). Secrets are masked.
pub async fn get_config(State(st): State<AppState>) -> Json<ConfigView> {
    Json(st.config().view())
}

/// Merge Skill Shelf's own settings (admin). Body is a `{ KEY: value }` object;
/// value may be a string or JSON; `null` deletes a key. Takes effect immediately.
pub async fn put_config(
    State(st): State<AppState>,
    Json(patch): Json<BTreeMap<String, serde_json::Value>>,
) -> ApiResult<Json<ConfigView>> {
    st.config().update(patch).map(Json).map_err(ApiError::from)
}

// ---- config center: namespaces (admin) ---------------------------------

/// List all config-center namespaces (the reserved `_global` is always present).
pub async fn list_namespaces(State(st): State<AppState>) -> Json<Vec<NamespaceInfo>> {
    Json(st.config().list_namespaces())
}

/// View one namespace's stored KV (admin). Secrets are masked.
pub async fn get_namespace(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
) -> ApiResult<Json<NamespaceView>> {
    let ns = q.namespace.unwrap_or_else(|| GLOBAL_NS.to_string());
    if !valid_namespace(&ns) {
        return Err(ApiError::bad_request(format!("invalid namespace: {ns:?}")));
    }
    Ok(Json(st.config().namespace_view(&ns)))
}

/// Merge a patch into one namespace (admin). `null` deletes a key.
pub async fn put_namespace(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
    Json(patch): Json<BTreeMap<String, serde_json::Value>>,
) -> ApiResult<Json<NamespaceView>> {
    let ns = q.namespace.unwrap_or_else(|| GLOBAL_NS.to_string());
    st.config().update_namespace(&ns, patch).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
pub struct NsQuery {
    pub namespace: Option<String>,
}

fn ns_or_global(q: &NsQuery) -> String {
    q.namespace.clone().unwrap_or_else(|| GLOBAL_NS.to_string())
}

// ---- config center: versioning (admin) ---------------------------------

#[derive(Deserialize)]
pub struct PublishReq {
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub note: String,
}

/// Publish a namespace's draft as a new version (what consumers then resolve).
pub async fn publish_namespace(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
    Json(req): Json<PublishReq>,
) -> ApiResult<Json<NamespaceView>> {
    let ns = ns_or_global(&q);
    st.config().publish_namespace(&ns, &req.author, &req.note).map(Json).map_err(ApiError::from)
}

/// Published version history (metadata only), newest first.
pub async fn list_versions(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
) -> ApiResult<Json<Vec<VersionInfo>>> {
    let ns = ns_or_global(&q);
    if !valid_namespace(&ns) {
        return Err(ApiError::bad_request(format!("invalid namespace: {ns:?}")));
    }
    Ok(Json(st.config().list_versions(&ns)))
}

#[derive(Deserialize)]
pub struct VersionQuery {
    pub namespace: Option<String>,
    pub version: u64,
}

/// One published version's KV (admin; secrets masked).
pub async fn get_version(
    State(st): State<AppState>,
    Query(q): Query<VersionQuery>,
) -> ApiResult<Json<Vec<VarView>>> {
    let ns = q.namespace.unwrap_or_else(|| GLOBAL_NS.to_string());
    st.config().version_view(&ns, q.version).map(Json).map_err(ApiError::from)
}

#[derive(Deserialize)]
pub struct NsRollbackReq {
    pub version: u64,
}

/// Load a published version back into the draft (does NOT publish).
pub async fn rollback_namespace(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
    Json(req): Json<NsRollbackReq>,
) -> ApiResult<Json<NamespaceView>> {
    let ns = ns_or_global(&q);
    st.config().rollback_namespace(&ns, req.version).map(Json).map_err(ApiError::from)
}

/// Field-level diff of the draft against the latest published version.
pub async fn diff_namespace(
    State(st): State<AppState>,
    Query(q): Query<NsQuery>,
) -> ApiResult<Json<ConfigDiff>> {
    let ns = ns_or_global(&q);
    if !valid_namespace(&ns) {
        return Err(ApiError::bad_request(format!("invalid namespace: {ns:?}")));
    }
    Ok(Json(st.config().diff_namespace(&ns)))
}

// ---- config center: clients (admin) ------------------------------------

/// List service-token clients (admin). Tokens are never returned.
pub async fn list_clients(State(st): State<AppState>) -> Json<Vec<ClientView>> {
    Json(st.config().list_clients())
}

#[derive(Deserialize)]
pub struct CreateClientReq {
    pub name: String,
    #[serde(default)]
    pub namespaces: Vec<String>,
}

/// Create a client (admin). The plaintext token is returned exactly once.
pub async fn create_client(
    State(st): State<AppState>,
    Json(req): Json<CreateClientReq>,
) -> ApiResult<Json<NewClient>> {
    st.config().create_client(&req.name, req.namespaces).map(Json).map_err(ApiError::from)
}

/// Revoke a client (admin).
pub async fn delete_client(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    st.config().delete_client(&id).map(|_| StatusCode::NO_CONTENT).map_err(ApiError::from)
}

// ---- config center: consume (service token) ----------------------------

/// Resolve a namespace's merged config in plaintext for a consuming service.
/// Auth is a service token in the `X-Config-Token` header (NOT the admin JWT).
pub async fn resolve_config(
    State(st): State<AppState>,
    headers: header::HeaderMap,
    Query(q): Query<NsQuery>,
) -> ApiResult<Json<BTreeMap<String, serde_json::Value>>> {
    let token = headers
        .get("x-config-token")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing X-Config-Token header"))?;
    let ns = q.namespace.unwrap_or_else(|| GLOBAL_NS.to_string());
    match st.config().resolve(token, &ns) {
        Ok(vars) => Ok(Json(vars)),
        Err(ResolveError::Unauthorized) => Err(ApiError::unauthorized("invalid service token")),
        Err(ResolveError::Forbidden) => {
            Err(ApiError::forbidden(format!("token not granted namespace {ns:?}")))
        }
    }
}

// --------------------------------------------------------------- feedback

#[derive(Deserialize)]
pub struct FeedbackReq {
    pub commit_id: Option<String>,
    pub source: Option<String>, // "human" (default) | "agent"
    pub rating: i32,            // -1 / 0 / +1
    pub content: String,
    pub query: Option<String>,
}

pub async fn add_feedback(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<FeedbackReq>,
) -> ApiResult<Json<Feedback>> {
    let rating = req.rating.clamp(-1, 1);
    let source = req.source.as_deref().unwrap_or("human");
    Ok(Json(st.shelf().add_feedback(
        &id,
        req.commit_id.as_deref(),
        source,
        rating,
        &req.content,
        req.query.as_deref(),
    )?))
}

pub async fn list_feedback(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Feedback>>> {
    Ok(Json(st.shelf().list_feedback(&id)?))
}

#[derive(Deserialize)]
pub struct StatusReq {
    pub status: String,
}

pub async fn set_feedback_status(
    State(st): State<AppState>,
    Path((_id, fid)): Path<(String, String)>,
    Json(req): Json<StatusReq>,
) -> ApiResult<StatusCode> {
    st.shelf().set_feedback_status(&fid, &req.status)?;
    Ok(StatusCode::NO_CONTENT)
}

// ----------------------------------------------------------------- refine

#[derive(Serialize)]
pub struct RefineResp {
    pub commit: Commit,
    pub diff: Vec<FileDiff>,
}

/// Draft an improved SKILL.md from open feedback onto the `refine` branch.
/// The slow LLM call runs without holding the shelf lock.
pub async fn refine(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<RefineResp>> {
    // 1. Gather inputs under the lock, then release it.
    let (name, head, mut files, feedback) = {
        let shelf = st.shelf();
        let skill = shelf.get_skill(&id)?;
        if skill.kind != KIND_SKILL {
            return Err(ApiError::bad_request("refine only applies to skills"));
        }
        let head = shelf
            .get_branch(&id, DEFAULT_BRANCH)?
            .head
            .ok_or_else(|| ApiError::bad_request("nothing to refine: no commits yet"))?;
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for e in shelf.read_tree(&head)? {
            files.push((e.path.clone(), shelf.read_file(&head, &e.path)?));
        }
        let feedback = shelf.open_feedback(&id)?;
        (skill.name, head, files, feedback)
    };

    if feedback.is_empty() {
        return Err(ApiError::bad_request("no open feedback to act on"));
    }
    let idx = files
        .iter()
        .position(|(p, _)| p == "SKILL.md")
        .ok_or_else(|| ApiError::bad_request("skill has no SKILL.md to refine"))?;
    let current = String::from_utf8_lossy(&files[idx].1).to_string();

    // 2. LLM call (no lock held). Resolve AI config from the runtime store.
    let ai_cfg = st
        .config()
        .snapshot()
        .ai()
        .ok_or_else(|| ApiError::bad_request("AI not configured: set AI_BASE_URL, AI_API_KEY, AI_MODEL"))?;
    let improved = ai::refine_skillmd(&ai_cfg, &current, &feedback).await?;

    // Validate the model output before it can enter version control (spec
    // enforcement): reject invalid frontmatter or a changed name.
    let meta = skillmd::parse(improved.as_bytes())?;
    skillmd::validate(&meta)?;
    if meta.name != name {
        return Err(ApiError::bad_request(format!(
            "refined SKILL.md name '{}' must match skill name '{}'",
            meta.name, name
        )));
    }
    files[idx].1 = improved.into_bytes();

    // 3. Commit the draft onto a fresh `refine` branch based on main head.
    let shelf = st.shelf();
    shelf.point_branch(&id, REFINE_BRANCH, Some(&head))?;
    let inputs: Vec<FileInput> = files
        .into_iter()
        .map(|(path, content)| FileInput { path, content })
        .collect();
    let commit = shelf.commit(&id, REFINE_BRANCH, &inputs, "refine-bot", "refine draft from feedback")?;
    let diff = shelf.diff(&head, &commit.id)?;
    Ok(Json(RefineResp { commit, diff }))
}

/// Accept the refine draft into main: merge, re-index from the new SKILL.md,
/// and mark open feedback as applied.
pub async fn refine_merge(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Commit>> {
    let shelf = st.shelf();
    // Validate the refine draft BEFORE advancing main, so main never moves to
    // an invalid commit.
    let refine_head = shelf
        .get_branch(&id, REFINE_BRANCH)?
        .head
        .ok_or_else(|| ApiError::bad_request("no refine draft to merge"))?;
    let smd = shelf.read_file(&refine_head, "SKILL.md")?;
    let meta = skillmd::parse(&smd)?;
    skillmd::validate(&meta)?;
    let skill = shelf.get_skill(&id)?;
    if meta.name != skill.name {
        return Err(ApiError::bad_request(format!(
            "refine draft name '{}' must match skill name '{}'",
            meta.name, skill.name
        )));
    }
    let merged = shelf.merge_branch(&id, REFINE_BRANCH, DEFAULT_BRANCH, "refine-merge")?;
    shelf.set_skill_meta(&id, &meta)?;
    shelf.mark_feedback_applied(&id)?;
    Ok(Json(merged))
}
