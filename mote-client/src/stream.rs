use mote_eventlog::Event;
use mote_ipc::Client;
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

const MIN_RETRY: Duration = Duration::from_millis(20);
const MAX_RETRY: Duration = Duration::from_secs(1);

pub enum StreamUpdate {
    Event { event: Event, arrived_ns: u64 },
    Connected { replay_through: i64 },
    Disconnected,
}

impl From<Event> for StreamUpdate {
    fn from(event: Event) -> Self {
        Self::timed(event, 0)
    }
}

impl StreamUpdate {
    pub fn timed(event: Event, arrived_ns: u64) -> Self {
        Self::Event { event, arrived_ns }
    }
}

pub fn run(
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
) {
    run_after_connect(path, scope_id, cursor, tx, clock, repaint, || {});
}

pub fn run_after_connect(
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
    on_connected: impl FnOnce(),
) {
    let mut on_connected = Some(on_connected);
    run_with(
        cursor,
        |cursor| {
            let mut client = Client::connect(path)?;
            let status = client.call("status", serde_json::json!({}))?
                .map_err(|error| io::Error::other(error.message))?;
            let replay_through = status["last_seq"].as_i64().filter(|seq| *seq >= 0)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status has no valid last_seq"))?;
            Ok((client.subscribe(scope_id, cursor)?, replay_through))
        },
        |stream| {
            if let Some(on_connected) = on_connected.take() {
                on_connected();
            }
            stream.read_event()
        },
        || clock.now_ns(),
        |event| tx.send(event).is_ok(),
        repaint,
        std::thread::sleep,
    );
}

