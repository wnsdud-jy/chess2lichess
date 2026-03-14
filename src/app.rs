use std::{
    error::Error as StdError,
    future::Future,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process::Stdio,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use reqwest::{Client, StatusCode, header::HeaderValue};
use serde::{Deserialize, Serialize};
use tokio::{fs, process::Command as TokioCommand, time::sleep};

use crate::{browser::open_url, chesscom, clipboard, error::C2lError, lichess};

const MAX_RETRY_ATTEMPTS: u8 = 4;
const BASE_RETRY_DELAY_MS: u64 = 250;
const MAX_RETRY_DELAY_MS: u64 = 2500;
const ECO_OPENINGS_CSV: &str = include_str!("../assets/eco_openings.csv");

#[derive(Clone, Copy)]
enum TextTone {
    Info,
    Success,
    Warn,
    Error,
    Muted,
}

const WHITE_GRADIENT_FRAMES: [u8; 8] = [238, 242, 246, 249, 252, 255, 252, 249];
static ECO_OPENINGS: OnceLock<std::collections::BTreeMap<String, String>> = OnceLock::new();

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
    pub metadata: GameMetadata,
    pub retries: u8,
}

impl AnalysisResult {
    pub fn final_analysis_url(&self) -> String {
        self.analysis_url.clone()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameMetadata {
    pub event: Option<String>,
    pub white: Option<String>,
    pub black: Option<String>,
    #[serde(skip)]
    pub white_elo: Option<String>,
    #[serde(skip)]
    pub black_elo: Option<String>,
    pub result: Option<String>,
    pub date: Option<String>,
    pub opening: Option<String>,
    pub eco: Option<String>,
    pub moves_count: Option<u32>,
}

impl GameMetadata {
    fn from_pgn(pgn: &str) -> Self {
        let headers = parse_pgn_headers(pgn);
        Self {
            event: headers.get("Event").cloned(),
            white: headers.get("White").cloned(),
            black: headers.get("Black").cloned(),
            white_elo: headers.get("WhiteElo").cloned(),
            black_elo: headers.get("BlackElo").cloned(),
            result: headers.get("Result").cloned(),
            date: headers.get("Date").cloned(),
            opening: headers.get("Opening").cloned(),
            eco: headers.get("ECO").cloned(),
            moves_count: count_full_moves(pgn),
        }
    }

    pub fn players_label(&self) -> Option<String> {
        match (self.white.as_deref(), self.black.as_deref()) {
            (Some(white), Some(black)) => Some(format!(
                "{} vs {}",
                Self::format_player(white, self.white_elo.as_deref()),
                Self::format_player(black, self.black_elo.as_deref())
            )),
            (Some(white), None) => Some(Self::format_player(white, self.white_elo.as_deref())),
            (None, Some(black)) => Some(Self::format_player(black, self.black_elo.as_deref())),
            _ => None,
        }
    }

    pub fn summary_bits(&self) -> Vec<String> {
        let mut bits = Vec::new();
        if let Some(result) = &self.result {
            bits.push(format!("Result: {result}"));
        }
        if let Some(date) = &self.date {
            bits.push(format!("Date: {date}"));
        }
        if let Some(moves_count) = self.moves_count {
            bits.push(format!("Moves: {moves_count}"));
        }
        bits
    }

    pub fn opening_label(&self) -> Option<String> {
        let opening = self
            .opening
            .as_deref()
            .or_else(|| self.eco.as_deref().and_then(eco_opening_lookup));

        match (opening, self.eco.as_deref()) {
            (Some(opening), Some(eco)) => Some(format!("{opening} ({eco})")),
            (Some(opening), None) => Some(opening.to_string()),
            _ => None,
        }
    }

    fn format_player(name: &str, rating: Option<&str>) -> String {
        format!("{name} ({})", rating.unwrap_or("?"))
    }
}

#[derive(Serialize)]
struct JsonLine {
    input_url: String,
    success: bool,
    game_id: Option<String>,
    analysis_url: Option<String>,
    pgn: Option<String>,
    metadata: Option<GameMetadata>,
    retries: u8,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmSelfUpdateContext {
    pub package_name: String,
    pub current_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NpmUpdateInfo {
    pub package_name: String,
    pub current_version: String,
    pub latest_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NpmUpdatePromptChoice {
    UpgradeNow,
    SkipOnce,
    MuteForSevenDays,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct AppConfig {
    #[serde(default)]
    npm_update: NpmUpdateConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct NpmUpdateConfig {
    mute_until_epoch_seconds: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct NpmLatestPackage {
    version: String,
}

fn npm_update_context_from_values(
    wrapper: Option<&str>,
    package_name: Option<&str>,
    current_version: Option<&str>,
) -> Option<NpmSelfUpdateContext> {
    if wrapper != Some("1") {
        return None;
    }

    let package_name = package_name?.trim();
    let current_version = current_version?.trim();

    if package_name.is_empty() || current_version.is_empty() {
        return None;
    }

    Some(NpmSelfUpdateContext {
        package_name: package_name.to_string(),
        current_version: current_version.to_string(),
    })
}

pub fn npm_update_context_from_env() -> Option<NpmSelfUpdateContext> {
    npm_update_context_from_values(
        std::env::var("C2L_NPM_WRAPPER").ok().as_deref(),
        std::env::var("C2L_NPM_PACKAGE_NAME").ok().as_deref(),
        std::env::var("C2L_NPM_PACKAGE_VERSION").ok().as_deref(),
    )
}

fn encode_npm_package_name(package_name: &str) -> String {
    package_name.replace('@', "%40").replace('/', "%2F")
}

fn npm_latest_version_url(package_name: &str) -> String {
    format!(
        "https://registry.npmjs.org/{}/latest",
        encode_npm_package_name(package_name)
    )
}

pub fn stdout_is_tty() -> bool {
    io::stdout().is_terminal()
}

fn should_prompt_for_npm_update_with_tty(
    mode_selected: bool,
    stdin_tty: bool,
    stdout_tty: bool,
    options: &RunOptions,
) -> bool {
    !mode_selected
        && stdin_tty
        && stdout_tty
        && !options.json_output
        && !options.csv_output
        && !options.raw_url
        && !options.quiet
}

pub fn should_prompt_for_npm_update(mode_selected: bool, options: &RunOptions) -> bool {
    should_prompt_for_npm_update_with_tty(mode_selected, stdin_is_tty(), stdout_is_tty(), options)
}

pub async fn detect_available_npm_update() -> Result<Option<NpmUpdateInfo>> {
    let Some(context) = npm_update_context_from_env() else {
        return Ok(None);
    };

    let client = build_http_client()?;
    let latest_version = fetch_latest_npm_version(&client, &context.package_name).await?;

    if trim_version(&latest_version) == trim_version(&context.current_version) {
        return Ok(None);
    }

    Ok(Some(NpmUpdateInfo {
        package_name: context.package_name,
        current_version: context.current_version,
        latest_version,
    }))
}

async fn fetch_latest_npm_version(client: &Client, package_name: &str) -> Result<String> {
    let response = client
        .get(npm_latest_version_url(package_name))
        .timeout(Duration::from_secs(3))
        .header(
            reqwest::header::USER_AGENT,
            format!(
                "c2l/{version} (+https://github.com/wnsdud-jy/chess2lichess)",
                version = env!("CARGO_PKG_VERSION")
            ),
        )
        .send()
        .await
        .context("Failed to query npm registry")?;

    if response.status() != StatusCode::OK {
        return Err(anyhow!(
            "npm registry responded with status {}",
            response.status()
        ));
    }

    let body = response
        .text()
        .await
        .context("Failed to read npm registry response body")?;
    let package = serde_json::from_str::<NpmLatestPackage>(&body)
        .context("Failed to parse npm registry response")?;
    Ok(package.version)
}

pub fn npm_install_command(update: &NpmUpdateInfo) -> (String, Vec<String>) {
    let executable = if cfg!(windows) {
        "npm.cmd".to_string()
    } else {
        "npm".to_string()
    };

    let args = vec![
        "install".to_string(),
        "-g".to_string(),
        format!("{}@latest", update.package_name),
    ];

    (executable, args)
}

pub fn npm_install_command_display(update: &NpmUpdateInfo) -> String {
    let (executable, args) = npm_install_command(update);
    std::iter::once(executable)
        .chain(args)
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn install_npm_update(update: &NpmUpdateInfo) -> Result<()> {
    let (executable, args) = npm_install_command(update);
    let status = TokioCommand::new(&executable)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .with_context(|| format!("Failed to launch {}", executable))?;

    if !status.success() {
        return Err(anyhow!(
            "{} exited with status {}",
            executable,
            status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_string())
        ));
    }

    Ok(())
}

fn parse_pgn_headers(pgn: &str) -> std::collections::BTreeMap<String, String> {
    let header_re = regex::Regex::new(r#"^\[([A-Za-z0-9_]+)\s+"(.*)"\]$"#).unwrap();
    let mut headers = std::collections::BTreeMap::new();

    for line in pgn.lines() {
        let trimmed = line.trim();
        if let Some(captures) = header_re.captures(trimmed) {
            let key = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let value = captures
                .get(2)
                .map(|value| value.as_str())
                .unwrap_or_default()
                .replace("\\\"", "\"");
            headers.insert(key.to_string(), value);
        }
    }

    headers
}

fn count_full_moves(pgn: &str) -> Option<u32> {
    let move_number_re = regex::Regex::new(r"^\d+\.(\.\.)?$").unwrap();
    let body = pgn
        .lines()
        .filter(|line| !line.trim_start().starts_with('['))
        .collect::<Vec<_>>()
        .join("\n");

    let mut sanitized = String::with_capacity(body.len());
    let mut brace_depth = 0u32;
    let mut paren_depth = 0u32;
    let mut line_comment = false;

    for ch in body.chars() {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
                sanitized.push(' ');
            }
            continue;
        }

        match ch {
            '{' => brace_depth = brace_depth.saturating_add(1),
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '(' => paren_depth = paren_depth.saturating_add(1),
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ';' if brace_depth == 0 && paren_depth == 0 => line_comment = true,
            _ if brace_depth == 0 && paren_depth == 0 => sanitized.push(ch),
            _ => {}
        }
    }

    let mut plies = 0u32;
    for token in sanitized.split_whitespace() {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(trimmed, "1-0" | "0-1" | "1/2-1/2" | "*") {
            continue;
        }
        if trimmed.starts_with('$') {
            continue;
        }
        if move_number_re.is_match(trimmed) {
            continue;
        }
        plies = plies.saturating_add(1);
    }

    if plies == 0 {
        None
    } else {
        Some(plies.div_ceil(2))
    }
}

fn eco_opening_lookup(eco: &str) -> Option<&'static str> {
    eco_openings().get(eco.trim()).map(String::as_str)
}

fn eco_openings() -> &'static std::collections::BTreeMap<String, String> {
    ECO_OPENINGS.get_or_init(|| parse_eco_openings_csv(ECO_OPENINGS_CSV))
}

fn parse_eco_openings_csv(raw: &str) -> std::collections::BTreeMap<String, String> {
    let mut openings = std::collections::BTreeMap::new();

    for (index, line) in raw.lines().enumerate() {
        if index == 0 || line.trim().is_empty() {
            continue;
        }

        if let Some((eco, opening)) = parse_eco_opening_record(line) {
            openings.insert(eco, opening);
        }
    }

    openings
}

fn parse_eco_opening_record(line: &str) -> Option<(String, String)> {
    let line = line.trim_end_matches('\r');
    let (eco, opening) = line.split_once(',')?;
    Some((parse_csv_field(eco), parse_csv_field(opening)))
}

fn parse_csv_field(field: &str) -> String {
    let field = field.trim();
    match field
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        Some(value) => value.replace("\"\"", "\""),
        None => field.to_string(),
    }
}

fn config_dir_from_env(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(xdg) = xdg_config_home
        && !xdg.trim().is_empty()
    {
        return Some(PathBuf::from(xdg).join("c2l"));
    }

    let home = home?.trim();
    if home.is_empty() {
        return None;
    }

    Some(PathBuf::from(home).join(".config").join("c2l"))
}

pub fn config_path_from_env(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    config_dir_from_env(xdg_config_home, home).map(|dir| dir.join("config.json"))
}

fn config_path() -> Option<PathBuf> {
    config_path_from_env(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

fn current_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn mute_is_active(mute_until_epoch_seconds: Option<u64>, now_epoch_seconds: u64) -> bool {
    mute_until_epoch_seconds.is_some_and(|mute_until| mute_until > now_epoch_seconds)
}

async fn load_app_config() -> Result<AppConfig> {
    let Some(path) = config_path() else {
        return Ok(AppConfig::default());
    };

    match fs::read_to_string(&path).await {
        Ok(raw) => serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse config file: {}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AppConfig::default()),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

async fn save_app_config(config: &AppConfig) -> Result<()> {
    let Some(path) = config_path() else {
        return Err(anyhow!("Could not determine a config path for c2l"));
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let body = serde_json::to_string_pretty(config).context("Failed to serialize c2l config")?;
    fs::write(&path, body)
        .await
        .with_context(|| format!("Failed to write config file: {}", path.display()))?;
    Ok(())
}

pub async fn is_npm_update_prompt_muted() -> Result<bool> {
    let config = load_app_config().await?;
    Ok(mute_is_active(
        config.npm_update.mute_until_epoch_seconds,
        current_epoch_seconds(),
    ))
}

pub async fn mute_npm_update_prompt_for_days(days: u64) -> Result<u64> {
    let mut config = load_app_config().await?;
    let mute_until = current_epoch_seconds().saturating_add(days.saturating_mul(24 * 60 * 60));
    config.npm_update.mute_until_epoch_seconds = Some(mute_until);
    save_app_config(&config).await?;
    Ok(mute_until)
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
    let metadata = GameMetadata::from_pgn(&pgn);

    Ok(AnalysisResult {
        input_url: url.to_string(),
        game_id: game.game_id,
        pgn,
        analysis_url,
        metadata,
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

fn build_json_line(
    url: &str,
    result: Option<&AnalysisResult>,
    error: Option<&str>,
    retries: u8,
) -> Result<String> {
    let output = JsonLine {
        input_url: url.to_string(),
        success: result.is_some(),
        game_id: result.map(|r| r.game_id.clone()),
        analysis_url: result.map(|r| r.analysis_url.clone()),
        pgn: result.map(|r| r.pgn.clone()),
        metadata: result.map(|r| r.metadata.clone()),
        retries,
        error: error.map(str::to_string),
    };

    serde_json::to_string(&output).context("Failed to serialize JSON output")
}

fn emit_json_line(url: &str, result: Option<&AnalysisResult>, error: Option<&str>, retries: u8) {
    match build_json_line(url, result, error, retries) {
        Ok(value) => println!("{value}"),
        Err(err) => eprintln!("Failed to build JSON output for {url}: {err}"),
    }
}

fn csv_header() -> &'static str {
    "input_url,success,retries,game_id,analysis_url,event,white,black,result,date,opening,eco,moves_count,error"
}

fn build_csv_line(
    url: &str,
    result: Option<&AnalysisResult>,
    error: Option<&str>,
    retries: u8,
) -> String {
    let game_id = result.map_or("", |r| r.game_id.as_str());
    let analysis_url = result.map_or("", |r| r.analysis_url.as_str());
    let metadata = result.map(|r| &r.metadata);
    let event = metadata
        .and_then(|m| m.event.as_deref())
        .unwrap_or_default();
    let white = metadata
        .and_then(|m| m.white.as_deref())
        .unwrap_or_default();
    let black = metadata
        .and_then(|m| m.black.as_deref())
        .unwrap_or_default();
    let result_value = metadata
        .and_then(|m| m.result.as_deref())
        .unwrap_or_default();
    let date = metadata.and_then(|m| m.date.as_deref()).unwrap_or_default();
    let opening = metadata
        .and_then(|m| m.opening.as_deref())
        .unwrap_or_default();
    let eco = metadata.and_then(|m| m.eco.as_deref()).unwrap_or_default();
    let moves_count = metadata
        .and_then(|m| m.moves_count)
        .map(|value| value.to_string())
        .unwrap_or_default();
    let error = error.unwrap_or_default();
    format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        csv_escape(url),
        if result.is_some() { "ok" } else { "error" },
        retries,
        csv_escape(game_id),
        csv_escape(analysis_url),
        csv_escape(event),
        csv_escape(white),
        csv_escape(black),
        csv_escape(result_value),
        csv_escape(date),
        csv_escape(opening),
        csv_escape(eco),
        csv_escape(&moves_count),
        csv_escape(error)
    )
}

fn emit_csv_line(url: &str, result: Option<&AnalysisResult>, error: Option<&str>, retries: u8) {
    println!("{}", build_csv_line(url, result, error, retries));
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
            if let Some(players) = result.metadata.players_label() {
                println!(
                    "{}",
                    colorize(&format!("Players: {players}"), TextTone::Info)
                );
            }
            let summary_bits = result.metadata.summary_bits();
            if !summary_bits.is_empty() {
                println!("{}", colorize(&summary_bits.join(" | "), TextTone::Muted));
            }
            if let Some(opening) = result.metadata.opening_label() {
                println!(
                    "{}",
                    colorize(&format!("Opening: {opening}"), TextTone::Info)
                );
            }
            println!(
                "{}",
                colorize(
                    &format!("Final URL: {}", result.final_analysis_url()),
                    TextTone::Success
                )
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
        println!("{}", csv_header());
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
                format!(
                    "c2l-doctor/{version} (+https://github.com/wnsdud-jy/chess2lichess)",
                    version = env!("CARGO_PKG_VERSION")
                ),
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

        if let Some(context) = npm_update_context_from_env() {
            match fetch_latest_npm_version(&client, &context.package_name).await {
                Ok(latest) if trim_version(&latest) != trim_version(&context.current_version) => {
                    println!(
                        "{}",
                        colorize(
                            &format!(
                                "npm package update available: {} (current: {})",
                                latest, context.current_version
                            ),
                            TextTone::Warn
                        )
                    );
                    println!(
                        "{}",
                        colorize(
                            &format!(
                                "Upgrade command: {}",
                                npm_install_command_display(&NpmUpdateInfo {
                                    package_name: context.package_name.clone(),
                                    current_version: context.current_version.clone(),
                                    latest_version: latest,
                                })
                            ),
                            TextTone::Info
                        )
                    );
                }
                Ok(latest) => {
                    println!(
                        "{}",
                        colorize(
                            &format!("npm package is up to date: {}", trim_version(&latest)),
                            TextTone::Info
                        )
                    );
                }
                Err(error) => {
                    println!(
                        "{}",
                        colorize(
                            &format!("npm package update check failed: {error}"),
                            TextTone::Warn
                        )
                    );
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn sample_pgn() -> &'static str {
        r#"[Event "Live Chess"]
[Site "Chess.com"]
[Date "2026.03.14"]
[White "Alpha"]
[Black "Beta"]
[WhiteElo "1500"]
[BlackElo "1600"]
[Result "1-0"]
[ECO "C20"]
[Opening "King's Pawn Game"]

1. e4 { comment } e5 2. Nf3 Nc6 3. Bc4 Bc5 1-0"#
    }

    fn sample_result() -> AnalysisResult {
        let pgn = sample_pgn().to_string();
        AnalysisResult {
            input_url: "https://www.chess.com/game/live/123".to_string(),
            game_id: "123".to_string(),
            metadata: GameMetadata::from_pgn(&pgn),
            pgn,
            analysis_url: "https://lichess.org/abc123".to_string(),
            retries: 0,
        }
    }

    #[test]
    fn game_metadata_parses_pgn_headers() {
        let metadata = GameMetadata::from_pgn(sample_pgn());
        assert_eq!(metadata.event.as_deref(), Some("Live Chess"));
        assert_eq!(metadata.white.as_deref(), Some("Alpha"));
        assert_eq!(metadata.black.as_deref(), Some("Beta"));
        assert_eq!(metadata.white_elo.as_deref(), Some("1500"));
        assert_eq!(metadata.black_elo.as_deref(), Some("1600"));
        assert_eq!(metadata.result.as_deref(), Some("1-0"));
        assert_eq!(metadata.date.as_deref(), Some("2026.03.14"));
        assert_eq!(metadata.opening.as_deref(), Some("King's Pawn Game"));
        assert_eq!(metadata.eco.as_deref(), Some("C20"));
        assert_eq!(metadata.moves_count, Some(3));
    }

    #[test]
    fn build_json_line_includes_metadata() {
        let line = build_json_line(
            "https://www.chess.com/game/live/123",
            Some(&sample_result()),
            None,
            0,
        )
        .unwrap();
        let parsed = serde_json::from_str::<Value>(&line).unwrap();
        assert_eq!(parsed["metadata"]["white"], "Alpha");
        assert_eq!(parsed["metadata"]["moves_count"], 3);
    }

    #[test]
    fn build_csv_line_includes_metadata_columns() {
        let line = build_csv_line(
            "https://www.chess.com/game/live/123",
            Some(&sample_result()),
            None,
            0,
        );
        assert!(csv_header().contains("moves_count"));
        assert!(line.contains("\"Alpha\""));
        assert!(line.contains("\"King's Pawn Game\""));
    }

    #[test]
    fn players_label_includes_ratings_and_question_mark_fallback() {
        let metadata = GameMetadata {
            white: Some("Alpha".to_string()),
            black: Some("Beta".to_string()),
            white_elo: Some("1500".to_string()),
            ..Default::default()
        };

        assert_eq!(
            metadata.players_label().as_deref(),
            Some("Alpha (1500) vs Beta (?)")
        );
    }

    #[test]
    fn opening_label_uses_eco_lookup_when_opening_is_missing() {
        let metadata = GameMetadata {
            eco: Some("C20".to_string()),
            ..Default::default()
        };

        assert_eq!(metadata.opening_label().as_deref(), Some("Open Game (C20)"));
    }

    #[test]
    fn opening_label_prefers_pgn_opening_over_eco_lookup() {
        let metadata = GameMetadata {
            opening: Some("Custom Opening".to_string()),
            eco: Some("C20".to_string()),
            ..Default::default()
        };

        assert_eq!(
            metadata.opening_label().as_deref(),
            Some("Custom Opening (C20)")
        );
    }

    #[test]
    fn eco_openings_csv_covers_full_eco_range() {
        assert_eq!(eco_openings().len(), 500);
        assert_eq!(
            eco_opening_lookup("D72"),
            Some("Neo-Grünfeld Defense: 5. cxd5, Main Line")
        );
        assert_eq!(
            eco_opening_lookup("D73"),
            Some("Neo-Grünfeld Defense: 5. Nf3")
        );
        assert_eq!(
            eco_opening_lookup("E57"),
            Some(
                "Nimzo-Indian Defense: Normal Variation, Gligoric System, Bernstein Defense, 9. Bxc4 cxd4"
            )
        );
        assert_eq!(
            eco_opening_lookup("E88"),
            Some("King's Indian Defense: Sämisch Variation, Orthodox Variation, 7. d5 c6")
        );
    }

    #[test]
    fn config_path_prefers_xdg_config_home() {
        let path = config_path_from_env(Some("/tmp/cfg"), Some("/tmp/home")).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/tmp/cfg").join("c2l").join("config.json")
        );
    }

    #[test]
    fn mute_check_uses_future_deadlines() {
        assert!(mute_is_active(Some(101), 100));
        assert!(!mute_is_active(Some(100), 100));
        assert!(!mute_is_active(None, 100));
    }

    #[test]
    fn npm_update_context_requires_wrapper_env() {
        assert_eq!(
            npm_update_context_from_values(None, Some("@wnsdud-jy/c2l"), Some("0.1.6"),),
            None
        );
    }

    #[test]
    fn npm_update_context_reads_package_metadata() {
        let context =
            npm_update_context_from_values(Some("1"), Some("@wnsdud-jy/c2l"), Some("0.1.6"))
                .unwrap();

        assert_eq!(context.package_name, "@wnsdud-jy/c2l");
        assert_eq!(context.current_version, "0.1.6");
    }

    #[test]
    fn npm_latest_version_url_encodes_scoped_package_names() {
        assert_eq!(
            npm_latest_version_url("@wnsdud-jy/c2l"),
            "https://registry.npmjs.org/%40wnsdud-jy%2Fc2l/latest"
        );
    }

    #[test]
    fn should_prompt_for_npm_update_skips_machine_output_modes() {
        let options = RunOptions {
            copy: false,
            force_open: false,
            no_open: false,
            print_pgn: false,
            save_pgn: None,
            raw_url: true,
            json_output: false,
            csv_output: false,
            quiet: false,
            verbose: false,
        };

        assert!(!should_prompt_for_npm_update_with_tty(
            false, true, true, &options
        ));
    }

    #[test]
    fn should_prompt_for_npm_update_only_in_human_terminal_mode() {
        let options = RunOptions {
            copy: false,
            force_open: false,
            no_open: false,
            print_pgn: false,
            save_pgn: None,
            raw_url: false,
            json_output: false,
            csv_output: false,
            quiet: false,
            verbose: false,
        };

        assert!(should_prompt_for_npm_update_with_tty(
            false, true, true, &options
        ));
        assert!(!should_prompt_for_npm_update_with_tty(
            true, true, true, &options
        ));
        assert!(!should_prompt_for_npm_update_with_tty(
            false, false, true, &options
        ));
    }

    #[test]
    fn npm_install_command_targets_latest_global_package() {
        let update = NpmUpdateInfo {
            package_name: "@wnsdud-jy/c2l".to_string(),
            current_version: "0.1.5".to_string(),
            latest_version: "0.1.6".to_string(),
        };

        let (executable, args) = npm_install_command(&update);
        assert!(executable == "npm" || executable == "npm.cmd");
        assert_eq!(
            args,
            vec![
                "install".to_string(),
                "-g".to_string(),
                "@wnsdud-jy/c2l@latest".to_string()
            ]
        );
    }
}
