//! The Seatbelt profile a confinement becomes on macOS, written here as
//! text `sandbox-exec -p` reads. Pure, so it is built and tested on every
//! platform.
//!
//! Metadata (`stat`) is readable everywhere, as it is under Landlock;
//! contents only beneath the granted paths and the few system paths the
//! dynamic loader needs.

use std::{io, path::Path};

use super::Confinement;

/// Read by the dynamic loader and the C library of every program.
const SYSTEM_READ: &[&str] = &[
    "/usr/lib",
    "/usr/share/icu",
    "/System",
    "/Library/Apple",
    "/private/var/db/dyld",
    "/private/var/db/timezone",
    "/dev/null",
    "/dev/zero",
    "/dev/urandom",
    "/dev/random",
];

/// The services the C library asks for user and group names, and logging.
const MACH_SERVICES: &[&str] = &[
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.logger",
    "com.apple.logd",
];

/// What name resolution and TLS ask for, with network only.
const NETWORK_MACH_SERVICES: &[&str] = &[
    "com.apple.dnssd.service",
    "com.apple.mDNSResponder",
    "com.apple.SystemConfiguration.configd",
    "com.apple.trustd",
    "com.apple.trustd.agent",
];

pub(super) fn profile(confinement: &Confinement) -> io::Result<String> {
    let mut profile = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process-fork)\n\
         (allow signal (target same-sandbox))\n\
         (allow process-info* (target self))\n\
         (allow sysctl-read)\n\
         (allow file-read-metadata)\n",
    );
    let services = MACH_SERVICES.iter().chain(if confinement.network {
        NETWORK_MACH_SERVICES
    } else {
        &[]
    });
    for service in services {
        profile.push_str(&format!(
            "(allow mach-lookup (global-name {}))\n",
            quoted(service)
        ));
    }
    for path in SYSTEM_READ {
        profile.push_str(&rule("file-read*", Path::new(path))?);
    }
    for path in &confinement.read {
        profile.push_str(&rule("file-read*", path)?);
    }
    for path in &confinement.read_execute {
        profile.push_str(&rule("file-read* process-exec", path)?);
    }
    for path in &confinement.write {
        profile.push_str(&rule("file-read* file-write*", path)?);
    }
    for path in &confinement.devices {
        profile.push_str(&rule("file-read* file-write-data file-ioctl", path)?);
    }
    if confinement.network {
        profile.push_str(
            "(allow network-outbound (remote tcp))\n\
             (allow network-outbound (remote udp \"*:53\"))\n",
        );
    } else {
        profile.push_str("(deny network*)\n");
    }
    Ok(profile)
}

fn rule(operations: &str, path: &Path) -> io::Result<String> {
    let Some(text) = path.to_str() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot grant {}: not UTF-8", path.display()),
        ));
    };
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cannot grant {text}: not an absolute path"),
        ));
    }
    Ok(format!("(allow {operations} (subpath {}))\n", quoted(text)))
}

/// A string literal of the profile language: a quote or a backslash in a
/// path is escaped, so no path can end the literal and add a rule.
fn quoted(text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for character in text.chars() {
        match character {
            '"' | '\\' => {
                literal.push('\\');
                literal.push(character);
            }
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::confine::Limits;

    fn confinement(network: bool) -> Confinement {
        Confinement {
            read: vec!["/repo".into()],
            read_execute: vec!["/usr/bin".into()],
            write: vec!["/tmp/run".into(), "/dev/null".into()],
            devices: Vec::new(),
            network,
            limits: Limits::with_cpu_seconds(10),
        }
    }

    #[test]
    fn everything_is_denied_but_what_was_granted() {
        let profile = profile(&confinement(false)).unwrap();
        assert!(profile.starts_with("(version 1)\n(deny default)\n"));
        assert!(profile.contains("(allow file-read* (subpath \"/repo\"))"));
        assert!(profile.contains("(allow file-read* process-exec (subpath \"/usr/bin\"))"));
        assert!(profile.contains("(allow file-read* file-write* (subpath \"/tmp/run\"))"));
        assert!(profile.contains("(allow signal (target same-sandbox))"));
        assert!(profile.contains("(deny network*)"));
        assert!(!profile.contains("network-outbound"));
        assert!(!profile.contains("system-socket"));
        assert!(!profile.contains("mDNSResponder"));
    }

    #[test]
    fn network_allows_tcp_out_and_dns_and_never_a_unix_socket() {
        let profile = profile(&confinement(true)).unwrap();
        assert!(profile.contains("(allow network-outbound (remote tcp))"));
        assert!(profile.contains("(remote udp \"*:53\")"));
        assert!(profile.contains("com.apple.mDNSResponder"));
        assert!(!profile.contains("(deny network*)"));
        assert!(!profile.contains("unix"));
    }

    #[test]
    fn a_path_cannot_break_out_of_its_literal() {
        let mut hostile = confinement(false);
        hostile.read = vec!["/x\")) (allow default) ((\\".into()];
        let profile = profile(&hostile).unwrap();
        assert!(profile.contains("(subpath \"/x\\\")) (allow default) ((\\\\\"))"));
        assert_eq!(
            profile
                .lines()
                .filter(|line| line.contains("allow default"))
                .count(),
            1,
            "only inside the quoted literal"
        );
        assert_eq!(quoted("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn a_relative_path_is_refused() {
        let mut relative = confinement(false);
        relative.write = vec!["scratch".into()];
        assert!(profile(&relative).is_err());
    }
}
