//! The workspace's content digests, split by what the caller needs.
//!
//! **Identification** ([`fnv1a64`], [`short_hex`]) — a cache directory
//! name, the identity of a managed region. FNV-1a on purpose: these callers
//! ask "is this the same content as before", never "did somebody replace
//! it". It needs no dependency and its output is stable forever, unlike
//! `std`'s `DefaultHasher`, whose algorithm the standard library explicitly
//! does not promise to keep across versions.
//!
//! **Authentication** ([`tree_sha256`], [`sha256`], [`secret_sha256`]) —
//! the `integrity` a lock pins, what an operator's approval of a command
//! line names, and what a record keeps of a secret. Here
//! the question *is* "did somebody replace it", so a digest that is cheap to
//! collide is worse than none: it would state a guarantee it cannot keep.
//! This is the only reason `sha2` is a dependency, and the two must not be
//! confused at a call site — hence one module naming both, rather than a
//! helper wherever each is needed.
//!
//! Stability is the requirement that makes this shared: a digest that
//! changed between releases would silently orphan every artifact previously
//! named by it, and invalidate every `integrity` previously written.

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

/// The digest rendered as fixed-width lowercase hex — the form every caller
/// embeds in a name, so two call sites can never disagree on padding.
pub fn short_hex(bytes: &[u8]) -> String {
    format!("{:016x}", fnv1a64(bytes))
}

/// The digest a secret is recorded as, as `sha256:<hex>`: what a record
/// keeps so that whoever reads the record does not hold the secret too.
pub fn secret_sha256(secret: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(secret.as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hex}")
}

/// The authenticating digest of a directory tree, as `sha256:<hex>`. Paths
/// take part [`portable`](crate::path::portable)-spelled: a digest is a
/// package's identity across machines.
///
/// Every file's path takes part, not only its bytes: moving a skill from
/// `skills/a/SKILL.md` to `skills/b/SKILL.md` changes what the package
/// *does*, so it must change the digest. Paths are sorted, and each field
/// is length-prefixed, so no arrangement of names and contents can be made
/// to produce the same stream as a different one. Directories contribute
/// nothing of their own — an empty one carries no behavior.
///
/// A symlink contributes its name and the path it points at, never the
/// bytes on the other end. It is never entered, exactly as
/// [`crate::engine::discover_files`] treats them: the tree being digested is
/// often a freshly cloned remote checkout, and following a link to an
/// ancestor is an unbounded walk, not a digest. But a link *is* part of
/// what a package does — `assert_self_contained` admits a relative,
/// contained one — so a marketplace adding, removing or repointing one has
/// to move the digest.
pub fn tree_sha256(root: &std::path::Path) -> std::io::Result<String> {
    tree_sha256_with_links(root, &Links::new())
}

/// Where a package holds a link it could not make as one, and what the link
/// points at: the path it would sit at, relative to the package root.
pub type Links = std::collections::BTreeMap<std::path::PathBuf, std::path::PathBuf>;

