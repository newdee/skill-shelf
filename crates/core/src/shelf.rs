use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use uuid::Uuid;

use crate::diff::text_diff;
use crate::error::{Error, Result};
use crate::index::{Index, PgIndex, SqliteIndex};
use crate::models::{
    Branch, Commit, Feedback, FileDiff, FileInput, RouteResult, Skill, TreeEntry, User,
    FEEDBACK_OPEN, KIND_PROMPT, KIND_SKILL,
};
use crate::skillmd::SkillMeta;
use crate::store::ObjectStore;

/// The default branch name created with every new skill.
pub const DEFAULT_BRANCH: &str = "main";

/// The version-controlled store for skills. File content and the immutable
/// version tree live in a content-addressed [`ObjectStore`] (filesystem, shared
/// by all backends); metadata + routing live in a pluggable [`Index`]
/// (SQLite locally, Postgres for shared deployments).
pub struct Shelf {
    store: ObjectStore,
    index: Box<dyn Index>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Shelf {
    /// Open a SQLite-backed shelf rooted at `root` (objects in `root/objects`,
    /// index at `root/index.db`).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let store = ObjectStore::new(root.join("objects"))?;
        let index = Box::new(SqliteIndex::open(root.join("index.db"))?);
        Ok(Self { store, index })
    }

    /// Open a Postgres-backed shelf: metadata/routing in Postgres (`db_url`),
    /// CAS blobs on the filesystem under `data_dir/objects`.
    pub fn open_postgres(db_url: &str, data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref();
        std::fs::create_dir_all(data_dir)?;
        let store = ObjectStore::new(data_dir.join("objects"))?;
        let index = Box::new(PgIndex::connect(db_url)?);
        Ok(Self { store, index })
    }

    // ----------------------------------------------------------------- skills

    pub fn create_skill(&self, name: &str, description: &str) -> Result<Skill> {
        self.create_skill_with_kind(name, description, KIND_SKILL)
    }

    pub fn create_skill_with_kind(&self, name: &str, description: &str, kind: &str) -> Result<Skill> {
        if name.trim().is_empty() {
            return Err(Error::Invalid("skill name must not be empty".into()));
        }
        if kind != KIND_SKILL && kind != KIND_PROMPT {
            return Err(Error::Invalid(format!("invalid kind: {kind}")));
        }
        let skill = Skill {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            kind: kind.to_string(),
            license: None,
            compatibility: None,
            metadata: Default::default(),
            allowed_tools: None,
            created_at: now(),
        };
        self.index.insert_skill(&skill)?;
        self.index.insert_branch(&skill.id, DEFAULT_BRANCH, None)?;
        Ok(skill)
    }

    pub fn list_skills(&self) -> Result<Vec<Skill>> {
        self.index.search_skills(None, None, i64::MAX, 0)
    }

