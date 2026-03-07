use thiserror::Error;

#[derive(Error, Debug)]
pub enum C2lError {
    #[error("잘못된 URL 형식: {0}")]
    InvalidUrl(String),
    #[error("지원하지 않는 Chess.com URL 형식: {0}")]
    UnsupportedUrl(String),
    #[error("비공개 또는 접근 제한 경기: {0}")]
    PrivateOrUnavailable(String),
    #[error("PGN 확보 실패: {0}")]
    PgnUnavailable(String),
}
