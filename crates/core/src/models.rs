use serde::{Deserialize, Serialize};

/// A versioned unit on the shelf. Two kinds share the same machinery:
/// - `skill`  — a multi-file Claude Agent Skill package (SKILL.md + resources)
/// - `prompt` — effectively a single-file skill (one text blob)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// "skill" | "prompt"
    pub kind: String,
    // Cached SKILL.md frontmatter (refreshed from the committed SKILL.md), so
    // basic info is one cheap DB read. SKILL.md remains the source of truth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub metadata: std::collections::BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<String>,
    pub created_at: i64,
}

/// The valid values for [`Skill::kind`].
pub const KIND_SKILL: &str = "skill";
pub const KIND_PROMPT: &str = "prompt";

/// A named line of development (like a git branch), pointing at its head commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub skill_id: String,
    pub name: String,
    pub head: Option<String>,
}

/// An immutable snapshot of the whole skill directory.
///
/// `id` is the content hash of the commit object and is NOT part of the
/// serialized form (that would be circular); it is filled in on read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    #[serde(default)]
    pub id: String,
    pub tree: String,
    pub parent: Option<String>,
    pub author: String,
    pub message: String,
    pub timestamp: i64,
}

/// One entry in a tree: a file path mapped to the content-addressed blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub path: String,
    pub hash: String,
    pub size: u64,
}

/// Input file for a commit.
#[derive(Debug, Clone)]
pub struct FileInput {
    pub path: String,
    pub content: Vec<u8>,
}

/// A registry user (for authenticated web/team deployments).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    /// Argon2 hash; never serialized to clients.
    #[serde(skip_serializing)]
    pub password_hash: String,
    /// "admin" | "user"
    pub role: String,
    pub created_at: i64,
}

pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_USER: &str = "user";

/// Feedback on a skill (and optionally a specific version), driving optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: String,
    pub skill_id: String,
    /// The version the feedback targets, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_id: Option<String>,
    /// "human" | "agent"
    pub source: String,
    /// -1 (bad) / 0 (neutral) / +1 (good)
    pub rating: i32,
    pub content: String,
    /// The routing query that led here, if from an agent — feeds routing too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// "open" | "applied" | "dismissed"
    pub status: String,
    pub created_at: i64,
}

pub const FEEDBACK_OPEN: &str = "open";
pub const FEEDBACK_APPLIED: &str = "applied";
pub const FEEDBACK_DISMISSED: &str = "dismissed";

/// A skill returned by the router, ranked for a query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    /// higher = better match
    pub score: f64,
}

/// Per-file difference between two commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    /// one of: added | removed | modified | unchanged
    pub status: String,
    /// unified text diff, present only for changed UTF-8 text files
    pub diff: Option<String>,
}
