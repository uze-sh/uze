//! Embeds `src/uze.manifest` in every binary of this package on Windows:
//! paths past the Win32 limit, where the system allows them, and UTF-8 as
//! the code page of every byte string an API or a child process is handed.
//!
//! The manifest goes in as a compiled resource the linker takes directly.
//! `/MANIFEST:EMBED` would hand it to `mt.exe`, which only a Windows SDK
//! install has, and the release and the playground cross-link with
//! `rust-lld`.

use std::{env, fs, path::PathBuf};

const MANIFEST: &str = "src/uze.manifest";

fn main() {
    println!("cargo:rerun-if-changed={MANIFEST}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let manifest = fs::read(MANIFEST).expect("the application manifest");
    let resources = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("manifest.res");
    fs::write(&resources, compiled_manifest(&manifest)).expect("the compiled manifest");
    println!("cargo:rustc-link-arg-bins={}", resources.display());
}

/// A `.res` file holding `manifest` as the application's manifest: the
/// empty entry every such file starts with, then `RT_MANIFEST` number 1.
fn compiled_manifest(manifest: &[u8]) -> Vec<u8> {
    const RT_MANIFEST: u16 = 24;
    const CREATEPROCESS_MANIFEST_RESOURCE_ID: u16 = 1;
    const MOVEABLE_PURE: u16 = 0x0030;
    const EN_US: u16 = 0x0409;
    let mut resources = Vec::new();
    resource(&mut resources, 0, 0, 0, 0, &[]);
    resource(
        &mut resources,
        RT_MANIFEST,
        CREATEPROCESS_MANIFEST_RESOURCE_ID,
        MOVEABLE_PURE,
        EN_US,
        manifest,
    );
    resources
}

/// One entry: its 32-byte header (sizes, numeric type and name, flags,
/// language), its data, and padding to the next four-byte boundary.
fn resource(out: &mut Vec<u8>, kind: u16, name: u16, flags: u16, language: u16, data: &[u8]) {
    let size = u32::try_from(data.len()).expect("a manifest under 4 GiB");
    out.extend(size.to_le_bytes());
    out.extend(32u32.to_le_bytes());
    for word in [0xFFFF, kind, 0xFFFF, name] {
        out.extend(word.to_le_bytes());
    }
    out.extend(0u32.to_le_bytes()); // data version
    out.extend(flags.to_le_bytes());
    out.extend(language.to_le_bytes());
    out.extend(0u32.to_le_bytes()); // version
    out.extend(0u32.to_le_bytes()); // characteristics
    out.extend(data);
    out.resize(out.len().next_multiple_of(4), 0);
}
