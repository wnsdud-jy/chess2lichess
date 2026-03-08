use std::{
    error::Error as StdError,
    future::Future,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use reqwest::{Client, StatusCode, header::HeaderValue};
use serde::Serialize;
use tokio::{fs, time::sleep};

use crate::{browser::open_url, chesscom, clipboard, error::C2lError, lichess};

const MAX_RETRY_ATTEMPTS: u8 = 4;
const BASE_RETRY_DELAY_MS: u64 = 250;
const MAX_RETRY_DELAY_MS: u64 = 2500;

#[derive(Clone, Copy)]
enum TextTone {
    Info,
    Success,
    Warn,
    Error,
    Muted,
}

const WHITE_GRADIENT_FRAMES: [u8; 8] = [238, 242, 246, 249, 252, 255, 252, 249];

fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    io::stdout().is_terminal() || io::stderr().is_terminal()
}

fn colorize(text: &str, tone: TextTone) -> String {
    if !color_enabled() {
        return text.to_string();
    }

    let code = match tone {
        TextTone::Info => "94",
        TextTone::Success => "92",
        TextTone::Warn => "93",
        TextTone::Error => "91",
        TextTone::Muted => "90",
    };

    format!("\x1b[{code}m{text}\x1b[0m")
}

fn clear_line() {
    print!("\r\x1b[K");
}

fn gray_shade(text: &str, shade: u8) -> String {
    if !color_enabled() {
        return text.to_string();
    }

    format!("\x1b[38;5;{shade}m{text}\x1b[0m")
}

fn gradient_text(text: &str, frame: usize) -> String {
    if !color_enabled() {
        return text.to_string();
    }

    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        let shade = WHITE_GRADIENT_FRAMES[(idx + frame) % WHITE_GRADIENT_FRAMES.len()];
        out.push_str(&gray_shade(&ch.to_string(), shade));
    }
    out
}

fn muted_step_label(step: u8, total: u8) -> String {
    if color_enabled() {
        format!("\x1b[2m[{step}/{total}]\x1b[0m")
    } else {
        format!("[{step}/{total}]")
    }
}

fn working_line(step: u8, total: u8, frame: usize) -> String {
    format!(
        "{} {}",
        muted_step_label(step, total),
        gradient_text("Working...", frame)
    )
}

fn render_working(step: u8, total: u8, frame: usize) {
    clear_line();
    print!("{}", working_line(step, total, frame));
    let _ = io::stdout().flush();
}

async fn render_prompt_animation(stop: tokio::sync::oneshot::Receiver<()>) {
    let prompt = "URL> ";
    let mut frame = 0usize;
    let mut stop = stop;

    loop {
        tokio::select! {
            _ = &mut stop => {
                print!("\r{}", gradient_text(prompt, 0));
                let _ = io::stdout().flush();
                break;
            }
            _ = sleep(Duration::from_millis(95)) => {
                print!("\r{}", gradient_text(prompt, frame));
                let _ = io::stdout().flush();
                frame = frame.saturating_add(1);
            }
        }
    }
}

fn fail_color_for_count(failed: usize) -> TextTone {
    if failed > 0 {
        TextTone::Error
    } else {
        TextTone::Success
    }
}

async fn read_line_with_prompt_animation() -> Result<String> {
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let prompt_task = tokio::spawn(render_prompt_animation(stop_rx));

    let read_task = tokio::task::spawn_blocking::<_, Result<(usize, String)>>(|| {
        let mut raw = String::new();
        let read = io::stdin().read_line(&mut raw)?;
        Ok((read, raw))
    });

    let (read, raw) = read_task.await.context("Failed to read URL from stdin")??;

    let _ = stop_tx.send(());
    let _ = prompt_task.await;

    if read == 0 {
        Ok(String::new())
    } else {
        Ok(raw)
    }
}

async fn run_stage<T, Fut>(options: &RunOptions, step: u8, total: u8, future: Fut) -> Result<T>
where
    Fut: Future<Output = Result<T>> + Send,
{
    if !options.should_emit_progress() {
        return future.await;
    }

    let mut future = std::pin::pin!(future);
    let mut frame = 0usize;

    render_working(step, total, 0);

    let result = loop {
        tokio::select! {
            result = &mut future => {
                break result;
            }
            _ = sleep(Duration::from_millis(150)) => {
                render_working(step, total, frame);
                frame = frame.saturating_add(1);
            }
        }
    };

    if color_enabled() {
        clear_line();
    }

    result
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub copy: bool,
    pub force_open: bool,
    pub no_open: bool,
    pub print_pgn: bool,
    pub save_pgn: Option<PathBuf>,
    pub raw_url: bool,
    pub json_output: bool,
    pub csv_output: bool,
    pub quiet: bool,
    pub verbose: bool,
}

