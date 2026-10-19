use std::fs;
use std::path::Path;

#[test]
fn direct_dependencies_are_standard_library_and_protocol_only() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("core manifest");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("dependency section");
    let names: Vec<&str> = dependencies
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('=').next().expect("dependency name").trim())
        .collect();
    assert_eq!(names, ["mote-voice-protocol"]);
}

#[test]
fn source_has_no_os_model_daemon_or_network_imports() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let banned = [
        "std::net",
        "std::process",
        "tokio",
        "whisper",
        "coreaudio",
        "audiotoolbox",
        "reqwest",
        "ureq",
        "libc",
    ];
    for entry in fs::read_dir(source).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("Rust source");
        for token in banned {
            assert!(
                !contents.to_lowercase().contains(token),
                "{} contains {token}",
                path.display()
            );
        }
    }
}
