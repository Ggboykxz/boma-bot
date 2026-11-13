//! A minimal CDP (Chromium DevTools Protocol) client for the computer's browser.
//!
//! The computer runs a headless Chromium with `--remote-debugging-port=0` (or a fixed one the
//! deployment publishes); this client talks the websocket of a page target and translates the
//! handful of commands the computer tools use. No general CDP surface: every method here is one
//! tool, small and testable.

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite};

/// The ceiling for a single CDP call, in seconds.
const CDP_TIMEOUT_SECONDS: u64 = 10;

/// How long [`Cdp::connect`] keeps trying to reach a browser that is still coming up, in seconds.
///
/// A container's published port accepts connections before Chromium listens behind it, so the
/// first answers can be empty or the websocket handshake can stall. The budget is short: a
/// computer that is genuinely down must report `Unavailable`, not hold the caller.
const CDP_CONNECT_SECONDS: u64 = 5;

/// The pause between two connection attempts, in milliseconds.
const CDP_CONNECT_PAUSE_MILLIS: u64 = 200;

/// A CDP failure the use cases translate into [`boma_app::AppError::Unavailable`].
#[derive(Debug, thiserror::Error)]
pub enum CdpError {
    /// The websocket could not be opened or a message did not parse.
    ///
    /// Boxed: a transport error is large, and it travels through the whole call chain.
    #[error("{0}")]
    Transport(Box<tungstenite::Error>),
    /// The browser answered with a protocol error.
    #[error("{0}")]
    Protocol(String),
    /// An evaluated expression raised inside the page: the caller's fault, not the transport's.
    #[error("{0}")]
    Exception(String),
    /// A CDP call did not answer in time.
    #[error("the CDP call timed out")]
    Timeout,
}

impl From<tungstenite::Error> for CdpError {
    fn from(error: tungstenite::Error) -> Self {
        Self::Transport(Box::new(error))
    }
}

/// One CDP session: the page target of one computer, fresh per call.
pub struct Cdp {
    /// The websocket of the page target.
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// The id of the next CDP call.
    next_id: u64,
}

impl Cdp {
    /// Connects to the debug endpoint of the computer.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] for a transport failure or a page that never appears.
    pub async fn connect(port: u16) -> Result<Self, CdpError> {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(CDP_CONNECT_SECONDS);
        let ws = loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(CdpError::Timeout);
            }
            match Self::connect_once(port, deadline).await {
                Ok(ws) => break ws,
                Err(error) if std::time::Instant::now() < deadline => {
                    tracing::debug!(%error, port, "the CDP endpoint is not ready yet");
                    tokio::time::sleep(std::time::Duration::from_millis(CDP_CONNECT_PAUSE_MILLIS))
                        .await;
                }
                Err(error) => return Err(error),
            }
        };
        let mut cdp = Self { ws, next_id: 1 };
        // The domains the computer tools use; every other CDP call is refused.
        cdp.call("Page.enable", json!({})).await?;
        cdp.call("Runtime.enable", json!({})).await?;
        Ok(cdp)
    }

    /// One connection attempt, bounded by `deadline`: fetch the page target, then open its
    /// websocket. Both steps carry the timeout, so a half-open published port cannot hang here.
    async fn connect_once(
        port: u16,
        deadline: std::time::Instant,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, CdpError> {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(CdpError::Timeout);
        }
        let list = tokio::time::timeout(
            remaining,
            http_get(format!("http://127.0.0.1:{port}/json/list")),
        )
        .await
        .map_err(|_| CdpError::Timeout)??;
        let target = list
            .as_array()
            .and_then(|pages| pages.iter().find(|page| page["type"] == "page"))
            .ok_or_else(|| {
                CdpError::Protocol("no page target is exposed by the browser".to_owned())
            })?;
        let ws_url = target["webSocketDebuggerUrl"].as_str().ok_or_else(|| {
            CdpError::Protocol("the page target carries no ws endpoint".to_owned())
        })?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(CdpError::Timeout);
        }
        let (ws, _) = tokio::time::timeout(remaining, connect_async(ws_url))
            .await
            .map_err(|_| CdpError::Timeout)??;
        Ok(ws)
    }

    /// Evaluates an expression and returns the value, or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] when the expression errors, the session drops, or the browser refuses.
    pub async fn evaluate(&mut self, expression: &str) -> Result<Value, CdpError> {
        let result = self
            .call(
                "Runtime.evaluate",
                json!({
                    "expression": expression,
                    "returnByValue": true,
                }),
            )
            .await?;
        if let Some(exception) = result.get("exceptionDetails") {
            let text = result["result"]["description"]
                .as_str()
                .unwrap_or("the expression threw");
            return Err(CdpError::Exception(format!("{exception}: {text}")));
        }
        Ok(result["result"]["value"].clone())
    }

    /// Captures the page as a PNG.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] when the capture fails or the bytes are not base64.
    pub async fn screenshot(&mut self) -> Result<Vec<u8>, CdpError> {
        use base64::Engine as _;

        let result = self
            .call("Page.captureScreenshot", json!({"format": "png"}))
            .await?;
        let data = result["data"]
            .as_str()
            .ok_or_else(|| CdpError::Protocol("the screenshot carries no bytes".to_owned()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|error| {
                CdpError::Protocol(format!("the screenshot is not base64: {error}"))
            })?;
        Ok(bytes)
    }

    /// Navigates the page.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] on a refused navigation.
    pub async fn navigate(&mut self, url: &str) -> Result<(), CdpError> {
        self.call("Page.navigate", json!({ "url": url })).await?;
        Ok(())
    }

    /// Clicks at a viewport point.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] when the browser refuses the event.
    pub async fn click(&mut self, x: f64, y: f64) -> Result<(), CdpError> {
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mousePressed",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
                "pointerType": "mouse",
            }),
        )
        .await?;
        self.call(
            "Input.dispatchMouseEvent",
            json!({
                "type": "mouseReleased",
                "x": x,
                "y": y,
                "button": "left",
                "clickCount": 1,
                "pointerType": "mouse",
            }),
        )
        .await?;
        Ok(())
    }

    /// Types the text into the focused element, character by character.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] when the browser refuses a character.
    pub async fn type_text(&mut self, text: &str) -> Result<(), CdpError> {
        for character in text.chars() {
            self.call(
                "Input.dispatchKeyEvent",
                json!({
                    "type": "char",
                    "text": character.to_string(),
                }),
            )
            .await?;
        }
        Ok(())
    }

    /// Scrolls the page by `delta_x` and `delta_y` pixels.
    ///
    /// # Errors
    ///
    /// Returns [`CdpError`] when the browser refuses the wheel event.
    pub async fn scroll(&mut self, delta_x: f64, delta_y: f64) -> Result<(), CdpError> {
        // Chromium 124 exposes no `Input.dispatchWheelEvent`; a synthesized scroll gesture is the
        // supported way to send real wheel events. Its Y distance is positive to scroll *up*, so
        // the tool's delta (positive scrolls down, like the DOM) is negated here.
        self.call(
            "Input.synthesizeScrollGesture",
            json!({
                "x": 100.0,
                "y": 100.0,
                "xDistance": -delta_x,
                "yDistance": -delta_y,
                "speed": 8_000,
                "gestureSourceType": "mouse",
            }),
        )
        .await?;
        Ok(())
    }

    /// Sends a CDP command and waits for its answer, discarding events.
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, CdpError> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({ "id": id, "method": method, "params": params });
        self.ws
            .send(tungstenite::Message::Text(message.to_string().into()))
            .await?;
        loop {
            let Some(frame) = tokio::time::timeout(
                std::time::Duration::from_secs(CDP_TIMEOUT_SECONDS),
                self.ws.next(),
            )
            .await
            .map_err(|_| CdpError::Timeout)?
            else {
                return Err(CdpError::Transport(Box::new(
                    tungstenite::Error::ConnectionClosed,
                )));
            };
            let message = frame?;
            let tungstenite::Message::Text(text) = message else {
                continue;
            };
            let answer: Value = serde_json::from_str(&text)
                .map_err(|error| CdpError::Protocol(format!("unparseable CDP frame: {error}")))?;
            match answer.get("id").and_then(Value::as_u64) {
                Some(answer_id) if answer_id == id => {
                    if let Some(error) = answer.get("error") {
                        let message = error["message"].as_str().unwrap_or("unknown CDP error");
                        return Err(CdpError::Protocol(message.to_owned()));
                    }
                    return Ok(answer.get("result").cloned().unwrap_or(Value::Null));
                }
                // A browser event, not our answer: dropped on the floor on purpose.
                _ => {}
            }
        }
    }
}

