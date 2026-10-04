//! The file systems this machine reaches over a network protocol: `9p`,
//! which is how a Windows drive appears inside WSL. A walk of `PATH` that
//! probes every entry pays a round trip on each of them.

use std::{fs, path::PathBuf};

/// Mount points of the filesystems reached over a network protocol, read
/// once per process from the kernel's mount table. Empty where there is
/// no such table.
pub fn network_mount_points() -> &'static [PathBuf] {
    static MOUNTS: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    MOUNTS.get_or_init(|| {
        fs::read_to_string("/proc/self/mounts")
            .map(|table| network_mount_points_in(&table))
            .unwrap_or_default()
    })
}

/// Parses a `/proc/self/mounts` table (`source mountpoint fstype …`, one
/// mount per line, spaces in a path escaped as octal `\040`).
fn network_mount_points_in(table: &str) -> Vec<PathBuf> {
    const NETWORK_FILESYSTEMS: &[&str] = &["9p"];
    table
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let _source = fields.next()?;
            let mount_point = fields.next()?;
            let filesystem = fields.next()?;
            NETWORK_FILESYSTEMS
                .contains(&filesystem)
                .then(|| PathBuf::from(unescape_mount_field(mount_point)))
        })
        .collect()
}

fn unescape_mount_field(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut characters = field.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        let digits: String = characters.by_ref().take(3).collect();
        match u8::from_str_radix(&digits, 8) {
            Ok(byte) => out.push(byte as char),
            Err(_) => {
                out.push('\\');
                out.push_str(&digits);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_drive_mounted_into_wsl_is_a_network_mount() {
        let table = "/dev/sdd / ext4 rw,relatime 0 0\n\
                     C:\\134 /mnt/c 9p rw,noatime,aname=drvfs;path=C:\\ 0 0\n\
                     D:\\134 /mnt/my\\040drive 9p rw 0 0\n\
                     tmpfs /run tmpfs rw 0 0\n";
        assert_eq!(
            network_mount_points_in(table),
            vec![PathBuf::from("/mnt/c"), PathBuf::from("/mnt/my drive")]
        );
    }
}
