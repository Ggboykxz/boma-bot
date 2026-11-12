//! The live subscribers: who is listening to a thread right now.
//!
//! One channel per thread, so a subscriber only ever receives the events of the thread it asked
//! for. The channel is created when the first subscriber arrives and kept until the process ends:
//! a client that reconnects replays the log from its last sequence first, then listens
//! (REQ-005, REQ-020).

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use boma_app::ports::EventPublisher;
use boma_domain::{Event, ThreadId};
use tokio::sync::broadcast;

/// How many events a slow subscriber may fall behind before it loses the oldest ones.
const CHANNEL_CAPACITY: usize = 256;

/// The publisher the live subscribers listen to.
#[derive(Debug)]
pub struct BroadcastPublisher {
    channels: Mutex<HashMap<ThreadId, broadcast::Sender<Event>>>,
    capacity: usize,
}

impl BroadcastPublisher {
    /// Builds the publisher with the default capacity.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(CHANNEL_CAPACITY)
    }

    /// Builds the publisher with a capacity of its own.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    /// The channels, one per thread.
    fn channels(&self) -> MutexGuard<'_, HashMap<ThreadId, broadcast::Sender<Event>>> {
        // A poisoned lock means a thread panicked while holding it. The map is still sound, so
        // the lock is taken back rather than bringing the process down.
        match self.channels.lock() {
            Ok(channels) => channels,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The channel of a thread, created when it does not exist yet.
    fn channel(&self, thread_id: &ThreadId) -> broadcast::Sender<Event> {
        let mut channels = self.channels();
        channels
            .entry(thread_id.clone())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .clone()
    }
}

impl Default for BroadcastPublisher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventPublisher for BroadcastPublisher {
    fn publish(&self, event: Event) {
        let sender = self.channel(&event.thread_id);
        // A send fails only when nobody listens, which is not an error: the event stays in the
        // log, and a client that reconnects replays it from its last sequence.
        let _ = sender.send(event);
    }

    fn subscribe(&self, thread_id: &ThreadId) -> broadcast::Receiver<Event> {
        self.channel(thread_id).subscribe()
    }
}
