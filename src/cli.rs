use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "c2l", version, about = "chess.com PGN -> lichess analysis bridge")]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Option<Commands>,

    #[arg(value_name = "URL", help = "chess.com 경기 URL")]
    pub url: Option<String>,

    #[arg(long = "copy", help = "PGN을 클립보드에 복사")]
    pub copy: bool,

    #[arg(long = "open", conflicts_with = "no_open", help = "브라우저 열기")]
    pub open: bool,

    #[arg(long = "print-pgn", help = "PGN을 stdout에 출력")]
    pub print_pgn: bool,

    #[arg(long = "no-open", help = "자동으로 브라우저를 열지 않음")]
    pub no_open: bool,

    #[arg(long = "save-pgn", value_name = "PATH", help = "PGN을 파일로 저장")]
    pub save_pgn: Option<PathBuf>,

    #[arg(
        long = "raw-url",
        conflicts_with = "print_pgn",
        help = "최종 URL 한 줄 출력"
    )]
    pub raw_url: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// TUI 모드 실행
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