/// [`tree_sha256`] of a tree in which `links` stand where Git recorded
/// symbolic links a checkout could not make (Windows, where an ordinary
/// account makes none): each digests as the link it is, never as the copy
/// of its target standing in its place, so a package digests to the same
/// value wherever it was acquired.
pub fn tree_sha256_with_links(root: &std::path::Path, links: &Links) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};

    let mut entries = collect_entries(root, links)?;
    entries.sort_by(|left, right| left.path().cmp(right.path()));

    let mut hasher = Sha256::new();
    for entry in &entries {
        let spelled = crate::path::portable(entry.path());
        hasher.update(
            u64::try_from(spelled.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        hasher.update(spelled.as_bytes());
        let body = match entry {
            Entry::File { path } => std::fs::read(root.join(path))?,
            Entry::Link { target, .. } => {
                // A length no file can have — a body of `u64::MAX` bytes
                // does not exist — is what tells a link record from a file
                // record. Framing it this way rather than tagging every
                // record leaves a tree without symlinks digesting to
                // exactly what it always did, so no `integrity` already
                // pinned over such a tree is invalidated by reading links.
                hasher.update(u64::MAX.to_be_bytes());
                crate::path::portable(target).into_bytes()
            }
        };
        hasher.update(u64::try_from(body.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(&body);
    }
    Ok(spelled_sha256(hasher))
}

/// The authenticating digest of `bytes`, as `sha256:<hex>`: for content a
/// caller frames itself, where [`tree_sha256`] frames a directory tree.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    spelled_sha256(hasher)
}

fn spelled_sha256(hasher: sha2::Sha256) -> String {
    use sha2::Digest;
    use std::fmt::Write as _;
    // Spelled byte by byte rather than through the digest's own `LowerHex`:
    // the crate stopped offering one in 0.11, and the written form is what
    // every `integrity` already pinned — it cannot move with a dependency.
    let mut spelled = String::from("sha256:");
    for byte in hasher.finalize() {
        let _ = write!(spelled, "{byte:02x}");
    }
    spelled
}

/// The authenticating digest of some entries of a tree — each a file or a
/// directory named relative to `root` — as `sha256:<hex>`: the code an
/// approval to execute something was given over. A directory takes part as
/// its [`tree_sha256`], a file as its bytes, each beside its name.
pub fn entries_sha256(
    root: &std::path::Path,
    entries: &[std::path::PathBuf],
) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let mut sorted = entries.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut hasher = Sha256::new();
    for entry in &sorted {
        let spelled = crate::path::portable(entry);
        hasher.update(
            u64::try_from(spelled.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        hasher.update(spelled.as_bytes());
        let path = root.join(entry);
        let body = if std::fs::symlink_metadata(&path)?.is_dir() {
            tree_sha256(&path)?.into_bytes()
        } else {
            std::fs::read(&path)?
        };
        hasher.update(u64::try_from(body.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(&body);
    }
    let mut spelled = String::from("sha256:");
    for byte in hasher.finalize() {
        let _ = write!(spelled, "{byte:02x}");
    }
    Ok(spelled)
}

/// One thing in the tree the digest speaks for, named relative to `root`.
enum Entry {
    File {
        path: std::path::PathBuf,
    },
    Link {
        path: std::path::PathBuf,
        target: std::path::PathBuf,
    },
}

impl Entry {
    fn path(&self) -> &std::path::Path {
        match self {
            Entry::File { path } | Entry::Link { path, .. } => path,
        }
    }
}

/// The tree's files and symlinks, relative to `root`.
///
/// A worklist rather than recursion, and `symlink_metadata` rather than
/// `is_dir`: a package may legitimately contain `skills/up -> ..`, which
/// `is_dir` follows and which would otherwise descend until the stack
/// overflows — an abort, before any validation the caller meant to run.
/// A link is read, never followed, for the same reason.
fn collect_entries(root: &std::path::Path, links: &Links) -> std::io::Result<Vec<Entry>> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries: Vec<Entry> = links
        .iter()
        .map(|(path, target)| Entry::Link {
            path: path.clone(),
            target: target.clone(),
        })
        .collect();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let path = entry?.path();
            if path
                .strip_prefix(root)
                .is_ok_and(|relative| links.contains_key(relative))
            {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                if let Ok(relative) = path.strip_prefix(root) {
                    entries.push(Entry::Link {
                        path: relative.to_path_buf(),
                        target: std::fs::read_link(&path)?,
                    });
                }
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
            } else if let Ok(relative) = path.strip_prefix(root) {
                entries.push(Entry::File {
                    path: relative.to_path_buf(),
                });
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A link Git recorded but a checkout could not make, standing in as a
    /// copy of its target, digests as the link it is: what a checkout that
    /// made the link reaches. Unix only: the tree it is compared with holds
    /// the symbolic link itself, which Windows lets an ordinary account make
    /// only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_recorded_link_digests_as_the_link_it_stands_for() {
        let linked = tree("digest-real-link", &[("skills/a/SKILL.md", "body")]);
        std::os::unix::fs::symlink("a", linked.join("skills/b")).unwrap();
        let copied = tree(
            "digest-recorded-link",
            &[("skills/a/SKILL.md", "body"), ("skills/b/SKILL.md", "body")],
        );
        let links = Links::from([(
            std::path::PathBuf::from("skills/b"),
            std::path::PathBuf::from("a"),
        )]);
        assert_eq!(
            tree_sha256_with_links(&copied, &links).unwrap(),
            tree_sha256(&linked).unwrap()
        );
        assert_ne!(tree_sha256(&copied).unwrap(), tree_sha256(&linked).unwrap());
        let _ = std::fs::remove_dir_all(copied);
        let _ = std::fs::remove_dir_all(linked);
    }

    fn tree(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let root = uze_testkit::temp::scratch(label);
        for (relative, contents) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        root
    }

    #[test]
    fn the_same_tree_digests_the_same_whatever_order_it_was_written_in() {
        let one = tree("digest-order-a", &[("a.md", "A"), ("b/c.md", "C")]);
        let two = tree("digest-order-b", &[("b/c.md", "C"), ("a.md", "A")]);
        assert_eq!(tree_sha256(&one).unwrap(), tree_sha256(&two).unwrap());
        assert!(tree_sha256(&one).unwrap().starts_with("sha256:"));
    }

    #[test]
    fn changing_a_byte_changes_the_digest() {
        let before = tree("digest-bytes-a", &[("a.md", "A")]);
        let after = tree("digest-bytes-b", &[("a.md", "B")]);
        assert_ne!(tree_sha256(&before).unwrap(), tree_sha256(&after).unwrap());
    }

    /// Moving a skill changes what a package does, so it must change the
    /// digest even though every byte of content is the same.
    #[test]
    fn moving_a_file_changes_the_digest() {
        let before = tree("digest-move-a", &[("skills/a/SKILL.md", "s")]);
        let after = tree("digest-move-b", &[("skills/b/SKILL.md", "s")]);
        assert_ne!(tree_sha256(&before).unwrap(), tree_sha256(&after).unwrap());
    }

    /// The length prefixes exist for this: without them, a name ending
    /// where the next content begins could be rearranged into the same
    /// byte stream.
    #[test]
    fn adjacent_names_and_contents_cannot_be_rearranged_into_each_other() {
        let one = tree("digest-ambig-a", &[("ab", "cd")]);
        let two = tree("digest-ambig-b", &[("a", "bcd")]);
        assert_ne!(tree_sha256(&one).unwrap(), tree_sha256(&two).unwrap());
    }

    /// A tree that names itself is the shape a remote repository can ship;
    /// following it is an unbounded walk, so the digest must terminate —
    /// reading the link rather than entering it.
    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_directory_symlink_pointing_at_the_tree_itself_does_not_recurse() {
        let root = tree("digest-cycle", &[("a.md", "A")]);
        std::os::unix::fs::symlink(".", root.join("loop")).unwrap();
        std::os::unix::fs::symlink("..", root.join("up")).unwrap();

        assert!(tree_sha256(&root).unwrap().starts_with("sha256:"));
    }

    /// A symlink is part of what a package does — `assert_self_contained`
    /// admits a relative, contained one — so a marketplace must not be able
    /// to add, remove or repoint one behind an unchanged `integrity`.
    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn adding_or_repointing_a_symlink_changes_the_digest() {
        let plain = tree("digest-link-none", &[("a.md", "A")]);

        let added = tree("digest-link-added", &[("a.md", "A")]);
        std::os::unix::fs::symlink("a.md", added.join("b.md")).unwrap();

        let repointed = tree("digest-link-repointed", &[("a.md", "A")]);
        std::os::unix::fs::symlink("skills", repointed.join("b.md")).unwrap();

        assert_ne!(tree_sha256(&plain).unwrap(), tree_sha256(&added).unwrap());
        assert_ne!(
            tree_sha256(&added).unwrap(),
            tree_sha256(&repointed).unwrap()
        );
    }

    /// A link and a file spelled the same, carrying the same string, are
    /// two different packages: one resolves elsewhere at read time and the
    /// other does not.
    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
    #[cfg(unix)]
    #[test]
    fn a_symlink_does_not_digest_as_a_file_holding_its_target() {
        let linked = tree("digest-link-vs-file-a", &[("a.md", "A")]);
        std::os::unix::fs::symlink("a.md", linked.join("b.md")).unwrap();

        let plain = tree("digest-link-vs-file-b", &[("a.md", "A"), ("b.md", "a.md")]);

        assert_ne!(tree_sha256(&linked).unwrap(), tree_sha256(&plain).unwrap());
    }

    /// The framing that distinguishes a link record is a content length no
    /// file can have, so a tree without symlinks digests to exactly the
    /// stream it always did — every `integrity` pinned over one stays valid.
    #[test]
    fn a_tree_without_symlinks_digests_to_its_recorded_value() {
        let root = tree("digest-stable", &[("a.md", "A"), ("b/c.md", "C")]);
        assert_eq!(
            tree_sha256(&root).unwrap(),
            "sha256:3825ac85ae87734ec73c95f6e914752018bbeed1362f5413bc4962727e980711"
        );
    }

    #[test]
    fn the_digest_is_stable_and_fixed_width() {
        assert_eq!(short_hex(b"").len(), 16);
        assert_eq!(short_hex(b"uze"), short_hex(b"uze"));
        assert_ne!(short_hex(b"uze"), short_hex(b"uze "));
    }
}