impl RunOptions {
    pub fn should_open(&self) -> bool {
        if self.no_open {
            false
        } else {
            self.force_open || !self.no_open
        }
    }

    fn should_emit_progress(&self) -> bool {
        self.verbose || (!self.json_output && !self.csv_output && !self.raw_url && !self.quiet)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub input_url: String,
    pub game_id: String,
    pub pgn: String,
    pub analysis_url: String,
    pub retries: u8,
}

impl AnalysisResult {
    pub fn final_analysis_url(&self) -> String {
        self.analysis_url.clone()
    }
}

#[derive(Serialize)]
struct JsonLine {
    input_url: String,
    success: bool,
    game_id: Option<String>,
    analysis_url: Option<String>,
    pgn: Option<String>,
    retries: u8,
    error: Option<String>,
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

pub async fn resolve_with_progress(url: &str, options: &RunOptions) -> Result<AnalysisResult> {
    let client = build_http_client()?;
    let game = run_stage(options, 1, 5, async {
        chesscom::parse_chesscom_game_url(url)
    })
    .await?;

    run_stage(options, 2, 5, async { Ok(()) }).await?;

    let pgn = run_stage(options, 3, 5, async {
        chesscom::fetch_game_pgn(&client, &game)
            .await
            .context("Failed to extract PGN")
    })
    .await?;

    let analysis_url = run_stage(options, 4, 5, async {
        lichess::import_via_api(&client, &pgn)
            .await
            .context("lichess API import failed")
    })
    .await?;

    run_stage(options, 5, 5, async { Ok(()) }).await?;

    Ok(AnalysisResult {
        input_url: url.to_string(),
        game_id: game.game_id,
        pgn,
        analysis_url,
        retries: 0,
    })
}

fn retryable_delay(attempt: u8) -> u64 {
    let backoff = BASE_RETRY_DELAY_MS.saturating_mul(2u64.saturating_pow(attempt as u32));
    backoff.min(MAX_RETRY_DELAY_MS)
}

fn is_retryable_error(err: &anyhow::Error) -> bool {
    let mut current: Option<&dyn StdError> = Some(err.as_ref());
    while let Some(error) = current {
        if let Some(c2l_error) = error.downcast_ref::<C2lError>() {
            match c2l_error {
                C2lError::RetryableHttp { .. } | C2lError::RetryableRequest { .. } => {
                    return true;
                }
                _ => return false,
            }
        }
        current = error.source();
    }
    false
}

async fn run_with_retries(raw_url: String, options: &RunOptions) -> Result<AnalysisResult> {
    let mut last_error: Option<anyhow::Error> = None;
    let mut attempt = 0u8;

    while attempt < MAX_RETRY_ATTEMPTS {
        attempt = attempt.saturating_add(1);
        let result = resolve_with_progress(&raw_url, options).await;

        match result {
            Ok(resolved) => {
                let mut result = resolved;
                result.retries = attempt.saturating_sub(1);
                return Ok(result);
            }
            Err(err) => {
                last_error = Some(err);
                if attempt >= MAX_RETRY_ATTEMPTS
                    || !is_retryable_error(last_error.as_ref().expect("error"))
                {
                    break;
                }

                if !options.quiet {
                    eprintln!(
                        "{}",
                        colorize(
                            &format!(
                                "[retry] transient error for {} ({}/{}): {}",
                                raw_url,
                                attempt,
                                MAX_RETRY_ATTEMPTS,
                                last_error.as_ref().expect("error").to_string()
                            ),
                            TextTone::Warn
                        )
                    );
                }

                if options.verbose {
                    eprintln!(
                        "{}",
                        colorize(
                            &format!(
                                "[retry] waiting {}ms before attempt {}",
                                retryable_delay(attempt),
                                attempt.saturating_add(1)
                            ),
                            TextTone::Muted
                        )
                    );
                }

                sleep(Duration::from_millis(retryable_delay(attempt))).await;
            }
        }
    }

    match last_error {
        Some(error) => Err(error),
        None => Err(anyhow!("Failed after retries")),
    }
}

fn csv_escape(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn emit_json_line(url: &str, result: Option<&AnalysisResult>, error: Option<&str>, retries: u8) {
    let output = JsonLine {
        input_url: url.to_string(),
        success: result.is_some(),
        game_id: result.map(|r| r.game_id.clone()),
        analysis_url: result.map(|r| r.analysis_url.clone()),
        pgn: result.map(|r| r.pgn.clone()),
        retries,
        error: error.map(str::to_string),
    };

    match serde_json::to_string(&output) {
        Ok(value) => println!("{value}"),
        Err(err) => eprintln!("Failed to build JSON output for {url}: {err}"),
    }
}

fn emit_csv_line(url: &str, result: Option<&AnalysisResult>, error: Option<&str>, retries: u8) {
    let game_id = result.map_or("", |r| r.game_id.as_str());
    let analysis_url = result.map_or("", |r| r.analysis_url.as_str());
    let error = error.unwrap_or_default();
    println!(
        "{},{},{},{},{},{}",
        csv_escape(url),
        if result.is_some() { "ok" } else { "error" },
        retries,
        csv_escape(game_id),
        csv_escape(analysis_url),
        csv_escape(error)
    );
}

async fn emit_output(result: &AnalysisResult, options: &RunOptions) -> Result<()> {
    if options.copy {
        if let Err(err) = clipboard::copy_to_clipboard(&result.pgn) {
            if !options.quiet {
                eprintln!(
                    "{}",
                    colorize(&format!("Failed to copy PGN: {err}"), TextTone::Error)
                );
            }
        } else if !options.raw_url && !options.quiet {
            println!("{}", colorize("PGN copied", TextTone::Success));
        }
    }

    if let Some(path) = &options.save_pgn {
        fs::write(path, &result.pgn)
            .await
            .with_context(|| format!("Failed to save PGN: {}", path.display()))?;

        if !options.raw_url && !options.quiet {
            println!(
                "{}",
                colorize(&format!("PGN saved: {}", path.display()), TextTone::Success)
            );
        }
    }

    if options.should_open() {
        if let Err(err) = open_url(&result.analysis_url) {
            if !options.quiet {
                eprintln!(
                    "{}",
                    colorize(
                        &format!("Failed to auto-open browser: {err}"),
                        TextTone::Warn
                    )
                );
            }
        }
    }

    if !options.json_output && !options.csv_output {
        if options.print_pgn {
            if !options.raw_url {
                println!("{}", colorize("\n=== PGN ===", TextTone::Info));
            }
            println!("{}", result.pgn);
        }

        if options.raw_url {
            println!("{}", colorize(&result.final_analysis_url(), TextTone::Info));
        } else {
            println!(
                "{}",
                colorize(
                    &format!("Final URL: {}", result.final_analysis_url()),
                    TextTone::Success
                )
            );
            println!(
                "{}",
                colorize("Acquired via: lichess API import", TextTone::Muted)
            );
        }
    }

    Ok(())
}

pub async fn run_once(raw_url: String, options: &RunOptions) -> Result<AnalysisResult> {
    run_with_retries(raw_url, options).await
}

pub async fn run_once_and_output(raw_url: String, options: &RunOptions) -> Result<()> {
    match run_once(raw_url.clone(), options).await {
        Ok(result) => {
            emit_output(&result, options).await?;
            Ok(())
        }
        Err(err) => {
            if options.json_output {
                emit_json_line(&raw_url, None, Some(&err.to_string()), 0);
            } else if options.csv_output {
                emit_csv_line(&raw_url, None, Some(&err.to_string()), 0);
            } else {
                eprintln!("{}", colorize(&format!("Failed: {err}"), TextTone::Error));
            }
            Err(err)
        }
    }
}

pub async fn run_batch(urls: Vec<String>, options: &RunOptions) -> Result<()> {
    let mut failed = 0usize;
    let mut succeeded = 0usize;

    if options.csv_output && !options.quiet {
        println!("input_url,success,retries,game_id,analysis_url,error");
    }

    for raw_url in urls.into_iter().filter(|url| !url.trim().is_empty()) {
        match run_once(raw_url.clone(), options).await {
            Ok(result) => {
                let retries = result.retries;
                emit_output(&result, options).await?;
                if options.json_output {
                    emit_json_line(&raw_url, Some(&result), None, retries);
                } else if options.csv_output {
                    emit_csv_line(&raw_url, Some(&result), None, retries);
                }
                succeeded += 1;
            }
            Err(err) => {
                failed += 1;
                if options.json_output {
                    emit_json_line(&raw_url, None, Some(&err.to_string()), 0);
                } else if options.csv_output {
                    emit_csv_line(&raw_url, None, Some(&err.to_string()), 0);
                } else if !options.quiet {
                    eprintln!(
                        "{}",
                        colorize(&format!("Failed: {raw_url}: {err}"), TextTone::Error)
                    );
                }
            }
        }
    }

    if !options.json_output && !options.csv_output && !options.quiet {
        println!(
            "{}",
            colorize(
                &format!("Processed: {succeeded}, Failed: {failed}"),
                fail_color_for_count(failed),
            )
        );
    }

    if failed > 0 {
        return Err(anyhow!("Completed with {failed} failed conversions"));
    }

    Ok(())
}

pub async fn run_interactive(options: &RunOptions) -> Result<()> {
    println!("Interactive mode. Enter a URL, or type q / quit / exit to leave.");

    loop {
        let raw = read_line_with_prompt_animation().await?;
        io::stdout()
            .flush()
            .context("Failed to print input prompt")?;

        if raw.is_empty() {
            println!("{}", colorize("Bye.", TextTone::Muted));
            break;
        }

        if raw.trim().is_empty() {
            continue;
        }

        let lowered = raw.trim().to_lowercase();
        if matches!(lowered.as_str(), "q" | "quit" | "exit") {
            println!("{}", colorize("Bye.", TextTone::Muted));
            break;
        }

        if let Err(err) = run_once_and_output(raw.trim().to_string(), options).await {
            if !options.quiet {
                eprintln!("{}", colorize(&format!("Failed: {err}"), TextTone::Error));
            }
        }

        println!("{}", colorize("", TextTone::Muted));
    }

    Ok(())
}

pub fn stdin_is_tty() -> bool {
    io::stdin().is_terminal()
}

pub async fn urls_from_file(path: &PathBuf) -> Result<Vec<String>> {
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read input file: {}", path.display()))?;
    Ok(raw
        .lines()
        .map(|line| {
            line.split('#')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect())
}

pub fn urls_from_stdin() -> Result<Vec<String>> {
    let mut raw = String::new();
    io::stdin()
        .read_to_string(&mut raw)
        .with_context(|| "Failed to read URLs from stdin")?;
    Ok(raw
        .lines()
        .map(|line| {
            line.split('#')
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect())
}

#[derive(Debug, serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

fn trim_version(value: &str) -> &str {
    value.trim_start_matches('v')
}

pub async fn run_doctor(check_updates: bool) -> Result<()> {
    println!("{}", colorize("c2l doctor", TextTone::Info));
    println!(
        "{}",
        colorize(
            &format!("Version: {}", env!("CARGO_PKG_VERSION")),
            TextTone::Success
        )
    );
    println!(
        "{}",
        colorize(
            &format!("Binary: {}", std::env::current_exe()?.display()),
            TextTone::Success
        )
    );
    println!(
        "{}",
        colorize(
            &format!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH),
            TextTone::Success
        )
    );

    let client = build_http_client()?;

    for (name, url) in [
        ("chess.com", "https://www.chess.com"),
        ("lichess.org", "https://lichess.org"),
    ] {
        match client.get(url).timeout(Duration::from_secs(3)).send().await {
            Ok(response) => {
                let status = response.status();
                println!(
                    "{}",
                    colorize(&format!("Network: {name} => {status}"), TextTone::Info)
                );
            }
            Err(error) => {
                println!(
                    "{}",
                    colorize(
                        &format!("Network: {name} => unreachable ({error})"),
                        TextTone::Warn
                    )
                );
            }
        }
    }

    if check_updates {
        let response = client
            .get("https://api.github.com/repos/wnsdud-jy/chess2lichess/releases/latest")
            .header(
                reqwest::header::USER_AGENT,
                "c2l-doctor/0.1.4 (+https://github.com/wnsdud-jy/chess2lichess)",
            )
            .send()
            .await
            .context("Failed to query GitHub release endpoint")?;

        if response.status() != StatusCode::OK {
            println!(
                "{}",
                colorize(
                    &format!("Update check: API status {}", response.status()),
                    TextTone::Warn
                )
            );
            return Ok(());
        }

        let body = response
            .text()
            .await
            .context("Failed to read latest release response body")?;
        let release = serde_json::from_str::<GitHubRelease>(&body)
            .context("Failed to parse GitHub release response")?;
        let current = trim_version(env!("CARGO_PKG_VERSION"));
        let latest = trim_version(&release.tag_name);

        if latest != current {
            println!(
                "{}",
                colorize(
                    &format!("Update available: {latest} (current: {current})"),
                    TextTone::Warn
                )
            );
        } else {
            println!(
                "{}",
                colorize(
                    &format!("You are on the latest release: {current}"),
                    TextTone::Info
                )
            );
        }
    }

    Ok(())
}
