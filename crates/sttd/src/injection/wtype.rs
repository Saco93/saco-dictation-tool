use tokio::process::Command;

#[must_use]
pub fn is_available(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

pub async fn type_text(cmd: &str, text: &str) -> Result<(), String> {
    let status = Command::new(cmd)
        .arg(text)
        .status()
        .await
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("process exited with status {status}"))
    }
}

pub async fn autopaste_ctrl_v(cmd: &str) -> Result<(), String> {
    let status = Command::new(cmd)
        .args(["-M", "ctrl", "v", "-m", "ctrl"])
        .status()
        .await
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("process exited with status {status}"))
    }
}
