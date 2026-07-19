use std::sync::mpsc::{channel, Sender};
use std::thread;

use postgres::{Client, NoTls, Row};

use crate::error::{Error, Result};
use crate::index::Index;
use crate::models::{Branch, Feedback, RouteResult, Skill, User};
use crate::skillmd::SkillMeta;

/// A job run on the Postgres worker thread with exclusive access to the client.
type Job = Box<dyn FnOnce(&mut Client) + Send>;

/// Postgres-backed index (shared web deployments). Keyword routing uses a
/// generated `tsvector` column + `ts_rank` (native, no extension).
///
/// The sync `postgres` client calls `block_on` internally, which panics inside
/// a tokio runtime (the server is `#[tokio::main]`). So the client lives on a
/// dedicated thread with no async context; methods proxy work to it over a
/// channel and block on the reply. The single worker also serializes access.
pub struct PgIndex {
    tx: Sender<Job>,
}

fn dberr(e: postgres::Error) -> Error {
    Error::Other(format!("postgres: {e}"))
}

impl PgIndex {
    pub fn connect(url: &str) -> Result<Self> {
        let (tx, rx) = channel::<Job>();
        let (ready_tx, ready_rx) = channel::<std::result::Result<(), String>>();
        let url = url.to_string();
        thread::spawn(move || {
            let mut client = match Client::connect(&url, NoTls) {
                Ok(mut c) => match migrate(&mut c) {
                    Ok(()) => {
                        let _ = ready_tx.send(Ok(()));
                        c
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                },
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            for job in rx {
                job(&mut client);
            }
        });
        ready_rx
            .recv()
            .map_err(|_| Error::Other("postgres worker exited".into()))?
            .map_err(Error::Other)?;
        Ok(Self { tx })
    }

    /// Run `f` on the worker thread and block for its result.
    fn call<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Client) -> Result<T> + Send + 'static,
    {
        let (otx, orx) = channel::<Result<T>>();
        self.tx
            .send(Box::new(move |c| {
                let _ = otx.send(f(c));
            }))
            .map_err(|_| Error::Other("postgres worker gone".into()))?;
        orx.recv().map_err(|_| Error::Other("postgres worker dropped".into()))?
    }
}

fn migrate(c: &mut Client) -> std::result::Result<(), postgres::Error> {
    c.batch_execute(
        r#"
        CREATE TABLE IF NOT EXISTS skills (
            id text PRIMARY KEY, name text NOT NULL, description text NOT NULL DEFAULT '',
            kind text NOT NULL DEFAULT 'skill', license text, compatibility text,
            metadata text NOT NULL DEFAULT '{}', allowed_tools text, created_at bigint NOT NULL,
            fts tsvector GENERATED ALWAYS AS
                (to_tsvector('english', coalesce(name,'') || ' ' || coalesce(description,''))) STORED
        );
        CREATE INDEX IF NOT EXISTS skills_fts_idx ON skills USING gin(fts);
        CREATE TABLE IF NOT EXISTS branches (
            skill_id text NOT NULL, name text NOT NULL, head text, PRIMARY KEY (skill_id, name)
        );
        CREATE TABLE IF NOT EXISTS feedback (
            id text PRIMARY KEY, skill_id text NOT NULL, commit_id text,
            source text NOT NULL DEFAULT 'human', rating integer NOT NULL DEFAULT 0,
            content text NOT NULL DEFAULT '', query text, status text NOT NULL DEFAULT 'open',
            created_at bigint NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_feedback_skill ON feedback(skill_id);
        CREATE TABLE IF NOT EXISTS users (
            id text PRIMARY KEY, username text NOT NULL UNIQUE, password_hash text NOT NULL,
            role text NOT NULL DEFAULT 'user', created_at bigint NOT NULL
        );
        "#,
    )
}

const SKILL_COLS: &str =
    "id, name, description, kind, license, compatibility, metadata, allowed_tools, created_at";

fn row_to_skill(r: &Row) -> Skill {
    let metadata_json: String = r.get(6);
    Skill {
        id: r.get(0),
        name: r.get(1),
        description: r.get(2),
        kind: r.get(3),
        license: r.get(4),
        compatibility: r.get(5),
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        allowed_tools: r.get(7),
        created_at: r.get(8),
    }
}

fn row_to_branch(r: &Row) -> Branch {
    Branch { skill_id: r.get(0), name: r.get(1), head: r.get(2) }
}

fn row_to_feedback(r: &Row) -> Feedback {
    Feedback {
        id: r.get(0),
        skill_id: r.get(1),
        commit_id: r.get(2),
        source: r.get(3),
        rating: r.get(4),
        content: r.get(5),
        query: r.get(6),
        status: r.get(7),
        created_at: r.get(8),
    }
}

impl Index for PgIndex {
    fn insert_skill(&self, s: &Skill) -> Result<()> {
        let s = s.clone();
        self.call(move |c| {
            let metadata = serde_json::to_string(&s.metadata)?;
            c.execute(
                &format!("INSERT INTO skills ({SKILL_COLS}) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"),
                &[&s.id, &s.name, &s.description, &s.kind, &s.license, &s.compatibility, &metadata, &s.allowed_tools, &s.created_at],
            )
            .map_err(dberr)?;
            Ok(())
        })
    }

    fn get_skill(&self, id: &str) -> Result<Skill> {
        let id = id.to_string();
        self.call(move |c| {
            c.query_opt(&format!("SELECT {SKILL_COLS} FROM skills WHERE id=$1"), &[&id])
                .map_err(dberr)?
                .map(|r| row_to_skill(&r))
                .ok_or_else(|| Error::NotFound(format!("skill {id}")))
        })
    }

    fn get_skill_by_name(&self, name: &str) -> Result<Skill> {
        let name = name.to_string();
        self.call(move |c| {
            c.query_opt(
                &format!("SELECT {SKILL_COLS} FROM skills WHERE name=$1 ORDER BY created_at DESC LIMIT 1"),
                &[&name],
            )
            .map_err(dberr)?
            .map(|r| row_to_skill(&r))
            .ok_or_else(|| Error::NotFound(format!("skill named {name}")))
        })
    }

    fn search_skills(&self, q: Option<&str>, kind: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Skill>> {
        let q = q.map(str::to_string);
        let kind = kind.map(str::to_string);
        self.call(move |c| {
            let rows = c.query(
                &format!(
                    "SELECT {SKILL_COLS} FROM skills \
                     WHERE ($1::text IS NULL OR name ILIKE '%'||$1||'%' OR description ILIKE '%'||$1||'%') \
                       AND ($2::text IS NULL OR kind=$2) \
                     ORDER BY created_at DESC, id LIMIT $3 OFFSET $4"
                ),
                &[&q, &kind, &limit, &offset],
            )
            .map_err(dberr)?;
            Ok(rows.iter().map(row_to_skill).collect())
        })
    }

    fn delete_skill(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.call(move |c| {
            let n = c.execute("DELETE FROM skills WHERE id=$1", &[&id]).map_err(dberr)?;
            if n == 0 {
                return Err(Error::NotFound(format!("skill {id}")));
            }
            c.execute("DELETE FROM branches WHERE skill_id=$1", &[&id]).map_err(dberr)?;
            Ok(())
        })
    }

    fn update_skill_meta(&self, id: &str, meta: &SkillMeta) -> Result<()> {
        let id = id.to_string();
        let meta = meta.clone();
        self.call(move |c| {
            let metadata = serde_json::to_string(&meta.metadata)?;
            let n = c.execute(
                "UPDATE skills SET description=$2, license=$3, compatibility=$4, metadata=$5, allowed_tools=$6 WHERE id=$1",
                &[&id, &meta.description, &meta.license, &meta.compatibility, &metadata, &meta.allowed_tools],
            )
            .map_err(dberr)?;
            if n == 0 {
                return Err(Error::NotFound(format!("skill {id}")));
            }
            Ok(())
        })
    }

    fn insert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()> {
        let (skill_id, name, head) = (skill_id.to_string(), name.to_string(), head.map(str::to_string));
        self.call(move |c| {
            c.execute("INSERT INTO branches (skill_id, name, head) VALUES ($1,$2,$3)", &[&skill_id, &name, &head])
                .map_err(dberr)?;
            Ok(())
        })
    }

    fn upsert_branch(&self, skill_id: &str, name: &str, head: Option<&str>) -> Result<()> {
        let (skill_id, name, head) = (skill_id.to_string(), name.to_string(), head.map(str::to_string));
        self.call(move |c| {
            c.execute(
                "INSERT INTO branches (skill_id, name, head) VALUES ($1,$2,$3) \
                 ON CONFLICT (skill_id, name) DO UPDATE SET head = excluded.head",
                &[&skill_id, &name, &head],
            )
            .map_err(dberr)?;
            Ok(())
        })
    }

    fn set_branch_head(&self, skill_id: &str, name: &str, head: &str) -> Result<()> {
        let (skill_id, name, head) = (skill_id.to_string(), name.to_string(), head.to_string());
        self.call(move |c| {
            c.execute("UPDATE branches SET head=$3 WHERE skill_id=$1 AND name=$2", &[&skill_id, &name, &head])
                .map_err(dberr)?;
            Ok(())
        })
    }

    fn get_branch(&self, skill_id: &str, name: &str) -> Result<Branch> {
        let (skill_id, name) = (skill_id.to_string(), name.to_string());
        self.call(move |c| {
            c.query_opt("SELECT skill_id, name, head FROM branches WHERE skill_id=$1 AND name=$2", &[&skill_id, &name])
                .map_err(dberr)?
                .map(|r| row_to_branch(&r))
                .ok_or_else(|| Error::NotFound(format!("branch {name} of skill {skill_id}")))
        })
    }

    fn list_branches(&self, skill_id: &str) -> Result<Vec<Branch>> {
        let skill_id = skill_id.to_string();
        self.call(move |c| {
            let rows = c
                .query("SELECT skill_id, name, head FROM branches WHERE skill_id=$1 ORDER BY name", &[&skill_id])
                .map_err(dberr)?;
            Ok(rows.iter().map(row_to_branch).collect())
        })
    }

    fn route(&self, query: &str, top_k: usize) -> Result<Vec<RouteResult>> {
        if top_k == 0 {
            return Ok(Vec::new());
        }
        let query = query.to_string();
        let top = top_k as i64;
        self.call(move |c| {
            let rows = c
                .query(
                    "SELECT s.id, s.name, s.description, ts_rank(s.fts, q) AS score \
                     FROM skills s, plainto_tsquery('english', $1) q \
                     WHERE s.fts @@ q ORDER BY score DESC LIMIT $2",
                    &[&query, &top],
                )
                .map_err(dberr)?;
            Ok(rows
                .iter()
                .map(|r| RouteResult {
                    skill_id: r.get(0),
                    name: r.get(1),
                    description: r.get(2),
                    score: r.get::<_, f32>(3) as f64,
                })
                .collect())
        })
    }

    fn insert_feedback(&self, f: &Feedback) -> Result<()> {
        let f = f.clone();
        self.call(move |c| {
            c.execute(
                "INSERT INTO feedback (id, skill_id, commit_id, source, rating, content, query, status, created_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                &[&f.id, &f.skill_id, &f.commit_id, &f.source, &f.rating, &f.content, &f.query, &f.status, &f.created_at],
            )
            .map_err(dberr)?;
            Ok(())
        })
    }

    fn list_feedback(&self, skill_id: &str) -> Result<Vec<Feedback>> {
        let skill_id = skill_id.to_string();
        self.call(move |c| {
            let rows = c
                .query(
                    "SELECT id, skill_id, commit_id, source, rating, content, query, status, created_at \
                     FROM feedback WHERE skill_id=$1 ORDER BY created_at DESC",
                    &[&skill_id],
                )
                .map_err(dberr)?;
            Ok(rows.iter().map(row_to_feedback).collect())
        })
    }

    fn set_feedback_status(&self, id: &str, status: &str) -> Result<()> {
        let (id, status) = (id.to_string(), status.to_string());
        self.call(move |c| {
            let n = c.execute("UPDATE feedback SET status=$2 WHERE id=$1", &[&id, &status]).map_err(dberr)?;
            if n == 0 {
                return Err(Error::NotFound(format!("feedback {id}")));
            }
            Ok(())
        })
    }

    fn mark_feedback_applied(&self, skill_id: &str) -> Result<()> {
        let skill_id = skill_id.to_string();
        self.call(move |c| {
            c.execute("UPDATE feedback SET status='applied' WHERE skill_id=$1 AND status='open'", &[&skill_id])
                .map_err(dberr)?;
            Ok(())
        })
    }

    fn insert_user(&self, u: &User) -> Result<()> {
        let u = u.clone();
        self.call(move |c| {
            c.execute(
                "INSERT INTO users (id, username, password_hash, role, created_at) VALUES ($1,$2,$3,$4,$5)",
                &[&u.id, &u.username, &u.password_hash, &u.role, &u.created_at],
            )
            .map_err(dberr)?;
            Ok(())
        })
    }

    fn get_user_by_username(&self, username: &str) -> Result<User> {
        let username = username.to_string();
        self.call(move |c| {
            c.query_opt("SELECT id, username, password_hash, role, created_at FROM users WHERE username=$1", &[&username])
                .map_err(dberr)?
                .map(|r| User {
                    id: r.get(0),
                    username: r.get(1),
                    password_hash: r.get(2),
                    role: r.get(3),
                    created_at: r.get(4),
                })
                .ok_or_else(|| Error::NotFound(format!("user {username}")))
        })
    }

    fn count_users(&self) -> Result<i64> {
        self.call(move |c| Ok(c.query_one("SELECT COUNT(*) FROM users", &[]).map_err(dberr)?.get(0)))
    }
}
