//! Built-in TUI extensions for the uze workspace client — add-ons that
//! live below `src/ui/` in the dependency graph (`src/` uses this crate,
//! never the reverse), the same relationship `uze-integrations` has to the
//! harness registry: one crate, one module per extension, one registry
//! entry point ([`registry::ExtensionRegistry`]) naming the set.
//!
//! Three ship today, one per question a reader brings to a checkout.
//! [`spec`] answers what it *intends*: the proposals, designs, tasks and
//! specs a spec-driven-development tool keeps, read as documents.
//! [`architect`] answers what somebody *described*, drawing architecture
//! diagrams in cells. [`code`] answers what it *is*, four ways: what
//! changed in it, what it contains, its commit timeline, and a map of
//! where its lines are. Another is a module with its own `CATALOG` entry, one
//! registration in `ExtensionRegistry::builtin`, and its surfaces tagged
//! with the slot the host stands them in ([`ExtensionHit`]), not a new
//! crate. An extension a plugin declares is none of these: [`declared`]
//! compiles its manifest into the same contract, and its logic answers
//! through [`Host::answer`].
//!
//! # Where a file goes
//!
//! One directory per extension, named after it, with the extension's own
//! surface — its state, its keys, its registry entry — in the file beside
//! it. What more than one extension needs lives under `shared`, created
//! the day a second extension actually reached for it and not before —
//! `shared.rs` says what is there and why. [`view`] is
//! neither: it is the contract between an extension and whatever draws
//! it, which is why it sits at the root alongside [`Host`], the contract
//! in the other direction.
//!
//! # An extension holds no machine access of its own
//!
//! Everything outside the extension's own process memory — running Git,
//! reading a file, knowing where `$HOME` is — arrives through [`Host`].
//! The extension names what it needs; the host decides whether to oblige.
//! A built-in is always obliged, in this process; a declared extension's
//! question is answered only within what the operator consented to.
//!
//! What changes is that an extension is now a pure function of what it is
//! handed. That is what makes the trust question answerable later: code
//! authored elsewhere cannot reach anything it was not given, and the day
//! an extension runs somewhere else, nothing inside it has to change.
//!
//! # An extension describes; the host draws
//!
//! An extension answers with a [`view::View`] — what it has, never how it
//! looks. The host owns rendering, geometry, and the palette, which is why
//! the copy of `src/ui.rs`'s colour table that used to live here is gone
//! along with the two-sided "keep these in sync by eye" it required. See
//! [`view`] for the rest of the reasoning.

#![forbid(unsafe_code)]

pub mod architect;
pub mod code;
pub mod contract;
pub mod declared;
pub mod registry;
pub mod review;
pub mod shared;
pub mod spec;
pub mod view;

pub use shared::places::{ArtifactRoot, ArtifactSource};
pub use uze_contract::{Answer, Asking, Envelope, Question};

/// Something a viewer did inside an extension's own surface, addressed to
/// the slot the host stands that surface in.
///
/// The payload is [`view::ViewHit`] for every extension, because the host
/// produces it from what it drew and none of that is extension-specific.
/// The slot is what routes: with two surfaces open in one session, "row 3
/// was clicked" is meaningless without saying whose row 3. Which extension
/// and which of its surfaces a slot is, built-in or declared, is the
/// host's map to keep, since two plugins may declare the same ids.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionHit {
    pub slot: u32,
    pub hit: view::ViewHit,
}

/// Why [`Host::read_file`] has no contents to give.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Unreadable {
    /// The file is there and it is not text: an image, an archive, a
    /// font. Nothing went wrong, so a view says there is nothing it can
    /// draw rather than reporting a failure.
    NotText,
    /// Anything that did go wrong, as the sentence to show.
    Failed(String),
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotText => f.write_str("not readable as text"),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

/// One entry of a directory listing, as [`Host::list_dir`] answers it.
///
/// A name and a kind, never a handle: the extension addresses a child by
/// joining the name onto the directory it asked about, so nothing here
/// carries the ability to reach anywhere the host did not already say.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirEntry {
    /// Whether it holds other entries.
    pub directory: bool,
    pub name: String,
}

