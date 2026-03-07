use anyhow::{anyhow, Result};
use reqwest::{header::LOCATION, Client, StatusCode};
use serde_json::Value;

fn normalize_lichess_url(candidate: &str) -> Option<String> {
    if candidate.starts_with("https://lichess.org/") {
        return Some(candidate.to_string());
    }
    if candidate.starts_with('/') {
        return Some(format!("https://lichess.org{candidate}"));
    }
    None
}

fn extract_analysis_url(body: &str, location: Option<&str>, final_url: Option<&str>) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(url) = v.get("url").and_then(Value::as_str)
            && let Some(normalized) = normalize_lichess_url(url)
        {
            return Some(normalized);
        }
        if let Some(id) = v.get("id").and_then(Value::as_str) {
            return Some(format!("https://lichess.org/{id}"));
        }
    }

    if let Some(loc) = location
        && let Some(normalized) = normalize_lichess_url(loc)
    {
        return Some(normalized);
    }

    let trimmed = body.trim();
    if let Some(normalized) = normalize_lichess_url(trimmed) {
        return Some(normalized);
    }

    if let Some(url) = final_url
        && let Some(normalized) = normalize_lichess_url(url)
        && normalized != "https://lichess.org/api/import"
    {
        return Some(normalized);
    }

    None
}

pub async fn import_via_api(client: &Client, pgn: &str) -> Result<String> {
    if pgn.trim().is_empty() {
        return Err(anyhow!("Cannot import an empty PGN to lichess."));
    }

    let response = client
        .post("https://lichess.org/api/import")
        .form(&[("pgn", pgn)])
        .send()
        .await
        .map_err(|e| anyhow!(format!("lichess import API request failed: {e}")))?;

    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(anyhow!("lichess import API access was denied: {status}"));
    }

    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|h| h.to_str().ok())
        .map(std::string::ToString::to_string);
    let final_url = response.url().to_string();
    let body = response.text().await.unwrap_or_default();

    if let Some(url) = extract_analysis_url(&body, location.as_deref(), Some(&final_url)) {
        return Ok(url);
    }

    Err(anyhow!(format!(
        "Could not find a final URL in the lichess import API response (status: {status})"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_id_response() {
        let body = serde_json::json!({"id": "abc"}).to_string();
        assert_eq!(
            extract_analysis_url(&body, None, None),
            Some("https://lichess.org/abc".to_string())
        );
    }

    #[test]
    fn parse_url_response() {
        let body = serde_json::json!({"url": "https://lichess.org/analysis/xyz"}).to_string();
        assert_eq!(
            extract_analysis_url(&body, None, None),
            Some("https://lichess.org/analysis/xyz".to_string())
        );
    }

    #[test]
    fn parse_relative_location() {
        assert_eq!(
            extract_analysis_url("", Some("/xYNyKyLW"), None),
            Some("https://lichess.org/xYNyKyLW".to_string())
        );
    }
}
