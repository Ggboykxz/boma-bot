//! The live stream of a thread (REQ-005, REQ-020).
//!
//! `threads.subscribe` answers with `text/event-stream`. Every frame carries the thread's
//! sequence, so a client that reconnects resumes from the last sequence it saw without losing
//! an event, and a heartbeat proves the connection is alive during silence.
//!
//! The frames are produced by a task that owns the subscription and pushes into a channel: the
//! handler itself only decides *what* the stream is, never *when* a frame is written.

use std::{convert::Infallible, time::Duration};

use axum::{
    body::Bytes,
    response::{IntoResponse, Response},
};
use boma_domain::{EventType, TenantContext};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};

use crate::{auth::AuthContext, error::ApiError, state::AppState, wire::EventView};

/// How long the stream waits before sending a heartbeat.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// How many frames may wait before the producer is considered slow.
const CHANNEL_CAPACITY: usize = 64;

/// The body of `threads.subscribe`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SubscribeRequest {
    /// The thread to listen to.
    thread_id: String,
    /// The sequence the client has already seen; the stream continues after it.
    #[serde(default)]
    after_seq: Option<i64>,
}

/// Subscribes to a thread's stream.
///
/// The reply is an event stream, not a JSON body: the frames arrive as they happen.
///
/// # Errors
///
/// Returns [`ApiError::BadRequest`] when the body does not match the schema, and
/// [`ApiError::NotFound`] when the thread does not exist in the caller's tenant.
pub async fn subscribe(
    state: AppState,
    auth: AuthContext,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: SubscribeRequest = serde_json::from_slice(&body).map_err(|error| {
        tracing::debug!(%error, "the subscribe body did not match its schema");
        ApiError::bad_request()
    })?;

    let thread_id =
        boma_domain::ThreadId::new(request.thread_id.clone()).map_err(|_| ApiError::not_found())?;
    let context = auth.actor.context.clone();
    let thread = state
        .app()
        .stores
        .threads
        .find(&context, &thread_id)
        .await?
        .ok_or(ApiError::not_found())?;

    let after_seq = request.after_seq.unwrap_or_default();
    let history = state
        .app()
        .stores
        .events
        .since(&context, &thread_id, after_seq)
        .await?;

    let app = state.app().clone();
    let space_id = thread.space_id.clone();
    let context = auth.actor.context.clone();
    let (sender, receiver) = mpsc::channel::<axum::response::sse::Event>(CHANNEL_CAPACITY);

    tokio::spawn(async move {
        let mut live = app.services.publisher.subscribe(&thread_id);
        let mut last_seq = after_seq;

        for event in history {
            last_seq = last_seq.max(event.seq);
            if sender.send(frame(&event)).await.is_err() {
                return;
            }
        }

        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                message = live.recv() => {
                    match message {
                        Ok(event) => {
                            if event.seq <= last_seq {
                                continue;
                            }
                            last_seq = event.seq;
                            if sender.send(frame(&event)).await.is_err() {
                                return;
                            }
                        }
                        // A closed channel means the publisher is gone: the stream ends.
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        // A lagging receiver means this subscriber was too slow; it resumes from
                        // the log on the next connection.
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    }
                }
                _ = ticker.tick() => {
                    match heartbeat(&app, &context, &space_id, &thread_id).await {
                        Ok(event) => {
                            last_seq = last_seq.max(event.seq);
                            if sender.send(frame(&event)).await.is_err() {
                                return;
                            }
                        }
                        Err(error) => {
                            tracing::debug!(%error, "the heartbeat could not be written");
                        }
                    }
                }
            }
        }
    });

    let stream = ReceiverStream::new(receiver).map(Ok::<_, Infallible>);
    Ok(axum::response::sse::Sse::new(stream).into_response())
}

/// Encodes one event as an SSE frame.
fn frame(event: &boma_domain::Event) -> axum::response::sse::Event {
    let view = EventView::from(event);
    axum::response::sse::Event::default()
        .event(view.kind.clone())
        .json_data(view)
        .unwrap_or_else(|_| axum::response::sse::Event::default().data("{}"))
}

/// Writes one heartbeat into the thread's log and publishes it.
///
/// The heartbeat carries a sequence like every other frame, so a client that counts sequences
/// sees no gap (REQ-005).
async fn heartbeat(
    app: &boma_app::App,
    context: &TenantContext,
    space_id: &boma_domain::SpaceId,
    thread_id: &boma_domain::ThreadId,
) -> Result<boma_domain::Event, boma_app::AppError> {
    let event = app
        .stores
        .events
        .append(
            context,
            boma_app::ports::NewEvent {
                space_id: space_id.clone(),
                thread_id: thread_id.clone(),
                kind: EventType::Heartbeat,
                payload: serde_json::json!({}),
                created_at: app.services.clock.now(),
            },
        )
        .await?;
    app.services.publisher.publish(event.clone());
    Ok(event)
}

/// The stream's item type, for the callers that name it.
pub type FrameStream = ReceiverStream<mpsc::Receiver<axum::response::sse::Event>>;

/// The stream never fails: a frame that cannot be encoded is skipped, not fatal.
pub type FrameResult = Result<axum::response::sse::Event, Infallible>;

#[cfg(test)]
mod tests {
    use super::HEARTBEAT_INTERVAL;

    #[test]
    fn req_005_the_heartbeat_interval_is_shorter_than_a_proxy_timeout() {
        assert!(HEARTBEAT_INTERVAL.as_secs() <= 15);
        assert!(HEARTBEAT_INTERVAL > std::time::Duration::from_secs(0));
    }
}
