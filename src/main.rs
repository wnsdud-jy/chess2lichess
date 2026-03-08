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