/// The listing order the contract promises, carried by the type rather
/// than by whoever sorts it: directories first, then each half by name.
///
/// Derived, it would have ordered files first — `false` sorts before
/// `true` — which is the opposite of every file tree a person has used,
/// and a mistake nothing but a test would have caught.
impl Ord for DirEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .directory
            .cmp(&self.directory)
            .then_with(|| self.name.cmp(&other.name))
    }
}

impl PartialOrd for DirEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// What an extension may ask the host to do on its behalf.
///
/// Deliberately tiny: every method here is something an extension that
/// ships today actually needs, and a wider surface would be speculation
/// about one that does not exist yet.
///
/// Five of them write ([`Host::write_file`], [`Host::delete_file`],
/// [`Host::delete_dir`], [`Host::rename_path`] and
/// [`Host::restore_to_head`]), reached through the directory listing,
/// which is a
/// widening of what this trait once granted and the reason it is worth
/// stating plainly: an extension that edits a file needs to be *given*
/// that, and a grant nobody can name is a grant nobody can withhold.
/// Every method that can refuse says why it refused, because the reason
/// is what the surface draws or tells the person who asked — a file too
/// large to open and a file that is not text are both "nothing to show",
/// and only one of them is worth trying something else about.
pub trait Host {
    /// Runs a read-only Git command in `root`, returning its stdout.
    ///
    /// Exit `0` is always an answer. `answers` names the other exit codes
    /// that are one for this command rather than a failure — only the
    /// caller knows which: `git diff --no-index` exits `1` for "there are
    /// differences", `rev-parse --verify --quiet` for "no such ref".
    fn git(&self, root: &std::path::Path, args: &[&str], answers: &[i32])
    -> Result<String, String>;

    /// The working tree `path` sits in. Doubles as the "is this inside a
    /// Git repository" check: outside one, Git's own message is the error.
    ///
    /// Named rather than spelled as a `git` call because its answer cannot
    /// change while the path is still there, so a host may remember it —
    /// and the change badge asks it on every refresh.
    fn repository_root(&self, path: &std::path::Path) -> Result<std::path::PathBuf, String>;

    /// A file's contents, or why they cannot be shown. Unreadable is a
    /// state a view renders, never an error it propagates — but the
    /// reasons are not interchangeable to the person looking at the row,
    /// and only the host knows which one applies: a binary, a file it may
    /// not read, and one too large to hold in memory all land here.
    fn read_file(&self, path: &std::path::Path) -> Result<String, Unreadable>;

    /// A file's raw contents, for a format the extension decodes itself —
    /// an image. Bounded the way [`Host::read_file`] is. A host that hands
    /// out no bytes answers [`Unreadable::NotText`], which leaves the file
    /// exactly as undrawable as it was before this was asked.
    fn read_bytes(&self, path: &std::path::Path) -> Result<Vec<u8>, Unreadable> {
        let _ = path;
        Err(Unreadable::NotText)
    }

    /// How many lines a file has, counted the way [`str::lines`] counts
    /// them. Separate from [`Host::read_file`] because the badge asks this
    /// of every untracked file on a timer, and holding each one in memory
    /// to count its newlines costs the size of the file for a number the
    /// caller could have streamed. The default answers from the contents;
    /// a host that can read without materialising them should say so.
    fn count_lines(&self, path: &std::path::Path) -> u32 {
        self.read_file(path)
            .map(|contents| u32::try_from(contents.lines().count()).unwrap_or(u32::MAX))
            .unwrap_or(0)
    }

    /// The entries of `path`, directories first and each half sorted by
    /// name — the order every listing arrives in, so the same directory
    /// never reads differently twice.
    fn list_dir(&self, path: &std::path::Path) -> Result<Vec<DirEntry>, String>;

