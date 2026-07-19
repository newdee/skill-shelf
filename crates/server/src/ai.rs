//! Optional AI refine + rerank via an OpenAI-compatible `/chat/completions`
//! endpoint. The (base, key, model) are resolved from the hot-reloadable runtime
//! config by the caller and passed in.

use serde::Deserialize;
use serde_json::json;
use skill_shelf_core::Feedback;

use crate::error::ApiError;

/// Resolved OpenAI-compatible settings: (base_url, api_key, model).
pub type AiConfig = (String, String, String);

/// One OpenAI-compatible chat call returning the assistant message content.
async fn chat(ai: &AiConfig, system: &str, user: &str, temperature: f64) -> Result<String, ApiError> {
    let (base, key, model) = ai;
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .bearer_auth(&key)
        .json(&json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user }
            ],
            "temperature": temperature
        }))
        .send()
        .await
        .map_err(|e| ApiError::internal(format!("AI request failed: {e}")))?;
    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(ApiError::internal(format!("AI returned {code}: {body}")));
    }
    let data: ChatResp = resp
        .json()
        .await
        .map_err(|e| ApiError::internal(format!("AI response parse failed: {e}")))?;
    data.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .ok_or_else(|| ApiError::internal("AI returned no choices"))
}

/// Rerank candidate skills for a query. Returns relevant skill ids, most
/// relevant first (empty if none). Reuses the OpenAI-compatible endpoint.
pub async fn rerank(ai: &AiConfig, query: &str, candidates: &[(String, String, String)]) -> Result<Vec<String>, ApiError> {
    let list = candidates
        .iter()
        .map(|(id, name, desc)| format!("- id={id} | {name}: {desc}"))
        .collect::<Vec<_>>()
        .join("\n");
    let system = "You are a router for a skill registry. Given a user's need and candidate \
                  skills, decide which are genuinely relevant. Reply with ONLY a JSON array of \
                  the matching `id` strings, most relevant first. Use [] if none fit. No prose, \
                  no code fences.";
    let user = format!("Need: {query}\n\nCandidates:\n{list}");
    let raw = chat(ai, system, &user, 0.0).await?;
    let cleaned = strip_fences(&raw);
    let ids: Vec<String> = serde_json::from_str(&cleaned)
        .map_err(|e| ApiError::internal(format!("rerank: bad JSON from model: {e}")))?;
    // Guard against hallucinated ids: keep only real candidates, preserve order.
    let valid: std::collections::HashSet<&str> = candidates.iter().map(|c| c.0.as_str()).collect();
    Ok(ids.into_iter().filter(|id| valid.contains(id.as_str())).collect())
}

/// Produce an improved SKILL.md from the current content + open feedback.
pub async fn refine_skillmd(ai: &AiConfig, current: &str, feedback: &[Feedback]) -> Result<String, ApiError> {
    let fb_text = feedback
        .iter()
        .map(|f| format!("- [{:+}] {}", f.rating, f.content))
        .collect::<Vec<_>>()
        .join("\n");
    let system = "You improve Claude Agent Skill SKILL.md files. Return ONLY the full revised \
                  SKILL.md content: valid YAML frontmatter (keep the same `name`; keep a \
                  non-empty `description`) followed by the markdown body. Address the feedback. \
                  No explanations, no code fences.";
    let user = format!("Current SKILL.md:\n\n{current}\n\nFeedback to address:\n{fb_text}");
    let content = chat(ai, system, &user, 0.3).await?;
    Ok(strip_fences(&content))
}

#[derive(Deserialize)]
struct ChatResp {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: Msg,
}
#[derive(Deserialize)]
struct Msg {
    content: String,
}

/// Drop a ```-fenced wrapper if the model added one despite instructions.
fn strip_fences(s: &str) -> String {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```") {
        let rest = rest.splitn(2, '\n').nth(1).unwrap_or(rest);
        let rest = rest.trim_end().strip_suffix("```").unwrap_or(rest);
        return rest.trim().to_string();
    }
    t.to_string()
}
