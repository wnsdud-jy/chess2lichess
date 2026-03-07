use anyhow::{anyhow, Result};

pub fn open_url(url: &str) -> Result<()> {
    webbrowser::open(url)
        .map_err(|e| anyhow!(format!("브라우저 실행 실패: {e}")))
        .map(|_| ())
}
