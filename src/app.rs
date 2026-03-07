use std::{io, io::Write, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use reqwest::{header::HeaderValue, Client};
use tokio::fs;

use crate::{
    browser::open_url,
    chesscom,
    clipboard,
    lichess,
};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub copy: bool,
    pub force_open: bool,
    pub no_open: bool,
    pub print_pgn: bool,
    pub save_pgn: Option<PathBuf>,
    pub raw_url: bool,
}

impl RunOptions {
    pub fn should_open(&self) -> bool {
        if self.no_open {
            false
        } else {
            self.force_open || !self.no_open
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub game_id: String,
    pub pgn: String,
    pub analysis_url: String,
}

impl AnalysisResult {
    pub fn final_analysis_url(&self) -> String {
        self.analysis_url.clone()
    }
}

fn build_http_client() -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("text/html,application/json;q=0.9,*/*;q=0.8"),
    );
    headers.insert(
        reqwest::header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    Client::builder()
        .default_headers(headers)
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
             AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/131.0.0.0 Safari/537.36",
        )
        .build()
        .map_err(|e| anyhow!(format!("Failed to create HTTP client: {e}")))
}

pub async fn resolve_with_progress<F>(
    url: &str,
    mut progress: F,
) -> Result<AnalysisResult>
where
    F: FnMut(&str),
{
    let client = build_http_client()?;

    progress("[1/5] Validating URL");
    let game = chesscom::parse_chesscom_game_url(url)?;
    progress("[1/5] URL validation complete");

    progress("[2/5] chess.com game lookup complete");

    progress("[3/5] Starting PGN extraction");
    let pgn = chesscom::fetch_game_pgn(&client, &game)
        .await
        .context("Failed to extract PGN")?;
    progress("[3/5] PGN extraction complete");

    progress("[4/5] Starting lichess API import");
    let analysis_url = lichess::import_via_api(&client, &pgn)
        .await
        .context("lichess API import failed")?;
    progress("[4/5] lichess API import succeeded");

    progress("[5/5] Done");

    Ok(AnalysisResult {
        game_id: game.game_id,
        pgn,
        analysis_url,
    })
}

pub async fn run_once(raw_url: String, options: RunOptions) -> Result<()> {
    let result = resolve_with_progress(&raw_url, |msg| {
        if !options.raw_url {
            println!("{msg}");
        }
    })
    .await?;

    finalize_output(&result, &options).await
}

pub async fn run_interactive(options: RunOptions) -> Result<()> {
    println!("Enter a chess.com game URL");
    print!("> ");
    io::stdout().flush().context("Failed to print input prompt")?;

    let mut raw = String::new();
    io::stdin().read_line(&mut raw)?;

    let url = raw.trim().to_string();
    if url.is_empty() {
        return Err(anyhow!("URL is empty."));
    }

    run_once(url, options).await
}

async fn finalize_output(result: &AnalysisResult, options: &RunOptions) -> Result<()> {
    let final_url = result.final_analysis_url();

    if options.copy {
        if let Err(err) = clipboard::copy_to_clipboard(&result.pgn) {
            eprintln!("Failed to copy PGN: {err}");
        } else if !options.raw_url {
            println!("PGN copied");
        }
    }

    if let Some(path) = &options.save_pgn {
        fs::write(path, &result.pgn)
            .await
            .with_context(|| format!("Failed to save PGN: {}", path.display()))?;
        if !options.raw_url {
            println!("PGN saved: {}", path.display());
        }
    }

    if options.print_pgn {
        if !options.raw_url {
            println!("\n=== PGN ===");
        }
        println!("{}", result.pgn);
    }

    if options.raw_url {
        println!("{}", final_url);
    } else {
        println!("Final URL: {final_url}");
        println!("Acquired via: lichess API import");
    }

    if options.should_open() {
        if let Err(err) = open_url(&final_url) {
            eprintln!("Failed to auto-open browser: {err}");
        }
    }

    Ok(())
}