/// Fetches a URL over HTTP/1.1 and returns the whole body, with no external client.
///
/// Chromium's DevTools HTTP server answers nothing at all to HTTP/1.0 requests, and it rejects
/// a `Host` value whose shape it does not recognize; the request line and header below are the
/// ones curl sends, verified to return 200.
async fn http_get(url: String) -> Result<Value, CdpError> {
    let stripped = url
        .strip_prefix("http://")
        .ok_or_else(|| CdpError::Protocol("only http:// URLs are supported".to_owned()))?;
    let (authority, path) = match stripped.find('/') {
        Some(index) => (stripped[..index].to_owned(), stripped[index..].to_owned()),
        None => (stripped.to_owned(), "/".to_owned()),
    };

    let mut stream = TcpStream::connect(authority.as_str())
        .await
        .map_err(|error| {
            tracing::warn!(%authority, %error, "the CDP endpoint is unreachable");
            CdpError::Transport(Box::new(tungstenite::Error::Io(error)))
        })?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: */*\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .map_err(tungstenite::Error::from)?;
    // Chromium ignores `Connection: close` and never closes the socket, so `read_to_end` would
    // hang: read the headers, then exactly the body length they announce.
    let mut raw = Vec::new();
    let mut chunk = [0_u8; 4096];
    let body_start = loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(tungstenite::Error::from)?;
        if read == 0 {
            return Err(CdpError::Protocol(
                "the CDP endpoint closed before its headers".to_owned(),
            ));
        }
        raw.extend_from_slice(&chunk[..read]);
        if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            break split + 4;
        }
        if raw.len() > 64 * 1024 {
            return Err(CdpError::Protocol(
                "the CDP endpoint answered with oversized headers".to_owned(),
            ));
        }
    };
    let headers = String::from_utf8_lossy(&raw[..body_start]);
    let length = headers
        .split("\r\n")
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| {
            CdpError::Protocol("the CDP endpoint answered with no content length".to_owned())
        })?;
    let mut body = raw[body_start..].to_vec();
    while body.len() < length {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(tungstenite::Error::from)?;
        if read == 0 {
            return Err(CdpError::Protocol(
                "the CDP endpoint answered with a truncated body".to_owned(),
            ));
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > 16 * 1024 * 1024 {
            return Err(CdpError::Protocol(
                "the CDP endpoint answered with an oversized body".to_owned(),
            ));
        }
    }
    body.truncate(length);
    serde_json::from_slice(&body).map_err(|error| {
        CdpError::Protocol(format!(
            "the CDP endpoint answered with unparseable JSON: {error}"
        ))
    })
}
