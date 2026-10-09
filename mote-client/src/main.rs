//! mote-client: a native eframe (egui) window. It connects to the daemon socket, subscribes
//! from its saved cursor, shows threads as chat with status dots and a typing indicator, renders
//! surfaces inline through mote-ui, and offers a pipeline view. See docs/rfcs/v0.md sections 13
//! and 14.
//!
//! Running this needs a desktop session; the V0-43..50 screenshots are captured there.

mod app;

use std::sync::mpsc;

fn main() -> eframe::Result<()> {
    let socket = mote_paths::socket_path();
    let (tx, rx) = mpsc::channel::<mote_eventlog::Event>();

    // Background thread streams daemon events into the UI, resuming from the saved cursor.
    let cursor = app::load_cursor();
    let sock = socket.clone();
    std::thread::spawn(move || {
        if let Ok(mut stream) = mote_ipc::EventStream::connect(&sock, "main", cursor) {
            loop {
                if let Some(ev) = stream.next_event() {
                    app::save_cursor(ev.seq);
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "mote",
        options,
        Box::new(move |cc| Ok(Box::new(app::MoteApp::new(cc, socket, rx)))),
    )
}
