//! Regenerate golden PNGs for the fixtures: cargo run -p mote-ui --example gen_goldens
use std::path::Path;
fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    files.sort();
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let node = mote_ui::validate(&bytes).expect("fixture valid");
        let png = mote_ui::render::render_png(&node);
        let out = f.with_extension("png");
        std::fs::write(&out, &png).unwrap();
        println!("wrote {}", out.display());
    }
}
