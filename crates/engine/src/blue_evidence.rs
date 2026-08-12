//! Bounded, cited web-evidence review for the daily BLUE showcase.

use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use store::{BlueEvidenceClaim, BlueEvidenceResult, BlueEvidenceSourceInput};

const RESPONSES_URL: &str = "https://api.openai.com/v1/responses";

#[derive(Clone)]
pub struct BlueEvidenceReviewer {
    client: Client,
    api_key: String,
    model: String,
}

impl BlueEvidenceReviewer {
    pub fn new(api_key: String, model: String) -> Result<Self, BlueEvidenceError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_mins(2))
            .build()?;
        Ok(Self {
            client,
            api_key,
            model,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn request_checksum(&self, claim: &BlueEvidenceClaim) -> String {
        let payload = self.request_payload(claim);
        format!("{:x}", Sha256::digest(payload.to_string().as_bytes()))
    }

    pub async fn review(
        &self,
        claim: &BlueEvidenceClaim,
    ) -> Result<BlueEvidenceResult, BlueEvidenceError> {
        let response = self
            .client
            .post(RESPONSES_URL)
            .bearer_auth(&self.api_key)
            .json(&self.request_payload(claim))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(BlueEvidenceError::Api(format!(
                "OpenAI Responses API returned {status}: {}",
                body.chars().take(300).collect::<String>()
            )));
        }
        parse_response(&body)
    }

    fn request_payload(&self, claim: &BlueEvidenceClaim) -> Value {
        let case_data = json!({
            "bulletin_date": claim.bulletin_date,
            "issued_at": claim.issued_at,
            "commune": claim.commune_name,
            "insee_code": claim.insee_code,
            "department_code": claim.department_code,
            "blue_daily_rank": claim.daily_rank,
            "blue_selection_score": claim.selection_score,
            "review_horizon": claim.review_horizon,
            "forecast_24h": {
                "index": claim.alert_24h_index,
                "valid_at": claim.alert_24h_valid_at,
            },
            "forecast_48h": {
                "index": claim.alert_48h_index,
                "valid_at": claim.alert_48h_valid_at,
            }
        });
        json!({
            "model": self.model,
            "tools": [{"type": "web_search"}],
            "include": ["web_search_call.action.sources"],
            "instructions": "Tu es le vérificateur de preuves de BLUE. Recherche sur le web des traces d'incendie ou de feu de végétation pour la commune dans la fenêtre qui commence à issued_at et se termine au valid_at du review_horizon demandé. Pour hours_24, produis un constat provisoire limité aux premières 24 heures. Pour hours_48, produis le constat final couvrant les 48 heures complètes, même si une première recherche a déjà eu lieu. Privilégie les sources datées et localisées (autorités, secours, presse locale crédible). Ne considère jamais une absence de résultat comme la preuve qu'aucun incendie n'a eu lieu. N'invente aucune source. Un verdict confirmed exige une preuve directe avec URL et concordance claire de lieu et de date; probable exige au moins une source crédible mais une concordance imparfaite; signal_observed exige une source réelle mais insuffisante; no_evidence_found signifie seulement que la recherche n'a rien trouvé et doit rester non concluant statistiquement. Réponds en français, sobrement.",
            "input": [{
                "role": "user",
                "content": format!("Vérifie ce dossier de prévision BLUE après échéance. Données du dossier: {case_data}")
            }],
            "text": {"format": evidence_schema()}
        })
    }
}

fn evidence_schema() -> Value {
    json!({
        "type": "json_schema",
        "name": "blue_evidence_review",
        "strict": true,
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "verdict": {"type": "string", "enum": ["signal_observed","probable","confirmed","no_evidence_found","inconclusive"]},
                "confidence": {"type": "number", "minimum": 0, "maximum": 1},
                "summary": {"type": "string", "maxLength": 1200},
                "observed_event_at": {"type": ["string","null"]},
                "observed_location": {"type": ["string","null"], "maxLength": 300},
                "evidence": {
                    "type": "array", "maxItems": 8,
                    "items": {
                        "type": "object", "additionalProperties": false,
                        "properties": {
                            "url": {"type": "string"},
                            "title": {"type": "string", "maxLength": 300},
                            "published_at": {"type": ["string","null"]},
                            "excerpt": {"type": ["string","null"], "maxLength": 500},
                            "relation_strength": {"type": "string", "enum": ["direct","corroborating","weak"]}
                        },
                        "required": ["url","title","published_at","excerpt","relation_strength"]
                    }
                }
            },
            "required": ["verdict","confidence","summary","observed_event_at","observed_location","evidence"]
        }
    })
}

#[derive(Deserialize)]
struct GeneratedReview {
    verdict: String,
    confidence: f32,
    summary: String,
    observed_event_at: Option<String>,
    observed_location: Option<String>,
    evidence: Vec<GeneratedEvidence>,
}

#[derive(Deserialize)]
struct GeneratedEvidence {
    url: String,
    title: String,
    published_at: Option<String>,
    excerpt: Option<String>,
    relation_strength: String,
}

