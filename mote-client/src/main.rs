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

fn matrix_source(args: &[std::ffi::OsString]) -> Result<Option<String>, String> {
    if args.first().is_none_or(|argument| argument != "--matrix") {
        return Ok(None);
    }
    let usage = "usage: mote-client [--gallery | --fixture <json-path> | --matrix <matrix-path>]";
    let path = args.get(1).ok_or(usage)?;
    if args.len() != 2 {
        return Err(usage.into());
    }
    let path = std::path::PathBuf::from(path);
    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|error| format!("cannot read matrix source {}: {error}", path.display()))
}

fn submit_matrix(socket: &std::path::Path, source: &str) -> Result<(), String> {
    let mut client = mote_ipc::Client::connect(socket)
        .map_err(|error| format!("cannot reach daemon: {error}"))?;
    let response = client
        .call(
            "matrix.render",
            serde_json::json!({"thread":"main", "source":source}),
        )
        .map_err(|error| format!("cannot submit matrix source: {error}"))?
        .map_err(|error| format!("{}: {}", error.code, error.message))?;
    if response["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(format!("matrix source rejected: {}", response["diagnostics"]))
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let source = match matrix_source(&args) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let fixture = match if source.is_some() {
        Ok(None)
    } else {
        fixture::load(args.into_iter())
    } {
        Ok(fixture) => fixture,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let socket = mote_paths::socket_path();
    if let Some(source) = source {
        if let Err(error) = submit_matrix(&socket, &source) {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn matrix_flag_loads_utf8_source() {
        let path = std::env::temp_dir().join(format!(
            "mote-client-matrix-{}-{}.matrix",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        std::fs::write(&path, "screen { text \"hello\" }").unwrap();
        let args = vec![OsString::from("--matrix"), path.clone().into_os_string()];

        assert_eq!(
            matrix_source(&args).unwrap().as_deref(),
            Some("screen { text \"hello\" }")
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn non_matrix_arguments_are_left_for_fixture_loading() {
        assert_eq!(matrix_source(&[]).unwrap(), None);
        assert_eq!(matrix_source(&[OsString::from("--gallery")]).unwrap(), None);
    }

    #[test]
    fn malformed_matrix_arguments_show_usage() {
        for args in [
            vec![OsString::from("--matrix")],
            vec![
                OsString::from("--matrix"),
                OsString::from("one"),
                OsString::from("two"),
            ],
        ] {
            assert!(matrix_source(&args).unwrap_err().contains("usage:"));
        }
    }
}
