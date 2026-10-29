//! Writes the extension schema and reference into `docs/extensions/`, from
//! the repository root wherever it is run:
//! `cargo run -p uze-contract --features schema --example extension-docs`.

use std::path::Path;

use uze_contract::docs;

fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (path, contents) in [
        (docs::SCHEMA_PATH, docs::schema()),
        (docs::REFERENCE_PATH, docs::reference()),
    ] {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, contents)?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
