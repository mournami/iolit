//! The request loop shared by every transport: one JSON line in, one out.
use crate::{engine::Engine, operation::OperationManager, service::Daemon};
use anyhow::{Result, bail};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use zapret_core::api::*;

/// Serves one client connection: reads a single command and writes the reply.
pub(crate) async fn client<E, S>(daemon: Arc<Daemon<E>>, stream: S)
where
    E: Engine + 'static,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        let mut stream = BufReader::new(stream);
        let mut line = Vec::new();
        loop {
            let buf = stream.fill_buf().await?;
            if buf.is_empty() {
                bail!("Пустой запрос");
            }
            let n = buf
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(buf.len());
            if line.len() + n > zapret_core::ipc::MAX_MESSAGE {
                bail!("Слишком большой запрос");
            }
            line.extend_from_slice(&buf[..n]);
            stream.consume(n);
            if line.last() == Some(&b'\n') {
                break;
            }
        }
        let result = match serde_json::from_slice::<DaemonCommand>(&line) {
            Ok(req) => daemon.dispatch(req).await,
            Err(e) => Err(e.into()),
        };
        let response = match result {
            Ok(data) => ApiResponse {
                ok: true,
                data: Some(data),
                error: None,
            },
            Err(e) => ApiResponse {
                ok: false,
                data: None,
                error: Some(e.to_string()),
            },
        };
        let mut data = serde_json::to_vec(&response)?;
        data.push(b'\n');
        stream.get_mut().write_all(&data).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let _ = outcome;
}

/// Stops background work and the engine when the service is asked to stop.
pub(crate) async fn shutdown<E: Engine + 'static>(
    daemon: &Daemon<E>,
    operations: &OperationManager,
) -> Result<()> {
    operations.cancel();
    // A running app update holds the gate until the installer stops this
    // service, which is happening now: waiting for it would hold the stop
    // until the update gives up.
    let _gate = if operations.app.lock().await.installing {
        None
    } else {
        Some(operations.gate.lock().await)
    };
    daemon.controller.lock().await.stop()?;
    Ok(())
}