    /// Whether `root`, a checkout, has something at `relative`: what an
    /// `applies-when` asks. One bit, from the listing of its parent, so
    /// nothing of the file is read; a path that climbs out has nothing.
    fn checkout_has(&self, root: &std::path::Path, relative: &std::path::Path) -> bool {
        let inside = relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
        let (Some(name), true) = (relative.file_name(), inside) else {
            return false;
        };
        let parent = relative
            .parent()
            .map_or(root.to_path_buf(), |parent| root.join(parent));
        self.list_dir(&parent).is_ok_and(|entries| {
            entries
                .iter()
                .any(|entry| std::ffi::OsStr::new(&entry.name) == name)
        })
    }

    /// Replaces `path`'s contents. The file must already exist: this
    /// extension edits what a project has, and creating a path from a
    /// typed string is a separate grant nothing asks for yet.
    ///
    /// `root` is the directory the surface is showing, and every write
    /// names it: a host refuses a path that, once the links a checkout
    /// carries are followed, lands outside it.
    fn write_file(
        &self,
        root: &std::path::Path,
        path: &std::path::Path,
        contents: &str,
    ) -> Result<(), String>;

    /// Removes `path`. Files only — a directory removal is recursive by
    /// nature, and is [`Host::delete_dir`], a grant of its own.
    fn delete_file(&self, root: &std::path::Path, path: &std::path::Path) -> Result<(), String>;

    /// Removes the directory `path` and everything in it. Its own grant
    /// rather than a flag on [`Host::delete_file`], because "delete this"
    /// meaning "delete these four hundred" is a different thing to hand
    /// over. Refused by default: a host grants it by saying so.
    fn delete_dir(&self, root: &std::path::Path, path: &std::path::Path) -> Result<(), String> {
        let _ = root;
        Err(format!("{} cannot be deleted here", path.display()))
    }

    /// Gives `from` the name `to`, which must sit in the same directory
    /// and must not exist: this renames, it neither moves nor overwrites.
    /// Refused by default, for the reason [`Host::delete_dir`] is.
    fn rename_path(
        &self,
        root: &std::path::Path,
        from: &std::path::Path,
        to: &std::path::Path,
    ) -> Result<(), String> {
        let _ = (root, to);
        Err(format!("{} cannot be renamed here", from.display()))
    }

    /// Puts each of `paths` back the way `root`'s last commit has it, in
    /// the index and on disk alike: restored where the commit has the
    /// path, removed where it does not. The one write into a repository
    /// an extension is granted, and a narrow one on purpose — throwing a
    /// change away is a reviewer's gesture, while staging and committing
    /// shape what the agent working in the checkout commits next.
    fn restore_to_head(
        &self,
        root: &std::path::Path,
        paths: &[std::path::PathBuf],
    ) -> Result<(), String>;

    /// The palette to render syntax-highlighted content with.
    ///
    /// Highlighting is the one thing an extension draws that carries its own
    /// colours rather than the host's roles (see [`view::Rgb`]) — so it is
    /// also the one thing that can end up unreadable against a background it
    /// knows nothing about. Naming the palette is the host's call for the
    /// same reason every other colour is, and it arrives here rather than as
    /// a constant so a light theme does not leave a diff illegible.
    fn syntax_theme(&self) -> String;

    /// Where the project around `project` declares its artifacts: the
    /// directories its `workspace.artifacts` names, why they were refused,
    /// or that it names none. A host that reads no declaration finds none.
    fn artifact_source(&self, _project: &std::path::Path) -> ArtifactSource {
        ArtifactSource::Undeclared
    }

    /// The branch `checkout` delivers to, as the workspace's delivery
    /// policy names it. A host that keeps no policy names none.
    fn delivery_target(&self, _checkout: &std::path::Path) -> Option<String> {
        None
    }

    /// Answers a declared extension's question: one of its declarations,
    /// asked with its input about a checkout. Never a program or a script's
    /// text: the host finds what answers it in the compile the operator
    /// consented to, and says why when nothing may. Only the declared
    /// engine asks it. A host that runs no declared extension answers
    /// none.
    fn answer(&self, _question: Question) -> Answer {
        Answer::withheld("this host answers no declared extension")
    }

