//! Regenerate schemas/tree.v0.json from the Rust types: cargo run -p mote-ui --example gen_schema
fn main() {
    println!("{}", mote_ui::tree_schema_pretty());
}
