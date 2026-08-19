//! Direct, bounded lookup of recent community fire reports.
//!
//! `FeuxDeForet` is a useful terrain signal, not an official authority. A
//! matching report may therefore support `probable`, never `confirmed`.

use chrono::{DateTime, Utc};
use reqwest::{Client, Url};
use serde::Deserialize;
use unicode_normalization::{UnicodeNormalization as _, char::is_combining_mark};

const BASE_URL: &str = "https://feuxdeforet.fr";
const PAGE_SIZE: usize = 40;
const MAX_PAGES: usize = 3;

#[derive(Clone)]
pub struct FeuxDeForetClient {
    client: Client,
}

#[derive(Clone, Debug)]
pub struct FeuxDeForetReport {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub occurred_at: DateTime<Utc>,
}

impl FeuxDeForetClient {
    pub fn new() -> Result<Self, FeuxDeForetError> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .user_agent("FireSift-BLUE/1.0 (+https://github.com/supremexxx/firesift)")
                .build()?,
        })
    }

    pub async fn find_report(
        &self,
        commune: &str,
        department_code: Option<&str>,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) -> Result<Option<FeuxDeForetReport>, FeuxDeForetError> {
        let expected_commune = normalize_text(commune);
        for page in 0..MAX_PAGES {
            let mut url = parse_url(&format!("{BASE_URL}/api/signalements/recent"))?;
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("per", &PAGE_SIZE.to_string())
                    .append_pair("offset", &(page * PAGE_SIZE).to_string());
                if let Some(department_code) = department_code {
                    query.append_pair("dept", department_code);
                }
            }
            let recent: RecentResponse = self.get_json(url).await?;
            if recent.signalements.is_empty() {
                break;
            }
            for entry in recent.signalements {
                if normalize_text(&entry.commune) != expected_commune {
                    continue;
                }
                let resolved = self.resolve(&entry.url).await?;
                if resolved.report_type != "signalement"
                    || is_false_alert(&resolved.data.title)
                    || resolved.data.date < window_start
                    || resolved.data.date > window_end
                {
                    continue;
                }
                return Ok(Some(FeuxDeForetReport {
                    id: resolved.data.id,
                    title: resolved.data.title,
                    url: absolute_url(&resolved.data.url)?,
                    occurred_at: resolved.data.date,
                }));
            }
            if (page + 1) * PAGE_SIZE >= recent.total {
                break;
            }
        }
        Ok(None)
    }

    async fn resolve(&self, path: &str) -> Result<ResolveResponse, FeuxDeForetError> {
        let mut url = parse_url(&format!("{BASE_URL}/api/resolve"))?;
        url.query_pairs_mut()
            .append_pair("path", path)
            .append_pair("page", "1");
        self.get_json(url).await
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        url: Url,
    ) -> Result<T, FeuxDeForetError> {
        let response = self.client.get(url).send().await?.error_for_status()?;
        Ok(response.json().await?)
    }
}

#[derive(Deserialize)]
struct RecentResponse {
    total: usize,
    signalements: Vec<RecentReport>,
}

#[derive(Deserialize)]
struct RecentReport {
    commune: String,
    url: String,
}

#[derive(Deserialize)]
struct ResolveResponse {
    #[serde(rename = "type")]
    report_type: String,
    data: ResolvedReport,
}

#[derive(Deserialize)]
struct ResolvedReport {
    id: i64,
    title: String,
    date: DateTime<Utc>,
    url: String,
}

fn absolute_url(path: &str) -> Result<String, FeuxDeForetError> {
    parse_url(BASE_URL)?
        .join(path)
        .map(|url| url.to_string())
        .map_err(|error| FeuxDeForetError::InvalidUrl(error.to_string()))
}

fn parse_url(value: &str) -> Result<Url, FeuxDeForetError> {
    Url::parse(value).map_err(|error| FeuxDeForetError::InvalidUrl(error.to_string()))
}

fn is_false_alert(title: &str) -> bool {
    normalize_text(title).contains("fausse alerte")
}

fn normalize_text(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, thiserror::Error)]
pub enum FeuxDeForetError {
    #[error("FeuxDeForet HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("FeuxDeForet URL error: {0}")]
    InvalidUrl(String),
}

#[cfg(test)]
mod tests {
    use super::{absolute_url, is_false_alert, normalize_text};

    #[test]
    fn detects_false_alert_after_normalization() {
        assert!(is_false_alert("Incendie à Manosque : FAUSSE ALERTE"));
        assert!(!is_false_alert(
            "Incendie à Chandolas : le feu est maîtrisé"
        ));
    }

    #[test]
    fn normalizes_commune_names_without_losing_boundaries() {
        assert_eq!(
            normalize_text("Saint-Martin-d’Uriage"),
            "saint martin d uriage"
        );
    }

    #[test]
    fn builds_canonical_public_url() {
        assert_eq!(
            absolute_url("/ardeche-07/chandolas-19-08-2026-10671/").expect("URL"),
            "https://feuxdeforet.fr/ardeche-07/chandolas-19-08-2026-10671/"
        );
    }
}
