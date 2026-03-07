use anyhow::{anyhow, Result};

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| anyhow!(format!("클립보드 초기화 실패: {e}")))?;
    clipboard
        .set_text(text.to_owned())
        .map_err(|e| anyhow!(format!("클립보드 저장 실패: {e}")))
}
