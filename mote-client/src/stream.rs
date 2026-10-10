use mote_eventlog::Event;
use mote_ipc::EventStream;
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

const MIN_RETRY: Duration = Duration::from_millis(20);
const MAX_RETRY: Duration = Duration::from_secs(1);

pub fn run(path: &Path, scope_id: &str, cursor: i64, tx: Sender<Event>) {
    run_with(
        cursor,
        |cursor| EventStream::connect(path, scope_id, cursor),
        EventStream::read_event,
        |event| tx.send(event).is_ok(),
        crate::app::save_cursor,
        std::thread::sleep,
    );
}

fn run_with<S>(
    initial_cursor: i64,
    mut connect: impl FnMut(i64) -> io::Result<S>,
    mut read: impl FnMut(&mut S) -> io::Result<Option<Event>>,
    mut deliver: impl FnMut(Event) -> bool,
    mut persist: impl FnMut(i64),
    mut wait: impl FnMut(Duration),
) {
    let mut cursor = initial_cursor;
    let mut retry = MIN_RETRY;

    loop {
        let mut source = match connect(cursor) {
            Ok(source) => source,
            Err(_) => {
                wait(retry);
                retry = (retry * 2).min(MAX_RETRY);
                continue;
            }
        };

        loop {
            match read(&mut source) {
                Ok(None) => {}
                Ok(Some(event)) if event.kind == "resync" => break,
                Ok(Some(event)) if event.seq <= cursor => {}
                Ok(Some(event)) => {
                    let delivered_cursor = event.seq;
                    if !deliver(event) {
                        return;
                    }
                    cursor = delivered_cursor;
                    persist(cursor);
                    retry = MIN_RETRY;
                }
                Err(_) => {
                    wait(retry);
                    retry = (retry * 2).min(MAX_RETRY);
                    break;
                }
            }
        }
    }
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
        let persisted = Arc::new(Mutex::new(Vec::new()));
        let waits = Arc::new(Mutex::new(Vec::new()));

        run_with(
            0,
            {
                let plans = plans.clone();
                let cursors = cursors.clone();
                move |cursor| {
                    cursors.lock().unwrap().push(cursor);
                    plans.lock().unwrap().pop_front().unwrap()
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            {
                let delivered = delivered.clone();
                move |event| {
                    if event.seq == 3 {
                        return false;
                    }
                    delivered.lock().unwrap().push(event.seq);
                    true
                }
            },
            {
                let persisted = persisted.clone();
                move |cursor| persisted.lock().unwrap().push(cursor)
            },
            {
                let waits = waits.clone();
                move |delay| waits.lock().unwrap().push(delay)
            },
        );

        assert_eq!(*cursors.lock().unwrap(), vec![0, 0, 1, 1]);
        assert_eq!(*delivered.lock().unwrap(), vec![1, 2]);
        assert_eq!(*persisted.lock().unwrap(), vec![1, 2]);
        assert_eq!(
            *waits.lock().unwrap(),
            vec![Duration::from_millis(20), Duration::from_millis(20)]
        );
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
                        Ok(source(vec![Ok(Some(event(1, "turn.user")))]))
                    }
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            |_| false,
            |_| {},
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
}
