use std::{env, fs, path::PathBuf};

use emanduite_lib::blueprint::blueprint_json_schema;

fn main() {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("schemas/emanduite-project.schema.json"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("failed to create schema output directory");
    }
    let schema = serde_json::to_vec_pretty(&blueprint_json_schema())
        .expect("failed to serialize Blueprint schema");
    fs::write(&path, schema).expect("failed to write Blueprint schema");
    println!("Blueprint schema written to {}", path.display());
}
