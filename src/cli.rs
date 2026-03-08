use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
    Csv,
}

#[derive(Parser, Debug)]
#[command(
    name = "c2l",
    version,
    about = "chess.com PGN -> lichess analysis bridge"
)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Option<Commands>,

    #[arg(
        value_name = "URL",
        help = "chess.com game URL(s). Can be repeated for batch mode."
    )]
    pub urls: Vec<String>,

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

    #[arg(long = "json", help = "Print machine-readable JSON output")]
    pub json: bool,

    #[arg(
        long = "quiet",
        help = "Suppress human-readable progress and summary messages"
    )]
    pub quiet: bool,

    #[arg(long = "verbose", help = "Show verbose progress logs")]
    pub verbose: bool,

    #[arg(long = "format", default_value_t = OutputFormat::Text, value_enum)]
    pub format: OutputFormat,

    #[arg(
        long = "input",
        value_name = "PATH",
        help = "Read URLs from a file, one per line"
    )]
    pub input_file: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run TUI mode
    Tui,
    /// Run environment and release checks
    Doctor {
        #[arg(
            long = "check-updates",
            help = "Check GitHub releases for latest version"
        )]
        check_updates: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_url_mode() {
        let cli = Cli::try_parse_from(["c2l", "https://www.chess.com/game/live/123"]).unwrap();
        assert!(cli.mode.is_none());
        assert_eq!(
            cli.urls.as_slice(),
            &["https://www.chess.com/game/live/123".to_string()]
        );
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

    #[test]
    fn parse_batch_urls() {
        let cli = Cli::try_parse_from([
            "c2l",
            "https://www.chess.com/game/live/111",
            "https://www.chess.com/game/live/222",
            "--json",
        ])
        .unwrap();

        assert!(cli.json);
        assert_eq!(cli.urls.len(), 2);
    }

    #[test]
    fn parse_doctor_command() {
        let cli = Cli::try_parse_from(["c2l", "doctor", "--check-updates"]).unwrap();
        assert!(matches!(
            cli.mode,
            Some(Commands::Doctor {
                check_updates: true
            })
        ));
    }
}
