//! mote-client: a native eframe (egui) window. It connects to the daemon socket, subscribes
//! from its saved cursor, shows threads as chat with delivery labels and a typing indicator, renders
//! surfaces inline through mote-ui, and offers a pipeline view. See docs/rfcs/v0.md sections 13
//! and 14.
//!
//! Running this needs a desktop session; the V0-43..50 screenshots are captured there.

mod app;
mod fixture;
mod gallery;
mod pipeline;
mod state;
mod stream;
mod surface;
mod theme;

use std::sync::mpsc;

fn main() -> eframe::Result<()> {
    let fixture = match fixture::load(std::env::args_os().skip(1)) {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let socket = mote_paths::socket_path();
    let (tx, rx) = mpsc::channel::<mote_eventlog::Event>();

    if fixture.is_none() {
        let cursor = app::load_cursor();
        let sock = socket.clone();
        std::thread::spawn(move || {
            stream::run(&sock, "main", cursor, tx);
        });
    }

    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "mote",
        options,
        Box::new(move |cc| Ok(Box::new(app::MoteApp::new(cc, socket, rx, fixture)))),
    )
}
