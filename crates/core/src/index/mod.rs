//! The metadata/routing index behind the [`Shelf`](crate::Shelf). The CAS
//! object store (blobs/trees/commits) is filesystem-based and shared; only the
//! index (skills, branches, feedback, users, search) is backend-specific.
//!
//! Two implementations: [`sqlite::SqliteIndex`] (local/desktop) and
//! [`pg::PgIndex`] (Postgres, for shared web deployments).

mod pg;
mod sqlite;

pub use pg::PgIndex;
pub use sqlite::SqliteIndex;

use crate::error::Result;
use crate::models::{Branch, Feedback, RouteResult, Skill, User};
use crate::skillmd::SkillMeta;

/// Backend-specific metadata + routing store. Implementations own their schema
/// and keep their search index in sync inside the write methods.
pub trait Index: Send {
    // --- skills (insert also indexes name+description for search) ---
    fn insert_skill(&self, skill: &Skill) -> Result<()>;
    fn get_skill(&self, id: &str) -> Result<Skill>;
    fn get_skill_by_name(&self, name: &str) -> Result<Skill>;
    fn search_skills(&self, q: Option<&str>, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Skill>>;
    fn delete_skill(&self, id: &str) -> Result<()>;
    fn update_skill_meta(&self, id: &str, meta: &SkillMeta) -> Result<()>;

    // --- branches (heads only; commit objects live in the CAS) ---
    fn insert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()>;
    fn upsert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()>;
    fn set_branch_head(&self, skill_id: &str, name: &str, head: &str) -> Result<()>;
    fn get_branch(&self, skill_id: &str, name: &str) -> Result<Branch>;
    fn list_branches(&self, skill_id: &str) -> Result<Vec<Branch>>;

    // --- routing (keyword recall; LLM rerank happens above, DB-agnostic) ---
    fn route(&self, query: &str, top_k: usize) -> Result<Vec<RouteResult>>;

    // --- feedback ---
    fn insert_feedback(&self, f: &Feedback) -> Result<()>;
    fn list_feedback(&self, skill_id: &str) -> Result<Vec<Feedback>>;
    fn set_feedback_status(&self, id: &str, status: &str) -> Result<()>;
    fn mark_feedback_applied(&self, skill_id: &str) -> Result<()>;

    // --- users ---
    fn insert_user(&self, user: &User) -> Result<()>;
    fn get_user_by_username(&self, username: &str) -> Result<User>;
    fn count_users(&self) -> Result<i64>;
}
