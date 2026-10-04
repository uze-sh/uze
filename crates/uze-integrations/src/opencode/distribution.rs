//! OpenCode's own build, distributed by UZE where OpenCode publishes no
//! installer for the platform (Windows): what its official installer
//! (`opencode.ai/install`) does elsewhere, step for step. The update service
//! names the version, the npm package built for this platform is downloaded
//! and its integrity checked, and the binary is placed where the installer
//! places it, `~/.opencode/bin`, which then goes on the user's `PATH`.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha512};
use uze_core::provisioning::{ProcessRunner, ProcessSpec};

const LATEST: &str = "https://opencode.ai/update/api/latest/cli/npm";
const REGISTRY: &str = "https://registry.npmjs.org";

/// The platform suffix of the package OpenCode publishes for this machine,
/// as its installer names it: `windows-x64`, with `-baseline` for an x86-64
/// processor without AVX2.
pub(super) fn target() -> Option<String> {
    target_for(
        uze_platform::target::OS,
        uze_platform::target::ARCH,
        uze_platform::cpu::lacks_avx2(),
    )
}

fn target_for(os: &str, arch: &str, lacks_avx2: bool) -> Option<String> {
    let os = match os {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "darwin",
        _ => return None,
    };
    let arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    let baseline = if arch == "x64" && lacks_avx2 {
        "-baseline"
    } else {
        ""
    };
    Some(format!("{os}-{arch}{baseline}"))
}

/// Where the binary goes: OpenCode's own installer's directory.
pub(super) fn install_dir() -> Option<PathBuf> {
    Some(uze_core::user_home()?.join(".opencode").join("bin"))
}

