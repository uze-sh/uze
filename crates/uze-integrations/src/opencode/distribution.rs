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
    // A fresh directory only this user can enter, never one found in the
    // shared temp directory: what is downloaded and unpacked there becomes
    // an executable on the person's PATH.
    let scratch = uze_core::acquisition::scratch_directory().map_err(|error| error.to_string())?;
    let installed = unpack(runner, target, &scratch).and_then(|built| place(&built));
    let _ = std::fs::remove_dir_all(&scratch);
    installed
}

/// Downloads, verifies and unpacks OpenCode's `target` build in `scratch`,
/// answering where its binary is.
fn unpack(runner: &dyn ProcessRunner, target: &str, scratch: &Path) -> Result<PathBuf, String> {
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
    let downloaded = scratch.join("download.tgz");
    fetch(runner, &tarball, &downloaded)?;
    let bytes = std::fs::read(&downloaded).map_err(|error| error.to_string())?;
    if !integrity_matches(&bytes, &integrity) {
        return Err(format!(
            "{package} {version} does not match its published integrity"
        ));
    }
    // `tar` reads the bytes that were verified, written to a file only this
    // call created, never the download a second time.
    let archive = scratch.join("verified.tgz");
    write_new(&archive, &bytes).map_err(|error| error.to_string())?;
    let unpacked = scratch.join("unpacked");
    uze_platform::fs::create_private_dir(&unpacked).map_err(|error| error.to_string())?;
    let extract = ProcessSpec::new(
        uze_platform::tools::system_program("tar").to_string_lossy(),
        [
            "-xzf",
            &archive.to_string_lossy(),
            "-C",
            &unpacked.to_string_lossy(),
        ],
    );
    run(runner, "tar", extract)?;
    let built = unpacked
        .join("package")
        .join("bin")
        .join(uze_platform::executable::file_name("opencode"));
    match std::fs::symlink_metadata(&built) {
        Ok(metadata) if metadata.is_file() => Ok(built),
        _ => Err(format!(
            "{package} {version} carries no regular file at package/bin"
        )),
    }
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    uze_platform::fs::private_file(std::fs::OpenOptions::new().write(true).create_new(true))
        .open(path)?
        .write_all(bytes)
}

/// Places the unpacked binary where OpenCode's own installer does, and puts
/// that directory on the user's PATH.
fn place(built: &Path) -> Result<PathBuf, String> {
    let binary_name = uze_platform::executable::file_name("opencode");
    let directory = install_dir().ok_or("no home directory to install into")?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join(&binary_name);
    // Beside its destination, so placing it is a rename on one volume.
    let staged = directory.join(format!(".{binary_name}.new"));
    let placed = std::fs::copy(built, &staged)
        .and_then(|_| uze_platform::executable::make_runnable(&staged))
        .and_then(|()| uze_platform::executable::replace_running(&staged, &destination));
    if let Err(error) = placed {
        // Beside the person's own binaries: nothing of a failed attempt
        // stays there.
        let _ = std::fs::remove_file(&staged);
        return Err(error.to_string());
    }
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
    let spec = download(url, to);
    run(runner, "curl", spec).map_err(|_| format!("could not download {url}"))
}

/// The same `curl` [`uze_platform::tools::curl`] runs: no `.curlrc`, HTTPS
/// only, and the environment's proxy and CA kept for the reason given
/// there.
fn download(url: &str, to: &Path) -> ProcessSpec {
    let arguments = uze_platform::tools::CURL_ARGUMENTS
        .into_iter()
        .map(str::to_owned)
        .chain(["-fsSL".to_owned(), url.to_owned(), "-o".to_owned()])
        .chain([to.to_string_lossy().into_owned()]);
    ProcessSpec::new(
        uze_platform::tools::system_program("curl").to_string_lossy(),
        arguments,
    )
    .without_env(uze_platform::tools::curl_environment_to_remove())
}

