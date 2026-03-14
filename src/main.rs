use anyhow::Result;

mod app;
mod browser;
mod chesscom;
mod cli;
mod clipboard;
mod error;
mod lichess;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let output_json = cli.json || matches!(cli.format, cli::OutputFormat::Json);
    let output_csv = matches!(cli.format, cli::OutputFormat::Csv);

    let options = app::RunOptions {
        copy: cli.copy,
        force_open: cli.open,
        no_open: cli.no_open,
        print_pgn: cli.print_pgn,
        save_pgn: cli.save_pgn,
        raw_url: cli.raw_url,
        json_output: output_json,
        csv_output: output_csv,
        quiet: cli.quiet,
        verbose: cli.verbose,
    };

    if app::should_prompt_for_npm_update(cli.mode.is_some(), &options) {
        match app::is_npm_update_prompt_muted().await {
            Ok(true) => {
                if cli.verbose {
                    eprintln!("Skipped npm update check because prompts are muted.");
                }
            }
            Ok(false) => match app::detect_available_npm_update().await {
                Ok(Some(update)) => match tui::prompt_npm_upgrade(&update)? {
                    app::NpmUpdatePromptChoice::UpgradeNow => {
                        println!(
                            "Running upgrade: {}",
                            app::npm_install_command_display(&update)
                        );
                        match app::install_npm_update(&update).await {
                            Ok(()) => {
                                println!(
                                    "Upgrade finished. Continuing with c2l {}.",
                                    env!("CARGO_PKG_VERSION")
                                );
                            }
                            Err(error) => {
                                eprintln!("Upgrade failed: {error}");
                            }
                        }
                    }
                    app::NpmUpdatePromptChoice::SkipOnce => {}
                    app::NpmUpdatePromptChoice::MuteForSevenDays => {
                        match app::mute_npm_update_prompt_for_days(7).await {
                            Ok(_) => {
                                println!("Muted npm update prompts for 7 days.");
                            }
                            Err(error) => {
                                eprintln!("Failed to persist update mute: {error}");
                            }
                        }
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    if cli.verbose {
                        eprintln!("Skipped npm update check: {error}");
                    }
                }
            },
            Err(error) => {
                if cli.verbose {
                    eprintln!("Skipped npm update mute check: {error}");
                }
            }
        }
    }

    match cli.mode {
        Some(Commands::Tui) => tui::run_tui().await,
        Some(Commands::Doctor { check_updates }) => app::run_doctor(check_updates).await,
        None => {
            let mut urls = cli.urls;
            if let Some(file) = cli.input_file {
                urls.extend(app::urls_from_file(&file).await?);
            }
            if urls.is_empty() && !app::stdin_is_tty() {
                urls.extend(app::urls_from_stdin()?);
            }

            if urls.is_empty() {
                app::run_interactive(&options).await
            } else {
                app::run_batch(urls, &options).await
            }
        }
    }
}