fn run_with<S>(
    initial_cursor: i64,
    mut connect: impl FnMut(i64) -> io::Result<(S, i64)>,
    mut read: impl FnMut(&mut S) -> io::Result<Option<Event>>,
    mut now_ns: impl FnMut() -> u64,
    mut deliver: impl FnMut(StreamUpdate) -> bool,
    mut wake: impl FnMut(),
    mut wait: impl FnMut(Duration),
) {
    let mut cursor = initial_cursor;
    let mut retry = MIN_RETRY;

    loop {
        let (mut source, replay_through) = match connect(cursor) {
            Ok(connection) => connection,
            Err(_) => {
                wait(retry);
                retry = (retry * 2).min(MAX_RETRY);
                continue;
            }
        };
        if !deliver_and_wake(StreamUpdate::Connected { replay_through }, &mut deliver, &mut wake) {
            return;
        }

        loop {
            match read(&mut source) {
                Ok(None) => {}
                Ok(Some(event)) => {
                    let arrived_ns = now_ns();
                    if event.kind == "resync" {
                        if !deliver_and_wake(StreamUpdate::Disconnected, &mut deliver, &mut wake) {
                            return;
                        }
                        break;
                    }
                    if event.seq <= cursor {
                        continue;
                    }
                    let delivered_cursor = event.seq;
                    if !deliver_and_wake(
                        StreamUpdate::timed(event, arrived_ns), &mut deliver, &mut wake,
                    ) {
                        return;
                    }
                    cursor = delivered_cursor;
                    retry = MIN_RETRY;
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {}
                Err(_) => {
                    if !deliver_and_wake(StreamUpdate::Disconnected, &mut deliver, &mut wake) {
                        return;
                    }
                    wait(retry);
                    retry = (retry * 2).min(MAX_RETRY);
                    break;
                }
            }
        }
    }
}

fn deliver_and_wake(
    update: StreamUpdate,
    mut deliver: impl FnMut(StreamUpdate) -> bool,
    mut wake: impl FnMut(),
) -> bool {
    let delivered = deliver(update);
    if delivered {
        wake();
    }
    delivered
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_eventlog::Event;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeSource {
        reads: VecDeque<io::Result<Option<Event>>>,
    }

    fn event(seq: i64, kind: &str) -> Event {
        Event {
            seq,
            ts: 0,
            scope_id: "main".into(),
            kind: kind.into(),
            payload: serde_json::json!({"seq": seq}),
        }
    }

    fn source(reads: Vec<io::Result<Option<Event>>>) -> FakeSource {
        FakeSource {
            reads: reads.into(),
        }
    }

    #[test]
    fn reconnects_from_the_last_delivered_cursor_after_disconnect_and_resync() {
        let plans = Arc::new(Mutex::new(VecDeque::from([
            Err(io::Error::new(io::ErrorKind::NotFound, "not ready")),
            Ok(source(vec![
                Ok(Some(event(1, "turn.user"))),
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "dropped")),
            ])),
            Ok(source(vec![
                Ok(Some(event(1, "turn.user"))),
                Ok(Some(event(5, "resync"))),
            ])),
            Ok(source(vec![
                Ok(Some(event(2, "talk.delta"))),
                Ok(Some(event(3, "talk.done"))),
            ])),
        ])));
        let cursors = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let waits = Arc::new(Mutex::new(Vec::new()));

        run_with(
            0,
            {
                let plans = plans.clone();
                let cursors = cursors.clone();
                move |cursor| {
                    cursors.lock().unwrap().push(cursor);
                    plans.lock().unwrap().pop_front().unwrap().map(|source| (source, 0))
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            || 0,
            {
                let delivered = delivered.clone();
                move |update| {
                    let event = match update {
                        StreamUpdate::Event { event, .. } => event,
                        StreamUpdate::Disconnected => {
                            delivered.lock().unwrap().push(-1);
                            return true;
                        }
                        StreamUpdate::Connected { .. } => {
                            delivered.lock().unwrap().push(-2);
                            return true;
                        }
                    };
                    if event.seq == 3 {
                        return false;
                    }
                    delivered.lock().unwrap().push(event.seq);
                    true
                }
            },
            || {},
            {
                let waits = waits.clone();
                move |delay| waits.lock().unwrap().push(delay)
            },
        );

        assert_eq!(*cursors.lock().unwrap(), vec![0, 0, 1, 1]);
        assert_eq!(*delivered.lock().unwrap(), vec![-2, 1, -1, -2, -1, -2, 2]);
        assert_eq!(
            *waits.lock().unwrap(),
            vec![Duration::from_millis(20), Duration::from_millis(20)]
        );
    }

    #[test]
    fn malformed_frame_keeps_the_connection_and_committed_surface() {
        let mut reads = VecDeque::from([
            Ok(Some(event(1, "ui.surface"))),
            Err(io::Error::new(io::ErrorKind::InvalidData, "malformed frame")),
            Ok(Some(event(2, "ui.surface"))),
        ]);
        let mut connected = 0;
        let mut delivered = Vec::new();
        run_with(0, |_| { connected += 1; Ok(((), 0)) }, |_| reads.pop_front().unwrap(), || 0,
            |update| match update {
                StreamUpdate::Connected { .. } => true,
                StreamUpdate::Disconnected => panic!("malformed candidate invalidated committed display"),
                StreamUpdate::Event { event, .. } => { delivered.push(event.seq); event.seq != 2 }
            }, || {}, |_| {});
        assert_eq!(connected, 1);
        assert_eq!(delivered, [1, 2]);
    }

    #[test]
    fn reconnect_backoff_is_capped() {
        let failures = Arc::new(Mutex::new(0usize));
        let waits = Arc::new(Mutex::new(Vec::new()));

        run_with(
            0,
            {
                let failures = failures.clone();
                move |_| {
                    let mut failures = failures.lock().unwrap();
                    if *failures < 8 {
                        *failures += 1;
                        Err(io::Error::new(io::ErrorKind::NotFound, "not ready"))
                    } else {
                        Ok((source(vec![Ok(Some(event(1, "turn.user")))]), 0))
                    }
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            || 0,
            |_| false,
            || {},
            {
                let waits = waits.clone();
                move |delay| waits.lock().unwrap().push(delay)
            },
        );

        let waits = waits.lock().unwrap();
        assert_eq!(waits.first(), Some(&Duration::from_millis(20)));
        assert_eq!(waits.last(), Some(&Duration::from_secs(1)));
        assert!(waits.iter().all(|delay| *delay <= Duration::from_secs(1)));
    }

    #[test]
    fn delivered_update_wakes_the_renderer_after_queueing() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let delivered = order.clone();
        let woken = order.clone();
        assert!(deliver_and_wake(
            StreamUpdate::Disconnected,
            move |_| { delivered.lock().unwrap().push("deliver"); true },
            move || woken.lock().unwrap().push("wake"),
        ));
        assert_eq!(*order.lock().unwrap(), ["deliver", "wake"]);
    }

    #[test]
    fn decoded_event_carries_the_clock_offset_before_delivery() {
        let update = StreamUpdate::timed(event(9, "ui.surface"), 123_456);
        match update {
            StreamUpdate::Event { event, arrived_ns } => {
                assert_eq!(event.seq, 9);
                assert_eq!(arrived_ns, 123_456);
            }
            _ => panic!("expected event"),
        }
    }

    #[test]
    fn stream_timestamps_decoded_events_and_wakes_after_successful_delivery() {
        let mut reads = VecDeque::from([
            Ok(Some(event(1, "ui.surface"))),
            Ok(Some(event(2, "ui.surface"))),
        ]);
        let mut times = VecDeque::from([123, 456]);
        let mut arrivals = Vec::new();
        let mut wakes = 0;
        run_with(
            0, |_| Ok(((), 0)), |_| reads.pop_front().unwrap(),
            || times.pop_front().unwrap(),
            |update| match update {
                StreamUpdate::Connected { .. } => true,
                StreamUpdate::Event { event, arrived_ns } => {
                    arrivals.push((event.seq, arrived_ns));
                    event.seq != 2
                }
                StreamUpdate::Disconnected => true,
            },
            || wakes += 1, |_| {},
        );
        assert_eq!(arrivals, [(1, 123), (2, 456)]);
        assert_eq!(wakes, 2, "connected and first queued event must wake");
    }
}
