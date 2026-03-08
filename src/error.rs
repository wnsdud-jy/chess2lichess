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

    #[error("Temporary HTTP error while requesting {url}: status={status}")]
    RetryableHttp {
        url: String,
        status: u16,
        message: String,
    },

    #[error("Temporary request failure while requesting {url}: {message}")]
    RetryableRequest { url: String, message: String },
}
