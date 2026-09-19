//! Optional relevance reranking of search results via the TypeSafe (Jev) API.
//!
//! Off unless `TYPESAFE_API_KEY` is set: search stays fully local and
//! offline otherwise. When enabled, candidate names/descriptions/text
//! excerpts are sent to TypeSafe's hosted API to judge relevance to the
//! query, so this should only be turned on for content acceptable to send
//! to that third party.
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MODEL: &str = "jev-latest";
const MAX_CANDIDATES: usize = 30;
const MAX_EXCERPT_CHARS: usize = 600;
const TIMEOUT: Duration = Duration::from_secs(8);

pub struct Candidate {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub path: String,
    pub text: String,
}

#[derive(Deserialize)]
struct ApiResponse {
    answers: HashMap<String, Answer>,
}
#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    noul: f64,
}

/// Returns candidate IDs reordered by relevance to `query`, or `None` if
/// reranking is unavailable (no API key, too many candidates, or a
/// network/response failure) so the caller can fall back to its own order.
pub fn rerank(query: &str, candidates: &[Candidate]) -> Option<Vec<String>> {
    if candidates.is_empty() || candidates.len() > MAX_CANDIDATES {
        return None;
    }
    let api_key = std::env::var("TYPESAFE_API_KEY").ok()?;
    let body = serde_json::json!({
        "state": state(query, candidates),
        "model": MODEL,
        "questions": questions(candidates.len()),
    });
    let response = ureq::post(ENDPOINT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .timeout(TIMEOUT)
        .send_json(body);
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            eprintln!("TypeSafe search ranking unavailable: {error}");
            return None;
        }
    };
    let payload: ApiResponse = match response.into_json() {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("TypeSafe search ranking response unreadable: {error}");
            return None;
        }
    };
    let mut scored: Vec<(usize, f64)> = (0..candidates.len())
        .map(|index| {
            let score = payload
                .answers
                .get(&question_key(index))
                .map(|answer| answer.noul)
                .unwrap_or(0.0);
            (index, score)
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    Some(
        scored
            .into_iter()
            .map(|(index, _)| candidates[index].id.clone())
            .collect(),
    )
}

fn question_key(index: usize) -> String {
    format!("match_{index}")
}

fn state(query: &str, candidates: &[Candidate]) -> serde_json::Value {
    serde_json::json!({
        "search_query": query,
        "candidates": candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| serde_json::json!({
                "index": index,
                "kind": candidate.kind,
                "name": candidate.name,
                "description": candidate.description,
                "path": candidate.path,
                "excerpt": truncate(&candidate.text, MAX_EXCERPT_CHARS),
            }))
            .collect::<Vec<_>>(),
    })
}

fn questions(count: usize) -> serde_json::Value {
    let mut map = serde_json::Map::with_capacity(count);
    for index in 0..count {
        map.insert(
            question_key(index),
            serde_json::json!({
                "type": "noul",
                "instructions": format!(
                    "In the `candidates` array of the state, does the item at index {index} plausibly match what the person is looking for, described in `search_query`? Judge by its name, description, and excerpt together."
                ),
                "criteria": {
                    "true": "The item's purpose, subject, or content relates closely to the search query.",
                    "false": "The item is unrelated to the search query beyond an incidental keyword.",
                },
            }),
        );
    }
    serde_json::Value::Object(map)
}

fn truncate(text: &str, max_chars: usize) -> &str {
    match text.char_indices().nth(max_chars) {
        Some((byte_index, _)) => &text[..byte_index],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_includes_searchable_identity_and_truncates_unicode_excerpts() {
        let candidate = Candidate {
            id: "one".into(),
            kind: "skill".into(),
            name: "Release helper".into(),
            description: "Prepares releases".into(),
            path: "/repo/.agents/skills/release/SKILL.md".into(),
            text: "é".repeat(MAX_EXCERPT_CHARS + 1),
        };
        let value = state("shipping", &[candidate]);
        let item = &value["candidates"][0];
        assert_eq!(item["path"], "/repo/.agents/skills/release/SKILL.md");
        assert_eq!(
            item["excerpt"].as_str().unwrap().chars().count(),
            MAX_EXCERPT_CHARS
        );
    }

    #[test]
    fn questions_are_independent_relevance_judgements() {
        let value = questions(2);
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert_eq!(value["match_0"]["type"], "noul");
        assert!(
            value["match_1"]["instructions"]
                .as_str()
                .unwrap()
                .contains("index 1")
        );
    }
}
