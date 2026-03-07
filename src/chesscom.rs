use anyhow::{anyhow, Context, Result};
use reqwest::{Client, StatusCode};
use regex::Regex;
use serde_json::Value;
use url::Url;

use crate::error::C2lError;

#[derive(Debug, Clone)]
pub struct ChessGameRef {
    pub source_url: Url,
    pub game_id: String,
}

pub fn is_supported_chesscom_game_url(url: &Url) -> bool {
    matches!(url.host_str(), Some("chess.com") | Some("www.chess.com")) && url.scheme().starts_with("http")
}

pub fn parse_game_id(url: &Url) -> Option<String> {
    let segments: Vec<_> = url
        .path_segments()
        .map(|parts| parts.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    if segments.len() < 3 {
        return None;
    }

    if segments[0] != "game" || segments[1] != "live" {
        return None;
    }

    let id = segments[2];
    let id_re = Regex::new(r"^[A-Za-z0-9-]+$").unwrap();
    if id_re.is_match(id) { Some(id.to_string()) } else { None }
}

pub fn parse_chesscom_game_url(raw: &str) -> Result<ChessGameRef> {
    let url = Url::parse(raw).with_context(|| C2lError::InvalidUrl(raw.to_string()).to_string())?;
    if !is_supported_chesscom_game_url(&url) {
        return Err(anyhow!(C2lError::UnsupportedUrl(raw.to_string())));
    }

    let game_id = parse_game_id(&url).ok_or_else(|| anyhow!(C2lError::UnsupportedUrl(raw.to_string())))?;
    Ok(ChessGameRef { source_url: url, game_id })
}

fn extract_pgn_from_json_value(value: &Value) -> Option<String> {
    match value {
        Value::Object(obj) => {
            if let Some(pgn) = obj.get("pgn").and_then(Value::as_str) {
                if !pgn.trim().is_empty() {
                    return Some(pgn.to_string());
                }
            }
            for v in obj.values() {
                if let Some(found) = extract_pgn_from_json_value(v) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(found) = extract_pgn_from_json_value(item) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn extract_pgn_from_jsonish(text: &str) -> Option<String> {
    let full = serde_json::from_str::<Value>(text).ok()?;
    extract_pgn_from_json_value(&full)
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn decode_quoted_json_string(raw: &str) -> Option<String> {
    serde_json::from_str(&format!("\"{}\"", decode_html_entities(raw))).ok()
}

fn extract_pgn_from_key_value(text: &str) -> Option<String> {
    const PATTERNS: [&str; 4] = [
        r#""pgn"\s*:\s*"((?:\\.|[^\\"])+)""#,
        r#""pgn"\s*:\s*'((?:\\.|[^\\'])+)'"#,
        r#"'pgn'\s*:\s*"((?:\\.|[^\\"])+)""#,
        r#"'pgn'\s*:\s*'((?:\\.|[^\\'])+)'"#,
    ];

    for pattern in PATTERNS {
        let re = Regex::new(pattern).unwrap();
        if let Some(cap) = re.captures(text) {
            let raw = cap.get(1)?.as_str();
            if let Some(pgn) = decode_quoted_json_string(raw) {
                return Some(pgn);
            }
        }
    }

    None
}

fn extract_pgn_from_data_attr(text: &str) -> Option<String> {
    const DATA_PATTERNS: [&str; 2] = [r#"data-pgn="([^"]+)""#, r#"data-pgn='([^']+)'"#];

    for pattern in DATA_PATTERNS {
        let re = Regex::new(pattern).unwrap();
        if let Some(cap) = re.captures(text) {
            let raw = cap.get(1)?.as_str();
            if let Some(pgn) = decode_quoted_json_string(raw) {
                return Some(pgn);
            }
        }
    }

    None
}

fn extract_pgn_from_embedded_json(value: &str) -> Option<String> {
    if let Ok(v) = serde_json::from_str::<Value>(value) {
        return extract_pgn_from_json_value(&v);
    }
    None
}

fn extract_pgn_from_embedded_assignment(text: &str) -> Option<String> {
    let re = Regex::new(
        r#"(?s)(?:window\.[A-Za-z0-9_$.\[\]]+|const\s+[A-Za-z0-9_$]+|let\s+[A-Za-z0-9_$]+|var\s+[A-Za-z0-9_$]+)\s*=\s*(\{[\s\S]*?\}|\[[\s\S]*?\])\s*;?"#,
    )
    .unwrap();

    for cap in re.captures_iter(text) {
        let raw = cap.get(1)?.as_str();
        if let Some(pgn) = extract_pgn_from_embedded_json(raw) {
            return Some(pgn);
        }
    }

    None
}

fn extract_pgn_from_scripts(text: &str) -> Option<String> {
    let script_re = Regex::new(r#"(?is)<script\b[^>]*>(.*?)</script>"#).unwrap();
    for cap in script_re.captures_iter(text) {
        let script = cap.get(1)?.as_str();
        if let Some(pgn) = extract_pgn_from_embedded_json(script) {
            return Some(pgn);
        }
        if let Some(pgn) = extract_pgn_from_key_value(script) {
            return Some(pgn);
        }
        if let Some(pgn) = extract_pgn_from_embedded_assignment(script) {
            return Some(pgn);
        }
    }

    None
}

fn extract_pgn_from_body(text: &str) -> Option<String> {
    if let Some(pgn) = extract_pgn_from_embedded_json(text) {
        return Some(pgn);
    }
    if let Some(pgn) = extract_pgn_from_jsonish(text) {
        return Some(pgn);
    }
    if let Some(pgn) = extract_pgn_from_key_value(text) {
        return Some(pgn);
    }
    if let Some(pgn) = extract_pgn_from_data_attr(text) {
        return Some(pgn);
    }
    extract_pgn_from_scripts(text).or_else(|| extract_pgn_from_embedded_assignment(text))
}

fn cleanup_pgn(pgn: String) -> String {
    pgn
        .replace("\r\n", "\n")
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn looks_like_pgn(pgn: &str) -> bool {
    let has_headers = pgn.contains("[Event ") && pgn.contains("[White ") && pgn.contains("[Black ");
    let has_moves = pgn.contains(" 1.") || pgn.contains("1.") || pgn.contains("1...");
    let has_result = pgn.contains("[Result ");
    has_headers && has_result && has_moves
}

fn parse_year_month(date: &str) -> Option<(i32, u32)> {
    let re = Regex::new(r"^(\d{4})\.(\d{2})\.(\d{2})$").unwrap();
    let cap = re.captures(date.trim())?;
    let year = cap.get(1)?.as_str().parse::<i32>().ok()?;
    let month = cap.get(2)?.as_str().parse::<u32>().ok()?;
    if (1..=12).contains(&month) {
        Some((year, month))
    } else {
        None
    }
}

fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 { (year - 1, 12) } else { (year, month - 1) }
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 { (year + 1, 1) } else { (year, month + 1) }
}

fn month_candidates(year: i32, month: u32) -> Vec<(i32, u32)> {
    let prev = prev_month(year, month);
    let next = next_month(year, month);
    vec![(year, month), prev, next]
}

fn push_unique(vec: &mut Vec<String>, value: Option<&str>) {
    if let Some(s) = value {
        let trimmed = s.trim();
        if !trimmed.is_empty() && !vec.iter().any(|x| x == trimmed) {
            vec.push(trimmed.to_string());
        }
    }
}

fn game_url_matches_id(url: &str, game_id: &str) -> bool {
    url.contains(&format!("/game/live/{game_id}"))
}

#[derive(Debug, Clone)]
struct ArchiveLookup {
    usernames: Vec<String>,
    year: i32,
    month: u32,
}

fn extract_archive_lookup(callback_json: &Value) -> Option<ArchiveLookup> {
    let mut usernames = Vec::new();
    push_unique(
        &mut usernames,
        callback_json
            .get("players")
            .and_then(|v| v.get("top"))
            .and_then(|v| v.get("username"))
            .and_then(Value::as_str),
    );
    push_unique(
        &mut usernames,
        callback_json
            .get("players")
            .and_then(|v| v.get("bottom"))
            .and_then(|v| v.get("username"))
            .and_then(Value::as_str),
    );
    push_unique(
        &mut usernames,
        callback_json
            .get("game")
            .and_then(|v| v.get("pgnHeaders"))
            .and_then(|v| v.get("White"))
            .and_then(Value::as_str),
    );
    push_unique(
        &mut usernames,
        callback_json
            .get("game")
            .and_then(|v| v.get("pgnHeaders"))
            .and_then(|v| v.get("Black"))
            .and_then(Value::as_str),
    );

    if usernames.is_empty() {
        return None;
    }

    let date = callback_json
        .get("game")
        .and_then(|v| v.get("pgnHeaders"))
        .and_then(|v| v.get("Date"))
        .and_then(Value::as_str)?;
    let (year, month) = parse_year_month(date)?;

    Some(ArchiveLookup {
        usernames,
        year,
        month,
    })
}

async fn fetch_callback_body(client: &Client, game_id: &str) -> Result<String> {
    let endpoint = format!("https://www.chess.com/callback/live/game/{game_id}");
    let response = client
        .get(&endpoint)
        .send()
        .await
        .with_context(|| format!("Callback endpoint request failed: {endpoint}"))?;

    if response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::UNAUTHORIZED {
        return Err(anyhow!(C2lError::PrivateOrUnavailable(format!(
            "{endpoint}"
        ))));
    }

    response
        .text()
        .await
        .with_context(|| format!("Failed to read callback response body: {endpoint}"))
}

async fn try_callback_endpoint(client: &Client, game_id: &str) -> Result<String> {
    let body = fetch_callback_body(client, game_id).await?;

    if let Some(pgn) = extract_pgn_from_body(&body) {
        return Ok(cleanup_pgn(pgn));
    }

    Err(anyhow!(C2lError::PgnUnavailable(format!(
        "PGN not found in callback: https://www.chess.com/callback/live/game/{game_id}"
    ))))
}

async fn try_page_scrape(client: &Client, page_url: &Url) -> Result<String> {
    let response = client
        .get(page_url.as_str())
        .send()
        .await
        .with_context(|| format!("Game page request failed: {}", page_url))?;

    if response.status() == StatusCode::FORBIDDEN || response.status() == StatusCode::UNAUTHORIZED {
        return Err(anyhow!(C2lError::PrivateOrUnavailable(format!(
            "Page access is restricted: {}",
            page_url
        ))));
    }

    let body = response.text().await?;

    if let Some(pgn) = extract_pgn_from_jsonish(&body) {
        return Ok(cleanup_pgn(pgn));
    }
    if let Some(pgn) = extract_pgn_from_body(&body) {
        return Ok(cleanup_pgn(pgn));
    }

    Err(anyhow!(C2lError::PgnUnavailable(
        "Could not extract PGN from the game page.".to_string()
    )))
}

async fn try_archive_month(
    client: &Client,
    username: &str,
    year: i32,
    month: u32,
    game_id: &str,
) -> Result<Option<String>> {
    let endpoint = format!("https://api.chess.com/pub/player/{username}/games/{year}/{month:02}");
    let response = client
        .get(&endpoint)
        .send()
        .await
        .with_context(|| format!("chess.com public API request failed: {endpoint}"))?;

    if response.status().is_client_error() || response.status().is_server_error() {
        return Ok(None);
    }

    let body = response
        .text()
        .await
        .with_context(|| format!("Failed to read chess.com public API response: {endpoint}"))?;
    let parsed = serde_json::from_str::<Value>(&body)
        .with_context(|| format!("Failed to parse chess.com public API JSON: {endpoint}"))?;
    let games = parsed.get("games").and_then(Value::as_array);
    let Some(games) = games else {
        return Ok(None);
    };

    for game in games {
        let url = game.get("url").and_then(Value::as_str).unwrap_or_default();
        if !game_url_matches_id(url, game_id) {
            continue;
        }
        if let Some(pgn) = game.get("pgn").and_then(Value::as_str) {
            let cleaned = cleanup_pgn(pgn.to_string());
            if !cleaned.is_empty() {
                return Ok(Some(cleaned));
            }
        }
    }

    Ok(None)
}

async fn try_archive_fallback(client: &Client, game_id: &str, lookup: &ArchiveLookup) -> Result<String> {
    for username in &lookup.usernames {
        for (year, month) in month_candidates(lookup.year, lookup.month) {
            if let Some(pgn) = try_archive_month(client, username, year, month, game_id).await? {
                return Ok(pgn);
            }
        }
    }

    Err(anyhow!(C2lError::PgnUnavailable(
        "PGN not found in public archive API.".to_string(),
    )))
}

pub async fn fetch_game_pgn(client: &Client, game: &ChessGameRef) -> Result<String> {
    let callback_body = fetch_callback_body(client, &game.game_id).await.ok();
    if let Some(body) = callback_body.as_deref() {
        if let Some(pgn) = extract_pgn_from_body(body).map(cleanup_pgn)
            && looks_like_pgn(&pgn)
        {
            return Ok(pgn);
        }
        if let Ok(callback_json) = serde_json::from_str::<Value>(body)
            && let Some(lookup) = extract_archive_lookup(&callback_json)
            && let Ok(pgn) = try_archive_fallback(client, &game.game_id, &lookup).await
            && looks_like_pgn(&pgn)
        {
            return Ok(pgn);
        }
    }

    if let Ok(pgn) = try_callback_endpoint(client, &game.game_id).await
        && looks_like_pgn(&pgn)
    {
        return Ok(pgn);
    }

    if let Ok(page_pgn) = try_page_scrape(client, &game.source_url).await
        && looks_like_pgn(&page_pgn)
    {
        return Ok(page_pgn);
    }

    Err(anyhow!(C2lError::PgnUnavailable(
        "Failed to retrieve PGN. callback/page/public archive all failed.".to_string(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_supported_url() {
        let parsed = parse_chesscom_game_url("https://www.chess.com/game/live/123456789").unwrap();
        assert_eq!(parsed.game_id, "123456789");
    }

    #[test]
    fn parse_unsupported_host() {
        let err = parse_chesscom_game_url("https://www.lichess.org/game/live/123").unwrap_err();
        assert!(err.to_string().contains("Unsupported"));
    }

    #[test]
    fn parse_bad_path() {
        let err = parse_chesscom_game_url("https://www.chess.com/pgn/123").unwrap_err();
        assert!(err.to_string().contains("Unsupported"));
    }

    #[test]
    fn validate_pgn_shape() {
        let good_with_result = "[Event \"x\"]\n[White \"a\"]\n[Black \"b\"]\n[Result \"1-0\"]\n1. e4 e5";
        let bad = "just text";
        assert!(looks_like_pgn(good_with_result));
        assert!(!looks_like_pgn(bad));
    }
}
