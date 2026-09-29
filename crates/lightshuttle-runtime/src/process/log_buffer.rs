//! Bounded in-memory ring of log chunks produced by a native process.

use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::broadcast;

use crate::runtime::LogChunk;

/// Number of chunks retained when no limit is given.
pub(crate) const DEFAULT_MAX_CHUNKS: usize = 2_048;

/// Number of bytes retained when no limit is given.
pub(crate) const DEFAULT_MAX_BYTES: usize = 1_048_576;

/// Capacity of the broadcast channel serving followers.
const BROADCAST_CAPACITY: usize = 256;

/// Bounded ring of log chunks, serving both replay and follow.
///
/// A container's logs are held by the daemon, which we ask again whenever we
/// need them. Nothing holds the logs of a native process, so this supervisor
/// keeps them itself.
///
/// It keeps them **in memory only**, and within a bound on both the number of
/// chunks and their total size. Neither half of that sentence is incidental.
/// A program's output holds whatever it decides to print, credentials
/// included, so writing it to disk would create a file nobody asked for and
/// nobody would think to clean up. And an unbounded buffer would turn a
/// chatty process into a memory leak that takes the developer's machine down
/// with it.
///
/// The consequence is accepted: output older than the bound is gone, and a
/// `logs` invocation replays what remains, not everything the process ever
/// wrote.
pub struct ProcessLogBuffer {
    retained: Mutex<VecDeque<LogChunk>>,
    max_chunks: usize,
    max_bytes: usize,
    followers: broadcast::Sender<LogChunk>,
}

impl ProcessLogBuffer {
    /// Builds a buffer retaining at most `max_chunks` chunks totalling at
    /// most `max_bytes` bytes, whichever bound is reached first.
    #[must_use]
    pub fn with_limits(max_chunks: usize, max_bytes: usize) -> Self {
        let (followers, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            retained: Mutex::new(VecDeque::new()),
            max_chunks,
            max_bytes,
            followers,
        }
    }

    /// Records `chunk`, evicting the oldest chunks until both bounds hold
    /// again, and hands it to every current follower.
    ///
    /// Retention and broadcast happen under the same lock, which is what makes
    /// [`Self::replay_and_subscribe`] exact. A chunk larger than the whole
    /// byte bound is broadcast to followers and retained by nobody: a bound
    /// that a single chunk could overrun would not be a bound.
    pub fn push(&self, chunk: LogChunk) {
        let mut retained = self.retained.lock().expect("retained mutex poisoned");
        retained.push_back(chunk.clone());

        while retained.len() > self.max_chunks {
            retained.pop_front();
        }

        let mut total: usize = retained.iter().map(|held| held.bytes.len()).sum();
        while total > self.max_bytes {
            match retained.pop_front() {
                Some(evicted) => total -= evicted.bytes.len(),
                None => break,
            }
        }

        // Inside the lock on purpose. A broadcast send never blocks: it drops
        // the oldest value for a follower that has fallen behind rather than
        // waiting for it, so holding the lock across it cannot stall the
        // process being supervised.
        let _ = self.followers.send(chunk);
    }

    /// Chunks currently retained, oldest first.
    #[must_use]
    pub fn replay(&self) -> Vec<LogChunk> {
        self.retained
            .lock()
            .expect("retained mutex poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Subscribes to chunks recorded from now on.
    ///
    /// A follower that falls behind the channel capacity loses chunks rather
    /// than slowing the process down: a supervisor must never apply back
    /// pressure to the program it supervises.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LogChunk> {
        self.followers.subscribe()
    }

    /// The retained history together with a subscription that continues from
    /// exactly where it ends.
    ///
    /// Calling [`Self::replay`] and then [`Self::subscribe`] separately leaves
    /// a window: a chunk pushed between the two is retained after the
    /// snapshot was taken and broadcast before the subscription exists, so it
    /// reaches neither half and is lost without a trace. Taking both under one
    /// lock closes it, which is why anything following a live process uses
    /// this rather than the two calls.
    #[must_use]
    pub fn replay_and_subscribe(&self) -> (Vec<LogChunk>, broadcast::Receiver<LogChunk>) {
        let retained = self.retained.lock().expect("retained mutex poisoned");
        let history = retained.iter().cloned().collect();
        let follower = self.followers.subscribe();
        drop(retained);
        (history, follower)
    }
}

impl Default for ProcessLogBuffer {
    fn default() -> Self {
        Self::with_limits(DEFAULT_MAX_CHUNKS, DEFAULT_MAX_BYTES)
    }
}
