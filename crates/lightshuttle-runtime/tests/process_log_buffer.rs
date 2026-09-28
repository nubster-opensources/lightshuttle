//! Behaviour of `ProcessLogBuffer`: a bounded, in-memory ring of log chunks
//! that serves both a replay of what a supervised process has already
//! printed and a live follow of what it prints from now on.
//!
//! Nothing here needs a Docker daemon or a real process: the buffer is a
//! pure in-memory structure, exercised directly through the chunks it is
//! handed.

use std::time::SystemTime;

use lightshuttle_runtime::{LogChunk, LogStream, ProcessLogBuffer};

/// Builds a chunk carrying `payload` on `stream`, timestamped at the moment
/// of the call. The exact timestamp does not matter to these tests: only
/// the payload and the stream are used to tell chunks apart.
fn chunk(stream: LogStream, payload: &str) -> LogChunk {
    LogChunk {
        stream,
        timestamp: SystemTime::now(),
        bytes: payload.as_bytes().to_vec(),
    }
}

fn stdout_chunk(payload: &str) -> LogChunk {
    chunk(LogStream::Stdout, payload)
}

/// Payloads of `chunks`, in order, decoded as UTF-8 for easy comparison.
fn payloads(chunks: &[LogChunk]) -> Vec<String> {
    chunks
        .iter()
        .map(|c| String::from_utf8(c.bytes.clone()).expect("test payloads are valid UTF-8"))
        .collect()
}

#[test]
fn oldest_chunks_are_evicted_once_the_chunk_count_bound_is_reached() {
    // A generous byte bound so only the chunk-count bound can trigger.
    let buffer = ProcessLogBuffer::with_limits(3, 1_000_000);

    for i in 0..5 {
        buffer.push(stdout_chunk(&format!("line-{i}")));
    }

    let retained = payloads(&buffer.replay());
    assert_eq!(
        retained,
        vec!["line-2", "line-3", "line-4"],
        "only the 3 most recently pushed chunks must survive a bound of 3"
    );
}

#[test]
fn oldest_chunks_are_evicted_once_the_byte_bound_is_reached_regardless_of_chunk_count() {
    // A generous chunk-count bound so only the byte bound can trigger. Each
    // payload below is exactly 10 bytes ("line-" is 5 bytes, plus a 5-digit
    // zero-padded index), so the arithmetic in this test is exact: a bound
    // of 25 bytes leaves room for exactly 2 whole chunks (20 bytes), never 3
    // (30 bytes).
    let buffer = ProcessLogBuffer::with_limits(1_000, 25);

    for i in 0..5 {
        buffer.push(stdout_chunk(&format!("line-{i:05}")));
    }

    let retained = buffer.replay();
    let total_bytes: usize = retained.iter().map(|c| c.bytes.len()).sum();
    assert!(
        total_bytes <= 25,
        "retained chunks must total at most the byte bound, got {total_bytes} bytes"
    );
    assert_eq!(
        payloads(&retained),
        vec!["line-00003", "line-00004"],
        "the byte bound must evict whole chunks starting from the oldest, \
         independently of how many chunks that leaves"
    );
}

#[test]
fn replay_returns_chunks_oldest_first() {
    let buffer = ProcessLogBuffer::with_limits(10, 10_000);

    buffer.push(stdout_chunk("first"));
    buffer.push(stdout_chunk("second"));
    buffer.push(stdout_chunk("third"));

    assert_eq!(
        payloads(&buffer.replay()),
        vec!["first", "second", "third"],
        "replay must hand chunks back in the order they were produced, \
         oldest first, not just with the right content"
    );
}

#[test]
fn a_subscriber_only_receives_chunks_pushed_after_it_subscribed() {
    let buffer = ProcessLogBuffer::with_limits(10, 10_000);

    buffer.push(stdout_chunk("before-subscribe"));
    let mut follower = buffer.subscribe();
    buffer.push(stdout_chunk("after-subscribe"));

    let received = follower
        .try_recv()
        .expect("a chunk pushed after subscribing must be delivered");
    assert_eq!(
        String::from_utf8(received.bytes).expect("valid UTF-8"),
        "after-subscribe"
    );

    assert!(
        follower.try_recv().is_err(),
        "the chunk pushed before subscribing must never reach this follower"
    );
}

#[test]
fn replay_then_subscribe_join_without_loss_or_duplication() {
    let buffer = ProcessLogBuffer::with_limits(10, 10_000);

    // History: what a caller replays to catch up.
    buffer.push(stdout_chunk("history-0"));
    buffer.push(stdout_chunk("history-1"));

    // The caller takes its history snapshot, then subscribes to continue
    // from exactly that point. No push happens between the two calls, so
    // the join between them must be exact: nothing repeated, nothing
    // skipped.
    let history = buffer.replay();
    let mut follower = buffer.subscribe();

    // Live: produced after the join point above.
    buffer.push(stdout_chunk("live-0"));
    buffer.push(stdout_chunk("live-1"));

    let mut combined = payloads(&history);
    while let Ok(live_chunk) = follower.try_recv() {
        combined.push(String::from_utf8(live_chunk.bytes).expect("valid UTF-8"));
    }

    assert_eq!(
        combined,
        vec!["history-0", "history-1", "live-0", "live-1"],
        "replay followed by subscribe must reconstruct the exact sequence, \
         with no chunk missing and none repeated at the join"
    );
}

#[test]
fn stdout_and_stderr_chunks_are_both_retained_and_distinguishable() {
    let buffer = ProcessLogBuffer::with_limits(10, 10_000);

    buffer.push(chunk(LogStream::Stdout, "on-stdout"));
    buffer.push(chunk(LogStream::Stderr, "on-stderr"));

    let retained = buffer.replay();
    assert_eq!(retained.len(), 2, "both streams must be retained");

    let on_stdout = retained
        .iter()
        .find(|c| c.stream == LogStream::Stdout)
        .expect("the stdout chunk must still be present");
    let on_stderr = retained
        .iter()
        .find(|c| c.stream == LogStream::Stderr)
        .expect("the stderr chunk must still be present");

    assert_eq!(
        String::from_utf8(on_stdout.bytes.clone()).expect("valid UTF-8"),
        "on-stdout"
    );
    assert_eq!(
        String::from_utf8(on_stderr.bytes.clone()).expect("valid UTF-8"),
        "on-stderr"
    );
}
