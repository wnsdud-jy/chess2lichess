use anyhow::{anyhow, Result};

pub fn open_url(url: &str) -> Result<()> {
    webbrowser::open(url)
        .map_err(|e| anyhow!(format!("Failed to open browser: {e}")))
        .map(|_| ())
}
