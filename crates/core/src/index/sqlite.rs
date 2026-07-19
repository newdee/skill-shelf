use std::path::Path;

use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{Error, Result};
use crate::index::Index;
use crate::models::{Branch, Feedback, RouteResult, Skill, User};
use crate::skillmd::SkillMeta;

/// SQLite-backed index (local/desktop; FTS5 BM25 routing).
pub struct SqliteIndex {
    db: Connection,
}

impl SqliteIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let idx = Self { db: Connection::open(path)? };
        idx.migrate()?;
        Ok(idx)
    }

    fn migrate(&self) -> Result<()> {
        self.db.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS skills (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'skill', license TEXT, compatibility TEXT,
                metadata TEXT NOT NULL DEFAULT '{}', allowed_tools TEXT, created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS branches (
                skill_id TEXT NOT NULL, name TEXT NOT NULL, head TEXT, PRIMARY KEY (skill_id, name)
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS skill_fts USING fts5 (skill_id UNINDEXED, name, description);
            CREATE TABLE IF NOT EXISTS feedback (
                id TEXT PRIMARY KEY, skill_id TEXT NOT NULL, commit_id TEXT,
                source TEXT NOT NULL DEFAULT 'human', rating INTEGER NOT NULL DEFAULT 0,
                content TEXT NOT NULL DEFAULT '', query TEXT, status TEXT NOT NULL DEFAULT 'open',
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_feedback_skill ON feedback(skill_id);
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY, username TEXT NOT NULL UNIQUE, password_hash TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'user', created_at INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(())
    }
}

const SKILL_COLS: &str =
    "id, name, description, kind, license, compatibility, metadata, allowed_tools, created_at";

fn row_to_skill(r: &rusqlite::Row) -> rusqlite::Result<Skill> {
    let metadata_json: String = r.get(6)?;
    Ok(Skill {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        kind: r.get(3)?,
        license: r.get(4)?,
        compatibility: r.get(5)?,
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        allowed_tools: r.get(7)?,
        created_at: r.get(8)?,
    })
}

fn row_to_branch(r: &rusqlite::Row) -> rusqlite::Result<Branch> {
    Ok(Branch { skill_id: r.get(0)?, name: r.get(1)?, head: r.get(2)? })
}

fn row_to_feedback(r: &rusqlite::Row) -> rusqlite::Result<Feedback> {
    Ok(Feedback {
        id: r.get(0)?,
        skill_id: r.get(1)?,
        commit_id: r.get(2)?,
        source: r.get(3)?,
        rating: r.get(4)?,
        content: r.get(5)?,
        query: r.get(6)?,
        status: r.get(7)?,
        created_at: r.get(8)?,
    })
}

/// Safe FTS5 MATCH expr: quote each alnum token, OR them.
fn build_match_query(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

impl Index for SqliteIndex {
    fn insert_skill(&self, s: &Skill) -> Result<()> {
        let metadata = serde_json::to_string(&s.metadata)?;
        self.db.execute(
            "INSERT INTO skills (id, name, description, kind, license, compatibility, metadata, allowed_tools, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![s.id, s.name, s.description, s.kind, s.license, s.compatibility, metadata, s.allowed_tools, s.created_at],
        )?;
        self.db.execute(
            "INSERT INTO skill_fts (skill_id, name, description) VALUES (?1,?2,?3)",
            params![s.id, s.name, s.description],
        )?;
        Ok(())
    }

    fn get_skill(&self, id: &str) -> Result<Skill> {
        self.db
            .query_row(&format!("SELECT {SKILL_COLS} FROM skills WHERE id = ?1"), params![id], row_to_skill)
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("skill {id}")))
    }

    fn get_skill_by_name(&self, name: &str) -> Result<Skill> {
        self.db
            .query_row(
                &format!("SELECT {SKILL_COLS} FROM skills WHERE name = ?1 ORDER BY created_at DESC LIMIT 1"),
                params![name],
                row_to_skill,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("skill named {name}")))
    }

    fn search_skills(&self, q: Option<&str>, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Skill>> {
        let mut sql = format!("SELECT {SKILL_COLS} FROM skills WHERE 1=1");
        let mut args: Vec<Value> = Vec::new();
        if let Some(q) = q.map(str::trim).filter(|s| !s.is_empty()) {
            sql.push_str(" AND (name LIKE ?1 OR description LIKE ?1)");
            args.push(Value::Text(format!("%{q}%")));
        }
        if let Some(k) = kind {
            sql.push_str(&format!(" AND kind = ?{}", args.len() + 1));
            args.push(Value::Text(k.to_string()));
        }
        sql.push_str(&format!(" ORDER BY created_at DESC, id LIMIT ?{} OFFSET ?{}", args.len() + 1, args.len() + 2));
        args.push(Value::Integer(limit));
        args.push(Value::Integer(offset));
        let mut stmt = self.db.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args), row_to_skill)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn delete_skill(&self, id: &str) -> Result<()> {
        let n = self.db.execute("DELETE FROM skills WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(Error::NotFound(format!("skill {id}")));
        }
        self.db.execute("DELETE FROM branches WHERE skill_id = ?1", params![id])?;
        self.db.execute("DELETE FROM skill_fts WHERE skill_id = ?1", params![id])?;
        Ok(())
    }

    fn update_skill_meta(&self, id: &str, meta: &SkillMeta) -> Result<()> {
        let metadata = serde_json::to_string(&meta.metadata)?;
        let n = self.db.execute(
            "UPDATE skills SET description=?2, license=?3, compatibility=?4, metadata=?5, allowed_tools=?6 WHERE id=?1",
            params![id, meta.description, meta.license, meta.compatibility, metadata, meta.allowed_tools],
        )?;
        if n == 0 {
            return Err(Error::NotFound(format!("skill {id}")));
        }
        self.db.execute("UPDATE skill_fts SET description=?2 WHERE skill_id=?1", params![id, meta.description])?;
        Ok(())
    }


    fn insert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()> {
        self.db.execute(
            "INSERT INTO branches (skill_id, name, head) VALUES (?1,?2,?3)",
            params![skill_id, name, head],
        )?;
        Ok(())
    }

    fn upsert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()> {
        self.db.execute(
            "INSERT INTO branches (skill_id, name, head) VALUES (?1,?2,?3) \
             ON CONFLICT(skill_id, name) DO UPDATE SET head = excluded.head",
            params![skill_id, name, head],
        )?;
        Ok(())
    }

    fn set_branch_head(&self, skill_id: &str, name: &str, head: &str) -> Result<()> {
        self.db.execute(
            "UPDATE branches SET head = ?3 WHERE skill_id = ?1 AND name = ?2",
            params![skill_id, name, head],
        )?;
        Ok(())
    }

    fn get_branch(&self, skill_id: &str, name: &str) -> Result<Branch> {
        self.db
            .query_row(
                "SELECT skill_id, name, head FROM branches WHERE skill_id = ?1 AND name = ?2",
                params![skill_id, name],
                row_to_branch,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("branch {name} of skill {skill_id}")))
    }

    fn list_branches(&self, skill_id: &str) -> Result<Vec<Branch>> {
        let mut stmt = self
            .db
            .prepare("SELECT skill_id, name, head FROM branches WHERE skill_id = ?1 ORDER BY name")?;
        let rows = stmt.query_map(params![skill_id], row_to_branch)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn route(&self, query: &str, top_k: usize) -> Result<Vec<RouteResult>> {
        let m = build_match_query(query);
        if m.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.db.prepare(
            "SELECT f.skill_id, s.name, s.description, bm25(skill_fts) AS score \
             FROM skill_fts f JOIN skills s ON s.id = f.skill_id \
             WHERE skill_fts MATCH ?1 ORDER BY score LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![m, top_k as i64], |r| {
            let bm25: f64 = r.get(3)?;
            Ok(RouteResult { skill_id: r.get(0)?, name: r.get(1)?, description: r.get(2)?, score: -bm25 })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn insert_feedback(&self, f: &Feedback) -> Result<()> {
        self.db.execute(
            "INSERT INTO feedback (id, skill_id, commit_id, source, rating, content, query, status, created_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![f.id, f.skill_id, f.commit_id, f.source, f.rating, f.content, f.query, f.status, f.created_at],
        )?;
        Ok(())
    }

    fn list_feedback(&self, skill_id: &str) -> Result<Vec<Feedback>> {
        let mut stmt = self.db.prepare(
            "SELECT id, skill_id, commit_id, source, rating, content, query, status, created_at \
             FROM feedback WHERE skill_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![skill_id], row_to_feedback)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn set_feedback_status(&self, id: &str, status: &str) -> Result<()> {
        let n = self.db.execute("UPDATE feedback SET status = ?2 WHERE id = ?1", params![id, status])?;
        if n == 0 {
            return Err(Error::NotFound(format!("feedback {id}")));
        }
        Ok(())
    }

    fn mark_feedback_applied(&self, skill_id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE feedback SET status = 'applied' WHERE skill_id = ?1 AND status = 'open'",
            params![skill_id],
        )?;
        Ok(())
    }

    fn insert_user(&self, u: &User) -> Result<()> {
        self.db.execute(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?1,?2,?3,?4,?5)",
            params![u.id, u.username, u.password_hash, u.role, u.created_at],
        )?;
        Ok(())
    }

    fn get_user_by_username(&self, username: &str) -> Result<User> {
        self.db
            .query_row(
                "SELECT id, username, password_hash, role, created_at FROM users WHERE username = ?1",
                params![username],
                |r| Ok(User {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    password_hash: r.get(2)?,
                    role: r.get(3)?,
                    created_at: r.get(4)?,
                }),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("user {username}")))
    }

    fn count_users(&self) -> Result<i64> {
        Ok(self.db.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?)
    }
}