    /// The bytes of a picture a declared extension's view names: `name`
    /// inside the extension's directory, from the compile the operator
    /// consented to, or `scratch:<file>`, a file its program wrote into
    /// its run's scratch and the host kept. Read no further than
    /// [`declared::IMAGE_FILE_MAX`] and one byte, so a file past it is
    /// refused without being held. Only the declared engine asks it. A
    /// host that runs no declared extension shows none.
    fn media(&self, _identity: &uze_contract::Identity, _name: &str) -> Result<Vec<u8>, String> {
        Err("this host shows no extension's pictures".to_owned())
    }

    /// The review kept for the agent `in_front` in `checkout`, or with
    /// none in front for the one agent working there. Only the review
    /// extension asks it. A host that keeps no reviews keeps none.
    fn review_record(
        &self,
        _checkout: &std::path::Path,
        _in_front: Option<&str>,
    ) -> Result<review::ReviewRecord, String> {
        Err("this host keeps no reviews".to_owned())
    }

    /// Reads that review, lets `change` edit it and writes it back, as one
    /// operation under the record's lock: the agent answers into the same
    /// record from its own process. Answers what was written. Only the
    /// review extension asks it.
    fn update_review_record(
        &self,
        _checkout: &std::path::Path,
        _in_front: Option<&str>,
        _change: &mut dyn FnMut(&mut review::ReviewRecord),
    ) -> Result<review::ReviewRecord, String> {
        Err("this host keeps no reviews".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// A host that implements only what every host must, holding one
    /// file of three lines.
    struct Least;

    impl Host for Least {
        fn git(&self, _: &Path, _: &[&str], _: &[i32]) -> Result<String, String> {
            Err("no git here".to_owned())
        }
        fn repository_root(&self, _: &Path) -> Result<PathBuf, String> {
            Err("no git here".to_owned())
        }
        fn read_file(&self, _: &Path) -> Result<String, Unreadable> {
            Ok("one\ntwo\nthree\n".to_owned())
        }
        fn list_dir(&self, _: &Path) -> Result<Vec<DirEntry>, String> {
            Ok(Vec::new())
        }
        fn write_file(&self, _: &Path, _: &Path, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn delete_file(&self, _: &Path, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn restore_to_head(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
            Ok(())
        }
        fn syntax_theme(&self) -> String {
            String::new()
        }
    }

    #[test]
    fn a_host_grants_no_more_than_it_implements() {
        let host = Least;
        let root = Path::new("work");
        let file = root.join("a.txt");
        assert_eq!(host.read_bytes(&file), Err(Unreadable::NotText));
        assert!(host.delete_dir(root, &root.join("src")).is_err());
        assert!(host.rename_path(root, &file, &root.join("b.txt")).is_err());
        assert_eq!(host.delivery_target(root), None);
        let identity = uze_contract::Identity {
            marketplace: "market".to_owned(),
            plugin: "gsd".to_owned(),
            id: "gsd".to_owned(),
        };
        assert!(host.media(&identity, "pet.png").is_err());
        assert!(host.review_record(root, None).is_err());
    }

    #[test]
    fn a_host_counts_lines_from_the_contents_it_reads() {
        assert_eq!(Least.count_lines(Path::new("a.txt")), 3);
    }

    #[test]
    fn unreadable_says_why_in_a_sentence() {
        assert_eq!(Unreadable::NotText.to_string(), "not readable as text");
        assert_eq!(
            Unreadable::Failed("permission denied".to_owned()).to_string(),
            "permission denied"
        );
    }

    #[test]
    fn a_directory_compares_before_a_file() {
        let directory = DirEntry {
            directory: true,
            name: "zeta".to_owned(),
        };
        let file = DirEntry {
            directory: false,
            name: "alpha".to_owned(),
        };
        assert!(directory < file);
        assert_eq!(directory.partial_cmp(&file), Some(std::cmp::Ordering::Less));
    }
}
