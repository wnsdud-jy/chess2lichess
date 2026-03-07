use thiserror::Error;

#[derive(Error, Debug)]
pub enum C2lError {
    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),
    #[error("Unsupported Chess.com URL format: {0}")]
    UnsupportedUrl(String),
    #[error("Private or restricted game: {0}")]
    PrivateOrUnavailable(String),
    #[error("Failed to retrieve PGN: {0}")]
    PgnUnavailable(String),
}
