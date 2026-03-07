use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "c2l", version, about = "chess.com PGN -> lichess analysis bridge")]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Option<Commands>,

    #[arg(value_name = "URL", help = "chess.com game URL")]
    pub url: Option<String>,

    #[arg(long = "copy", help = "Copy PGN to clipboard")]
    pub copy: bool,

    #[arg(long = "open", conflicts_with = "no_open", help = "Open browser")]
    pub open: bool,

    #[arg(long = "print-pgn", help = "Print PGN to stdout")]
    pub print_pgn: bool,

    #[arg(long = "no-open", help = "Do not open browser automatically")]
    pub no_open: bool,

    #[arg(long = "save-pgn", value_name = "PATH", help = "Save PGN to file")]
    pub save_pgn: Option<PathBuf>,

    #[arg(
        long = "raw-url",
        conflicts_with = "print_pgn",
        help = "Print only the final URL"
    )]
    pub raw_url: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run TUI mode
    Tui,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_url_mode() {
        let cli = Cli::try_parse_from(["c2l", "https://www.chess.com/game/live/123"]).unwrap();
        assert!(cli.mode.is_none());
        assert_eq!(cli.url.as_deref(), Some("https://www.chess.com/game/live/123"));
    }

    #[test]
    fn parse_tui_command() {
        let cli = Cli::try_parse_from(["c2l", "tui"]).unwrap();
        assert!(matches!(cli.mode, Some(Commands::Tui)));
    }

    #[test]
    fn parse_flags() {
        let cli = Cli::try_parse_from([
            "c2l",
            "https://www.chess.com/game/live/123",
            "--copy",
            "--save-pgn",
            "a.pgn",
        ])
        .unwrap();

        assert!(cli.copy);
        assert_eq!(cli.save_pgn.as_deref(), Some(std::path::Path::new("a.pgn")));
    }
}
