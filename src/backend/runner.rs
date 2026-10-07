use super::{BrewError, Result};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{mpsc, oneshot},
};
#[derive(Debug)]
pub enum CommandEvent {
    Cancelled,
    Output(String),
    Finished { success: bool, code: Option<i32> },
}
pub struct CommandHandle {
    pub events: mpsc::Receiver<CommandEvent>,
    cancel: Option<oneshot::Sender<()>>,
}
impl CommandHandle {
    pub fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
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
    let mut cmd = command(path, args);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn()?;
    #[cfg(unix)]
    let group = child.id().expect("spawned process") as i32;
    let (cancel, mut cancellation) = oneshot::channel();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (tx, events) = mpsc::channel(128);
    tokio::spawn(async move {
        let out = tokio::spawn(pipe(stdout, tx.clone()));
        let err = tokio::spawn(pipe(stderr, tx.clone()));
        let mut cancelled = false;
        let status = tokio::select! {
            status = child.wait() => status,
            request = &mut cancellation => {
                if request.is_ok() {
                    cancelled = true;
                    #[cfg(unix)]
                    // The child owns this process group; never signal the UI's group.
                    unsafe { libc::kill(-group, libc::SIGTERM); }
                    #[cfg(not(unix))]
                    let _ = child.start_kill();
                    let status = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
                    #[cfg(unix)]
                    unsafe { libc::kill(-group, libc::SIGKILL); }
                    match status {
                        Ok(status) => status,
                        Err(_) => {
                            let _ = child.start_kill();
                            child.wait().await
                        }
                    }
                } else {
                    child.wait().await
                }
            }
        };
        if cancelled {
            out.abort();
            err.abort();
        }
        let _ = tokio::join!(out, err);
        if cancelled {
            let _ = tx.send(CommandEvent::Cancelled).await;
            return;
        }
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
    Ok(CommandHandle {
        events,
        cancel: Some(cancel),
    })
}

/// Executes a reviewed plan in order, stopping at the first failure or cancellation.
pub fn spawn_plan(path: &Path, plan: Vec<Vec<String>>) -> Result<CommandHandle> {
    if plan.len() == 1 {
        return spawn(path, &plan[0]);
    }
    if plan.is_empty() {
        return Err(BrewError::Unsupported("Empty command plan".into()));
    }
    let path = path.to_path_buf();
    let (tx, events) = mpsc::channel(128);
    let (cancel, mut cancellation) = oneshot::channel();
    tokio::spawn(async move {
        let mut cancellation_received = false;
        let mut stopping = false;
        for args in plan {
            match cancellation.try_recv() {
                Ok(()) => {
                    let _ = tx.send(CommandEvent::Cancelled).await;
                    return;
                }
                Err(oneshot::error::TryRecvError::Closed) => cancellation_received = true,
                Err(oneshot::error::TryRecvError::Empty) => {}
            }
            if tx
                .send(CommandEvent::Output(format!("› brew {}\n", args.join(" "))))
                .await
                .is_err()
            {
                return;
            }
            let mut handle = match spawn(&path, &args) {
                Ok(handle) => handle,
                Err(e) => {
                    let _ = tx.send(CommandEvent::Output(e.to_string())).await;
                    let _ = tx
                        .send(CommandEvent::Finished {
                            success: false,
                            code: None,
                        })
                        .await;
                    return;
                }
            };
            loop {
                tokio::select! {
                    request = &mut cancellation, if !cancellation_received => {
                        cancellation_received = true;
                        if request.is_ok() { stopping = true; handle.cancel(); }
                    }
                    event = handle.events.recv() => {
                        match event {
                            Some(CommandEvent::Finished { success: true, .. }) => {
                                if stopping { let _ = tx.send(CommandEvent::Cancelled).await; return; }
                                break;
                            },
                            Some(event @ (CommandEvent::Finished { .. } | CommandEvent::Cancelled)) => {
                                let _ = tx.send(event).await;
                                return;
                            }
                            Some(event) => {
                                if tx.send(event).await.is_err() {
                                    handle.cancel();
                                    while handle.events.recv().await.is_some() {}
                                    return;
                                }
                            }
                            None => {
                                let _ = tx.send(CommandEvent::Finished { success: false, code: None }).await;
                                return;
                            }
                        }
                    }
                }
            }
        }
        let _ = tx
            .send(CommandEvent::Finished {
                success: true,
                code: Some(0),
            })
            .await;
    });
    Ok(CommandHandle {
        events,
        cancel: Some(cancel),
    })
}
