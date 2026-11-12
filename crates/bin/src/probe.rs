//! A dependency-free readiness probe for container health checks.
//!
//! `boma probe` opens one TCP connection, sends one HTTP/1.1 request and looks at the status line.
//! It has no TLS, no redirects and no client library on purpose: it runs *inside* the container
//! against its own loopback address, where a full client would only add failure modes.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, bail};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

/// Upper bound on the whole probe, connect included.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Requests `path` from `addr` and succeeds only on `200`.
///
/// # Errors
///
/// Returns an error when the connection fails, the exchange exceeds [`PROBE_TIMEOUT`], or the
/// response status is not `200`.
pub async fn probe(addr: SocketAddr, path: &str) -> anyhow::Result<()> {
    let mut stream = tokio::time::timeout(PROBE_TIMEOUT, TcpStream::connect(addr))
        .await
        .with_context(|| format!("connecting to {addr} timed out"))?
        .with_context(|| format!("cannot connect to {addr}"))?;

    let request = format!("GET {path} HTTP/1.1\r\nhost: {addr}\r\nconnection: close\r\n\r\n");
    tokio::time::timeout(PROBE_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .with_context(|| format!("writing to {addr} timed out"))?
        .with_context(|| format!("cannot write to {addr}"))?;

    let mut buffer = Vec::new();
    tokio::time::timeout(PROBE_TIMEOUT, stream.read_to_end(&mut buffer))
        .await
        .with_context(|| format!("reading from {addr} timed out"))?
        .with_context(|| format!("cannot read from {addr}"))?;

    let response = String::from_utf8_lossy(&buffer);
    let status_line = response.lines().next().unwrap_or_default();
    if !status_line.contains(" 200") {
        bail!("{addr}{path} answered `{status_line}`");
    }
    tracing::info!(%addr, path, "probe succeeded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::probe;

    /// A one-shot HTTP server that answers with the given status line.
    async fn answer(status: &'static str) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("the test binds a port");
        let addr = listener.local_addr().expect("the bound address is known");
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("a client connects");
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await;
            let response = format!("{status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
        });
        addr
    }

    #[tokio::test]
    async fn req_053_the_probe_accepts_a_200() {
        let addr = answer("HTTP/1.1 200 OK").await;
        probe(addr, "/ready")
            .await
            .expect("a 200 is a healthy answer");
    }

    #[tokio::test]
    async fn req_053_the_probe_refuses_anything_but_a_200() {
        let addr = answer("HTTP/1.1 503 Service Unavailable").await;
        let error = probe(addr, "/ready")
            .await
            .expect_err("a 503 is not a healthy answer");
        assert!(error.to_string().contains("503"), "{error}");
    }

    #[tokio::test]
    async fn req_053_the_probe_fails_when_nothing_listens() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("the test binds a port");
        let addr = listener.local_addr().expect("the bound address is known");
        drop(listener);

        probe(addr, "/ready")
            .await
            .expect_err("a closed port is not a healthy answer");
    }
}