fn parse_response(body: &str) -> Result<BlueEvidenceResult, BlueEvidenceError> {
    let raw: Value = serde_json::from_str(body)?;
    let text = raw
        .get("output")
        .and_then(Value::as_array)
        .and_then(|output| {
            output.iter().find_map(|item| {
                if item.get("type").and_then(Value::as_str) != Some("message") {
                    return None;
                }
                item.get("content")
                    .and_then(Value::as_array)
                    .and_then(|content| {
                        content.iter().find_map(|part| {
                            (part.get("type").and_then(Value::as_str) == Some("output_text"))
                                .then(|| part.get("text").and_then(Value::as_str))
                                .flatten()
                        })
                    })
            })
        })
        .ok_or(BlueEvidenceError::MissingOutput)?;
    let generated: GeneratedReview = serde_json::from_str(text)?;
    let mut sources = generated
        .evidence
        .into_iter()
        .filter_map(|item| normalize_source(item).ok())
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| left.url.cmp(&right.url));
    sources.dedup_by(|left, right| left.url == right.url);

    let has_direct = sources
        .iter()
        .any(|source| source.relation_strength == "direct");
    let verdict = match generated.verdict.as_str() {
        "confirmed" if has_direct => "confirmed",
        "confirmed" if !sources.is_empty() => "probable",
        "probable" | "signal_observed" if sources.is_empty() => "inconclusive",
        value => value,
    }
    .to_owned();
    let observed_event_at = generated
        .observed_event_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| BlueEvidenceError::InvalidOutput("invalid observed_event_at".to_owned()))?
        .map(|value| value.with_timezone(&Utc));
    let web_search_count = raw
        .get("output")
        .and_then(Value::as_array)
        .map_or(0, |output| {
            i64::try_from(
                output
                    .iter()
                    .filter(|item| {
                        item.get("type").and_then(Value::as_str) == Some("web_search_call")
                    })
                    .count(),
            )
            .unwrap_or(i64::MAX)
        });
    Ok(BlueEvidenceResult {
        verdict,
        confidence: generated.confidence.clamp(0.0, 1.0),
        summary: generated.summary.chars().take(1_200).collect(),
        observed_event_at,
        observed_location: generated
            .observed_location
            .map(|value| value.chars().take(300).collect()),
        response_id: raw
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        input_tokens: raw.pointer("/usage/input_tokens").and_then(Value::as_i64),
        output_tokens: raw.pointer("/usage/output_tokens").and_then(Value::as_i64),
        web_search_count,
        sources,
        raw_response: raw,
    })
}

fn normalize_source(
    source: GeneratedEvidence,
) -> Result<BlueEvidenceSourceInput, BlueEvidenceError> {
    let parsed = Url::parse(&source.url)
        .map_err(|_| BlueEvidenceError::InvalidOutput("invalid evidence URL".to_owned()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(BlueEvidenceError::InvalidOutput(
            "unsupported evidence URL scheme".to_owned(),
        ));
    }
    let domain = parsed
        .host_str()
        .ok_or_else(|| BlueEvidenceError::InvalidOutput("evidence URL has no host".to_owned()))?
        .to_owned();
    let published_at = source
        .published_at
        .as_deref()
        .map(DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|_| BlueEvidenceError::InvalidOutput("invalid source published_at".to_owned()))?
        .map(|value| value.with_timezone(&Utc));
    Ok(BlueEvidenceSourceInput {
        url: parsed.to_string().chars().take(2_000).collect(),
        title: source.title.chars().take(300).collect(),
        published_at,
        excerpt: source
            .excerpt
            .map(|value| value.chars().take(500).collect()),
        domain,
        relation_strength: source.relation_strength,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum BlueEvidenceError {
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid JSON response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("OpenAI API error: {0}")]
    Api(String),
    #[error("OpenAI response contained no structured output")]
    MissingOutput,
    #[error("invalid evidence output: {0}")]
    InvalidOutput(String),
}

#[cfg(test)]
mod tests {
    use super::parse_response;

    #[test]
    fn downgrades_confirmation_without_direct_evidence() {
        let body = serde_json::json!({
            "id":"resp_test","usage":{"input_tokens":12,"output_tokens":8},
            "output":[{"type":"web_search_call"},{"type":"message","content":[{
                "type":"output_text","text":serde_json::json!({
                    "verdict":"confirmed","confidence":0.9,"summary":"Signal local.",
                    "observed_event_at":null,"observed_location":"Commune test",
                    "evidence":[{"url":"https://example.org/fire","title":"Feu local",
                        "published_at":null,"excerpt":"Un signal.","relation_strength":"weak"}]
                }).to_string()
            }]}]
        })
        .to_string();
        let result = parse_response(&body).expect("valid response");
        assert_eq!(result.verdict, "probable");
        assert_eq!(result.web_search_count, 1);
        assert_eq!(result.sources.len(), 1);
    }

    #[test]
    fn evidence_free_probability_is_inconclusive() {
        let body = serde_json::json!({
            "id":"resp_test","output":[{"type":"message","content":[{
                "type":"output_text","text":serde_json::json!({
                    "verdict":"probable","confidence":0.4,"summary":"Sans source.",
                    "observed_event_at":null,"observed_location":null,"evidence":[]
                }).to_string()
            }]}]
        })
        .to_string();
        let result = parse_response(&body).expect("valid response");
        assert_eq!(result.verdict, "inconclusive");
    }
}
