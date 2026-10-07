use super::{BrewError, Result};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::mpsc,
};
#[derive(Debug)]
pub enum CommandEvent {
    Output(String),
    Finished { success: bool, code: Option<i32> },
}
pub struct CommandHandle {
    pub events: mpsc::Receiver<CommandEvent>,
}
fn command(path: &Path, args: &[String]) -> Command {
    let mut cmd = Command::new(path);
    cmd.args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_COLOR", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd
}
pub async fn query(path: &Path, args: &[String]) -> Result<Vec<u8>> {
    tracing::debug!(?args, "Homebrew query");
    let output = tokio::time::timeout(Duration::from_secs(60), command(path, args).output())
        .await
        .map_err(|_| BrewError::Timeout)??;
    if !output.status.success() {
        return Err(BrewError::Command(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ));
    }
    Ok(output.stdout)
}
async fn pipe(mut stream: impl AsyncRead + Unpin, tx: mpsc::Sender<CommandEvent>) {
    let mut buf = [0; 4096];
    loop {
        match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                if tx
                    .send(CommandEvent::Output(
                        String::from_utf8_lossy(&buf[..n]).into_owned(),
                    ))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(e) => {
                let _ = tx
                    .send(CommandEvent::Output(format!("Output error: {e}")))
                    .await;
                break;
            }
        }
    }
}
pub fn spawn(path: &Path, args: &[String]) -> Result<CommandHandle> {
    tracing::info!(?args, "Starting Homebrew operation");
    let mut child = command(path, args).spawn()?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, events) = mpsc::channel(128);
    tokio::spawn(async move {
        let out = tokio::spawn(pipe(stdout, tx.clone()));
        let err = tokio::spawn(pipe(stderr, tx.clone()));
        let status = child.wait().await;
        let _ = tokio::join!(out, err);
        match status {
            Ok(status) => {
                let _ = tx
                    .send(CommandEvent::Finished {
                        success: status.success(),
                        code: status.code(),
                    })
                    .await;
            }
            Err(e) => {
                let _ = tx.send(CommandEvent::Output(e.to_string())).await;
                let _ = tx
                    .send(CommandEvent::Finished {
                        success: false,
                        code: None,
                    })
                    .await;
            }
        }
    });
    Ok(CommandHandle { events })
}
