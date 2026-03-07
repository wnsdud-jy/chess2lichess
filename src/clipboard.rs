use anyhow::{anyhow, Result};

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| anyhow!(format!("Failed to initialize clipboard: {e}")))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|e| anyhow!(format!("Failed to write clipboard: {e}")))
}