/// Installs OpenCode's `target` build and answers where it now is. Every
/// download goes through `runner`, as an installer's would.
pub(super) fn install(runner: &dyn ProcessRunner, target: &str) -> Result<PathBuf, String> {
    let scratch = std::env::temp_dir().join(format!("uze-opencode-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).map_err(|error| error.to_string())?;
    let installed = install_through(runner, target, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    installed
}

fn install_through(
    runner: &dyn ProcessRunner,
    target: &str,
    scratch: &Path,
) -> Result<PathBuf, String> {
    let latest: serde_json::Value = fetch_json(runner, LATEST, &scratch.join("latest.json"))?;
    let (version, scope) = latest_release(&latest).ok_or("the update service named no version")?;
    let package = format!("{scope}/cli-{target}");
    let metadata: serde_json::Value = fetch_json(
        runner,
        &format!("{REGISTRY}/{}/{version}", package.replace('/', "%2f")),
        &scratch.join("package.json"),
    )?;
    let (tarball, integrity) = published_tarball(&metadata)
        .ok_or_else(|| format!("{package} {version} names no tarball"))?;
    let archive = scratch.join("package.tgz");
    fetch(runner, &tarball, &archive)?;
    let bytes = std::fs::read(&archive).map_err(|error| error.to_string())?;
    if !integrity_matches(&bytes, &integrity) {
        return Err(format!(
            "{package} {version} does not match its published integrity"
        ));
    }
    run(
        runner,
        "tar",
        [
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &scratch.to_string_lossy(),
        ],
    )?;
    let binary_name = uze_platform::executable::file_name("opencode");
    let built = scratch.join("package").join("bin").join(&binary_name);
    let directory = install_dir().ok_or("no home directory to install into")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join(&binary_name);
    // Beside its destination, so placing it is a rename on one volume.
    let staged = directory.join(format!(".{binary_name}.new"));
    std::fs::copy(&built, &staged).map_err(|error| error.to_string())?;
    uze_platform::executable::make_runnable(&staged).map_err(|error| error.to_string())?;
    uze_platform::executable::replace_running(&staged, &destination)
        .map_err(|error| error.to_string())?;
    if let Err(error) = uze_platform::environment::add_to_user_path(&directory) {
        tracing::warn!(%error, "OpenCode's directory was not added to PATH");
    }
    Ok(destination)
}

/// The version the update service names, and the npm scope it is published
/// under (`@opencode` from `@opencode/cli`).
fn latest_release(latest: &serde_json::Value) -> Option<(String, String)> {
    let version = latest.get("version")?.as_str()?.to_owned();
    let package = latest
        .pointer("/metadata/package")
        .or_else(|| latest.get("package"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("@opencode/cli");
    let scope = package.strip_suffix("/cli").unwrap_or(package).to_owned();
    Some((version, scope))
}

/// The tarball an npm version publishes, and its `sha512-…` integrity.
fn published_tarball(metadata: &serde_json::Value) -> Option<(String, String)> {
    let dist = metadata.get("dist")?;
    Some((
        dist.get("tarball")?.as_str()?.to_owned(),
        dist.get("integrity")?.as_str()?.to_owned(),
    ))
}

/// Whether `bytes` are what an npm `sha512-<base64>` integrity names.
fn integrity_matches(bytes: &[u8], integrity: &str) -> bool {
    integrity
        .strip_prefix("sha512-")
        .is_some_and(|expected| base64(&Sha512::digest(bytes)) == expected)
}

/// Standard, padded base64: the encoding npm writes an integrity in.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let triple = chunk.iter().enumerate().fold(0u32, |acc, (index, byte)| {
            acc | u32::from(*byte) << (16 - 8 * index)
        });
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(char::from(
                    ALPHABET[(triple >> (18 - 6 * position)) as usize & 63],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn fetch_json(
    runner: &dyn ProcessRunner,
    url: &str,
    to: &Path,
) -> Result<serde_json::Value, String> {
    fetch(runner, url, to)?;
    let text = std::fs::read_to_string(to).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| format!("{url} answered no JSON: {error}"))
}

fn fetch(runner: &dyn ProcessRunner, url: &str, to: &Path) -> Result<(), String> {
    run(runner, "curl", ["-fsSL", url, "-o", &to.to_string_lossy()])
        .map_err(|_| format!("could not download {url}"))
}

fn run<const N: usize>(
    runner: &dyn ProcessRunner,
    tool: &str,
    arguments: [&str; N],
) -> Result<(), String> {
    let program = uze_platform::tools::system_program(tool);
    let outcome = runner
        .run(&ProcessSpec::new(program.to_string_lossy(), arguments))
        .map_err(|error| error.to_string())?;
    if outcome.success {
        Ok(())
    } else {
        Err(format!("`{tool}` did not finish"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_is_named_as_opencode_s_installer_names_it() {
        assert_eq!(
            target_for("windows", "x86_64", false).as_deref(),
            Some("windows-x64")
        );
        assert_eq!(
            target_for("windows", "x86_64", true).as_deref(),
            Some("windows-x64-baseline")
        );
        assert_eq!(
            target_for("windows", "aarch64", true).as_deref(),
            Some("windows-arm64")
        );
        assert_eq!(
            target_for("macos", "aarch64", false).as_deref(),
            Some("darwin-arm64")
        );
        assert_eq!(target_for("freebsd", "x86_64", false), None);
    }

    #[test]
    fn the_update_service_names_the_version_and_its_scope() {
        let latest = serde_json::json!({
            "version": "2.0.22",
            "metadata": { "package": "@opencode/cli" }
        });
        assert_eq!(
            latest_release(&latest),
            Some(("2.0.22".to_owned(), "@opencode".to_owned()))
        );
    }

    /// The integrity npm publishes is the base64 SHA-512 of the tarball.
    #[test]
    fn an_integrity_is_checked_against_the_bytes() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        let integrity = format!("sha512-{}", base64(&Sha512::digest(b"package")));
        assert!(integrity_matches(b"package", &integrity));
        assert!(!integrity_matches(b"tampered", &integrity));
        assert!(!integrity_matches(b"package", "sha1-abc"));
    }
}
