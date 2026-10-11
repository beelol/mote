//! Regenerate schemas/tree.matrix.v1.json from the Rust contract:
//! cargo run -p mote-ui --example gen_matrix_v1_schema

fn main() {
    println!("{}", mote_ui::v1::tree_schema_pretty());
}
