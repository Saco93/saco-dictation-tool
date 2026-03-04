use tokio::{io::AsyncWriteExt, process::Command};

#[must_use]
pub fn is_available(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

pub async fn copy_to_clipboard(cmd: &str, text: &str) -> Result<(), String> {
    let mut child = Command::new(cmd)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("process exited with status {status}"))
    }
}