fn run(runner: &dyn ProcessRunner, tool: &str, spec: ProcessSpec) -> Result<(), String> {
    let outcome = runner.run(&spec).map_err(|error| error.to_string())?;
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

    /// Answers each download from `served` by URL, and unpacks by writing
    /// the binary where `tar` was told to, after recording the archive it
    /// was handed and what it held at that moment.
    struct Registry {
        served: Vec<(String, Vec<u8>)>,
        downloads: std::sync::Mutex<Vec<PathBuf>>,
        unpacked: std::sync::Mutex<Vec<(PathBuf, Vec<u8>)>>,
    }

    impl ProcessRunner for Registry {
        fn run(
            &self,
            spec: &ProcessSpec,
        ) -> uze_core::Result<uze_core::provisioning::ProcessResult> {
            let argument = |index: usize| PathBuf::from(&spec.arguments[index]);
            let url_at = uze_platform::tools::CURL_ARGUMENTS.len() + 1;
            if spec
                .program
                .ends_with(&*uze_platform::executable::file_name("curl"))
            {
                let (_, body) = self
                    .served
                    .iter()
                    .find(|(url, _)| *url == spec.arguments[url_at])
                    .expect("a URL the registry serves");
                std::fs::write(argument(url_at + 2), body).unwrap();
                self.downloads.lock().unwrap().push(argument(url_at + 2));
            } else {
                // Another writer racing the unpack: the download changes
                // under its name before `tar` reads.
                for download in self.downloads.lock().unwrap().iter() {
                    std::fs::write(download, b"tampered").unwrap();
                }
                let archive = argument(1);
                let held = std::fs::read(&archive).unwrap();
                self.unpacked.lock().unwrap().push((archive, held));
                let bin = argument(3).join("package/bin");
                std::fs::create_dir_all(&bin).unwrap();
                std::fs::write(
                    bin.join(uze_platform::executable::file_name("opencode")),
                    b"exe",
                )
                .unwrap();
            }
            Ok(uze_core::provisioning::ProcessResult {
                success: true,
                timed_out: false,
            })
        }
    }

    /// The archive `tar` unpacks holds the bytes whose integrity was
    /// checked, whatever happens to the download afterwards.
    #[test]
    fn what_is_unpacked_is_what_was_verified() {
        let tarball = b"the published tarball".to_vec();
        let integrity = format!("sha512-{}", base64(&Sha512::digest(&tarball)));
        let registry = Registry {
            served: vec![
                (
                    LATEST.to_owned(),
                    br#"{"version":"2.0.22","metadata":{"package":"@opencode/cli"}}"#.to_vec(),
                ),
                (
                    format!("{REGISTRY}/@opencode%2fcli-linux-x64/2.0.22"),
                    serde_json::json!({"dist": {"tarball": "https://t/x.tgz", "integrity": integrity}})
                        .to_string()
                        .into_bytes(),
                ),
                ("https://t/x.tgz".to_owned(), tarball.clone()),
            ],
            downloads: Default::default(),
            unpacked: Default::default(),
        };
        let scratch = uze_core::acquisition::scratch_directory().unwrap();

        let built = unpack(&registry, "linux-x64", &scratch).expect("unpacked");

        let unpacked = registry.unpacked.lock().unwrap();
        assert_eq!(unpacked.len(), 1);
        assert_eq!(
            unpacked[0].1,
            tarball,
            "tar read {}",
            unpacked[0].0.display()
        );
        assert!(built.starts_with(&scratch), "{}", built.display());
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_download_is_the_system_curl_reading_no_configuration() {
        let spec = download("https://registry.test/x.tgz", Path::new("x.tgz"));
        assert_eq!(
            Path::new(&spec.program),
            uze_platform::tools::system_program("curl")
        );
        assert!(Path::new(&spec.program).is_absolute());
        assert_eq!(
            spec.arguments[..uze_platform::tools::CURL_ARGUMENTS.len()],
            uze_platform::tools::CURL_ARGUMENTS
        );
        assert_eq!(
            spec.removed_environment,
            uze_platform::tools::curl_environment_to_remove()
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
