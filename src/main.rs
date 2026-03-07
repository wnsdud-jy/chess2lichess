use anyhow::Result;

mod app;
mod chesscom;
mod cli;
mod clipboard;
mod browser;
mod error;
mod lichess;
mod tui;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let options = app::RunOptions {
        copy: cli.copy,
        force_open: cli.open,
        no_open: cli.no_open,
        print_pgn: cli.print_pgn,
        save_pgn: cli.save_pgn,
        raw_url: cli.raw_url,
    };

    match cli.mode {
        Some(Commands::Tui) => tui::run_tui().await,
        None => {
            if let Some(url) = cli.url {
                app::run_once(url, options).await
            } else {
                app::run_interactive(options).await
            }
        }
    }
}