    pub fn search_skills(&self, q: Option<&str>, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Skill>> {
        self.index.search_skills(q, kind, limit, offset)
    }

    pub fn get_skill(&self, id: &str) -> Result<Skill> {
        self.index.get_skill(id)
    }

    pub fn get_skill_by_name(&self, name: &str) -> Result<Skill> {
        self.index.get_skill_by_name(name)
    }

    pub fn delete_skill(&self, id: &str) -> Result<()> {
        self.index.delete_skill(id)
    }

    /// Refresh the cached SKILL.md frontmatter (called after a validated
    /// skill-kind commit); keeps `description` + routing index in sync.
    pub fn set_skill_meta(&self, id: &str, meta: &SkillMeta) -> Result<()> {
        self.index.update_skill_meta(id, meta)
    }

    // --------------------------------------------------------------- branches

    pub fn get_branch(&self, skill_id: &str, name: &str) -> Result<Branch> {
        self.index.get_branch(skill_id, name)
    }

    pub fn list_branches(&self, skill_id: &str) -> Result<Vec<Branch>> {
        self.index.list_branches(skill_id)
    }

    pub fn create_branch(&self, skill_id: &str, name: &str, from: Option<&str>) -> Result<Branch> {
        self.get_skill(skill_id)?;
        if self.index.get_branch(skill_id, name).is_ok() {
            return Err(Error::Invalid(format!("branch {name} already exists")));
        }
        let head = match from {
            Some(f) => self.index.get_branch(skill_id, f)?.head,
            None => None,
        };
        self.index.insert_branch(skill_id, name, head.as_deref())?;
        Ok(Branch { skill_id: skill_id.to_string(), name: name.to_string(), head })
    }

    /// Point `name` at `commit`, creating the branch if needed (refine flow).
    pub fn point_branch(&self, skill_id: &str, name: &str, commit: Option<&str>) -> Result<()> {
        self.index.upsert_branch(skill_id, name, commit)
    }

    // ---------------------------------------------------------------- commits

    fn write_tree(&self, files: &[FileInput]) -> Result<String> {
        let mut entries: Vec<TreeEntry> = Vec::with_capacity(files.len());
        for f in files {
            let hash = self.store.put(&f.content)?;
            entries.push(TreeEntry { path: f.path.clone(), hash, size: f.content.len() as u64 });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let json = serde_json::to_vec(&entries)?;
        self.store.put(&json)
    }

    /// Commit a full snapshot of `files` onto `branch`, advancing its head.
    pub fn commit(&self, skill_id: &str, branch: &str, files: &[FileInput], author: &str, message: &str) -> Result<Commit> {
        let br = self.get_branch(skill_id, branch)?;
        let tree = self.write_tree(files)?;
        self.write_commit(skill_id, branch, tree, br.head, author, message)
    }

    /// Build, store, and point `branch` at a commit object with the given tree.
    /// `id` is empty while hashing so the hash is a stable function of content.
    fn write_commit(&self, skill_id: &str, branch: &str, tree: String, parent: Option<String>, author: &str, message: &str) -> Result<Commit> {
        let mut commit = Commit {
            id: String::new(),
            tree,
            parent,
            author: author.to_string(),
            message: message.to_string(),
            timestamp: now(),
        };
        let bytes = serde_json::to_vec(&commit)?;
        let id = self.store.put(&bytes)?;
        commit.id = id.clone();
        self.index.set_branch_head(skill_id, branch, &id)?;
        Ok(commit)
    }

    pub fn get_commit(&self, commit_id: &str) -> Result<Commit> {
        let data = self
            .store
            .get(commit_id)
            .map_err(|_| Error::NotFound(format!("commit {commit_id}")))?;
        let mut commit: Commit = serde_json::from_slice(&data)?;
        commit.id = commit_id.to_string();
        Ok(commit)
    }

    /// Commits reachable from `branch`'s head, newest first.
    pub fn list_commits(&self, skill_id: &str, branch: &str) -> Result<Vec<Commit>> {
        let br = self.get_branch(skill_id, branch)?;
        let mut out = Vec::new();
        let mut cur = br.head;
        while let Some(id) = cur {
            let c = self.get_commit(&id)?;
            cur = c.parent.clone();
            out.push(c);
        }
        Ok(out)
    }

    /// Roll `branch` back to the tree of `to_commit` (history-preserving).
    pub fn rollback(&self, skill_id: &str, branch: &str, to_commit: &str, author: &str) -> Result<Commit> {
        let target = self.get_commit(to_commit)?;
        let br = self.get_branch(skill_id, branch)?;
        let short = &to_commit[..to_commit.len().min(8)];
        self.write_commit(skill_id, branch, target.tree, br.head, author, &format!("rollback to {short}"))
    }

    /// Merge `from`'s tree onto `to` as a new commit (accept a refine draft).
    pub fn merge_branch(&self, skill_id: &str, from: &str, to: &str, author: &str) -> Result<Commit> {
        let from_head = self
            .get_branch(skill_id, from)?
            .head
            .ok_or_else(|| Error::Invalid(format!("branch {from} has no commits")))?;
        let tree = self.get_commit(&from_head)?.tree;
        let to_head = self.get_branch(skill_id, to)?.head;
        self.write_commit(skill_id, to, tree, to_head, author, &format!("merge {from}"))
    }

    // ------------------------------------------------------------------ trees

    pub fn read_tree(&self, commit_id: &str) -> Result<Vec<TreeEntry>> {
        let commit = self.get_commit(commit_id)?;
        let data = self.store.get(&commit.tree)?;
        Ok(serde_json::from_slice(&data)?)
    }

    fn tree_map(&self, commit_id: &str) -> Result<BTreeMap<String, TreeEntry>> {
        Ok(self.read_tree(commit_id)?.into_iter().map(|e| (e.path.clone(), e)).collect())
    }

    pub fn read_file(&self, commit_id: &str, path: &str) -> Result<Vec<u8>> {
        let entry = self
            .read_tree(commit_id)?
            .into_iter()
            .find(|e| e.path == path)
            .ok_or_else(|| Error::NotFound(format!("file {path} in commit {commit_id}")))?;
        self.store.get(&entry.hash)
    }

    // ------------------------------------------------------------------- diff

    /// Per-file diff between two commits, with a text diff for changed UTF-8 files.
    pub fn diff(&self, a: &str, b: &str) -> Result<Vec<FileDiff>> {
        let ta = self.tree_map(a)?;
        let tb = self.tree_map(b)?;
        let paths: std::collections::BTreeSet<String> = ta.keys().chain(tb.keys()).cloned().collect();

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            let ea = ta.get(&path);
            let eb = tb.get(&path);
            let status = match (ea, eb) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(x), Some(y)) if x.hash == y.hash => "unchanged",
                _ => "modified",
            };
            let diff = if status == "unchanged" {
                None
            } else {
                let old = match ea { Some(e) => self.store.get(&e.hash)?, None => Vec::new() };
                let new = match eb { Some(e) => self.store.get(&e.hash)?, None => Vec::new() };
                text_diff(&old, &new)
            };
            out.push(FileDiff { path, status: status.to_string(), diff });
        }
        Ok(out)
    }

    // --------------------------------------------------------------- feedback

    pub fn add_feedback(&self, skill_id: &str, commit_id: Option<&str>, source: &str, rating: i32, content: &str, query: Option<&str>) -> Result<Feedback> {
        self.get_skill(skill_id)?;
        let fb = Feedback {
            id: Uuid::new_v4().to_string(),
            skill_id: skill_id.to_string(),
            commit_id: commit_id.map(str::to_string),
            source: source.to_string(),
            rating,
            content: content.to_string(),
            query: query.map(str::to_string),
            status: FEEDBACK_OPEN.to_string(),
            created_at: now(),
        };
        self.index.insert_feedback(&fb)?;
        Ok(fb)
    }

    pub fn list_feedback(&self, skill_id: &str) -> Result<Vec<Feedback>> {
        self.index.list_feedback(skill_id)
    }

    /// Open feedback for a skill (the input to a refine pass).
    pub fn open_feedback(&self, skill_id: &str) -> Result<Vec<Feedback>> {
        Ok(self.list_feedback(skill_id)?.into_iter().filter(|f| f.status == FEEDBACK_OPEN).collect())
    }

    pub fn set_feedback_status(&self, id: &str, status: &str) -> Result<()> {
        self.index.set_feedback_status(id, status)
    }

    pub fn mark_feedback_applied(&self, skill_id: &str) -> Result<()> {
        self.index.mark_feedback_applied(skill_id)
    }

    // ------------------------------------------------------------------ users

    pub fn count_users(&self) -> Result<i64> {
        self.index.count_users()
    }

    pub fn create_user(&self, username: &str, password_hash: &str, role: &str) -> Result<User> {
        if username.trim().is_empty() {
            return Err(Error::Invalid("username must not be empty".into()));
        }
        if self.index.get_user_by_username(username).is_ok() {
            return Err(Error::Invalid(format!("username {username} already taken")));
        }
        let user = User {
            id: Uuid::new_v4().to_string(),
            username: username.to_string(),
            password_hash: password_hash.to_string(),
            role: role.to_string(),
            created_at: now(),
        };
        self.index.insert_user(&user)?;
        Ok(user)
    }

    pub fn get_user_by_username(&self, username: &str) -> Result<User> {
        self.index.get_user_by_username(username)
    }

    // ------------------------------------------------------------------ route

    /// Phase-A routing: keyword recall (BM25/ts_rank) via the index. LLM rerank
    /// happens in the server layer.
    pub fn route(&self, query: &str, top_k: usize) -> Result<Vec<RouteResult>> {
        self.index.route(query, top_k)
    }
}
