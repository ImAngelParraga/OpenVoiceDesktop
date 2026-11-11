use anyhow::{Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

static STARTUP: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Start the user service without changing the user's autostart preference.
pub async fn ensure_started(socket: &Path) -> Result<()> {
    let _startup = STARTUP.lock().await;
    let mut service = Command::new("systemctl");
    service.args(["--user", "start", "openvoice-desktop.service"]);
    let mut standalone = Command::new(std::env::current_exe()?);
    standalone.arg("daemon");
    ensure_with_commands(socket, service, standalone, Duration::from_secs(5)).await
}

async fn reachable(socket: &Path) -> bool {
    tokio::time::timeout(Duration::from_millis(500), crate::ipc::send_status(socket))
        .await
        .is_ok_and(|result| result.is_ok())
}

async fn ensure_with_commands(
    socket: &Path,
    mut service: Command,
    mut standalone: Command,
    timeout: Duration,
) -> Result<()> {
    if reachable(socket).await {
        return Ok(());
    }
    for command in [&mut service, &mut standalone] {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    service.kill_on_drop(true);
    tokio::time::timeout(timeout, async {
        let managed = service.status().await.is_ok_and(|status| status.success());
        // Source builds may have no installed user unit. Keep their daemon
        // independent of the window process too.
        let mut child = if managed {
            None
        } else {
            Some(standalone.spawn().context("starting recording daemon")?)
        };
        loop {
            if reachable(socket).await {
                return Ok(());
            }
            if let Some(child) = child.as_mut() {
                if let Some(exit) = child.try_wait()? {
                    anyhow::bail!("recording daemon exited during startup: {exit}");
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .context("recording service did not become ready within the startup deadline")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve_status(socket: &Path) -> tokio::task::JoinHandle<()> {
        let listener = tokio::net::UnixListener::bind(socket).unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = String::new();
            stream.read_to_string(&mut request).await.unwrap();
            assert_eq!(request, crate::ipc::STATUS_COMMAND);
            stream
                .write_all(b"{\"state\":\"Idle\",\"mic_level_bucket\":0,\"language\":null}\n")
                .await
                .unwrap();
        })
    }

    #[tokio::test]
    async fn test_v78_running_daemon_is_neither_restarted_nor_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        let server = serve_status(&socket).await;
        ensure_with_commands(
            &socket,
            Command::new("missing-service-command"),
            Command::new("missing-daemon-command"),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_v78_missing_service_reports_failed_standalone_start() {
        let dir = tempfile::tempdir().unwrap();
        let error = ensure_with_commands(
            &dir.path().join("daemon.sock"),
            Command::new("false"),
            Command::new("false"),
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("exited during startup"));
    }

    #[tokio::test]
    async fn test_v78_service_start_is_bounded() {
        let dir = tempfile::tempdir().unwrap();
        let mut service = Command::new("sleep");
        service.arg("30");
        let error = ensure_with_commands(
            &dir.path().join("daemon.sock"),
            service,
            Command::new("missing-daemon-command"),
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("startup deadline"));
    }

    #[tokio::test]
    async fn test_v78_launch_starts_service_or_standalone_and_waits_for_status() {
        for managed in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("daemon.sock");
            let marker = dir.path().join("started");
            let server_socket = socket.clone();
            let server_marker = marker.clone();
            let server = tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !server_marker.exists() {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    serve_status(&server_socket).await.await.unwrap();
                })
                .await
                .unwrap();
            });
            let mut start = Command::new("sh");
            start
                .args(["-c", "touch \"$1\"; sleep 0.3", "fixture"])
                .arg(&marker);
            let (service, standalone) = if managed {
                (start, Command::new("missing-daemon-command"))
            } else {
                (Command::new("false"), start)
            };
            ensure_with_commands(&socket, service, standalone, Duration::from_secs(2))
                .await
                .unwrap();
            server.await.unwrap();
        }
    }
}
