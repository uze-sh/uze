//! What keeps a Git process UZE starts from running anything the
//! repository's own configuration names.
//!
//! A repository's configuration and its hooks live in its common Git
//! directory, which every agent that can commit can also write. So the
//! repository's configuration is never trusted to name a program: hooks and
//! the file-system monitor are switched off outright, and the keys that
//! name a program under a name the repository picks — a filter, a merge
//! driver — or that pick the program a transport runs are answered with
//! the operator's own value, or an inert one, whenever the repository sets
//! them. The operator's own configuration is the system, global and
//! environment scopes: a per-repository setting that is the operator's
//! belongs in global configuration under `includeIf "gitdir:…"`, which Git
//! reports as global.

/// What every invocation carries, read or write, whatever the platform.
/// A hook is a file in a directory any agent writes, so none runs in
/// UZE's own operations, `pre-push` and `post-checkout` included: what
/// the operator wants run before a delivery is the approved
/// `workspace.gate`. A file-system monitor only makes a scan faster. The
/// `ext` transport runs whatever its URL says, and a per-protocol setting
/// beats the repository's `protocol.allow`. A submodule carries a
/// configuration of its own that the override below never sees, so
/// nothing recurses into one. A signature shown in a log runs the signing
/// program the repository may name.
pub(crate) const EVERY_INVOCATION: &[(&str, &str)] = &[
    ("core.fsmonitor", "false"),
    ("core.hooksPath", uze_platform::fs::NULL_DEVICE),
    ("protocol.ext.allow", "never"),
    ("submodule.recurse", "false"),
    ("fetch.recurseSubmodules", "false"),
    ("push.recurseSubmodules", "no"),
    ("log.showSignature", "false"),
];

/// The keys that name a program Git runs, asked of Git with their scope.
/// Git matches the canonical key: section and name lowercased, a
/// subsection as written.
pub(crate) const PROGRAM_KEYS: &str = concat!(
    r"^(core\.(sshcommand|askpass|gitproxy|alternaterefscommand)",
    r"|credential\..*helper",
    r"|filter\..*\.(clean|smudge|process)",
    r"|merge\..*\.driver",
    r"|remote\..*\.(uploadpack|receivepack)",
    r"|gpg\.(.*\.)?program|gpg\.ssh\.defaultkeycommand",
    r"|http\..*savecookies)$",
);

/// Which side of the transport a command came through.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    Read,
    Write,
}

/// The subcommands that reach a remote, and with it an SSH that may want a
/// passphrase or a host-key answer.
pub(crate) const NETWORK_SUBCOMMANDS: &[&str] = &["fetch", "pull", "push", "clone", "ls-remote"];

/// Subcommands that only read or move objects and references: they hash
/// no work tree file, so run no filter, and merge, sign and transport
/// nothing.
const PLUMBING: &[&str] = &[
    "rev-parse",
    "rev-list",
    "for-each-ref",
    "show-ref",
    "merge-base",
    "ls-tree",
    "cat-file",
    "diff-tree",
    "write-tree",
    "name-rev",
    "describe",
    "log",
    "show",
    "symbolic-ref",
    "update-ref",
    "config",
    "branch",
    "init",
    "clean",
    "var",
    "check-ref-format",
    "check-attr",
    "check-ignore",
    "count-objects",
    "mktree",
    "clone",
];

/// `worktree` verbs that only edit its administrative records.
const INERT_WORKTREE_VERBS: &[&str] = &["prune", "lock", "unlock", "list", "repair", "move"];

/// Subcommands that ask for a credential or speak a transport.
///
/// `clone` is not among them: it runs in no repository, so no
/// repository's configuration reaches it, and the one it makes holds only
/// what the clone itself wrote.
const TRANSPORT_SUBCOMMANDS: &[&str] = &["fetch", "pull", "push", "ls-remote", "credential"];

/// What a command can reach that the configuration names a program for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Reach {
    /// No program a configuration key names.
    Nothing,
    /// A filter (any command that hashes or writes work tree content), a
    /// merge driver or a signing program.
    Content,
    /// A transport: SSH, a credential helper, an askpass, an upload or
    /// receive pack.
    Transport,
}

/// The subcommand `args` run and where it is, skipping the global options
/// Git accepts before it.
pub(crate) fn subcommand_of<'a>(args: &[&'a str]) -> Option<(usize, &'a str)> {
    let mut index = 0;
    while let Some(argument) = args.get(index) {
        match *argument {
            "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace" => index += 2,
            option if option.starts_with('-') => index += 1,
            subcommand => return Some((index, subcommand)),
        }
    }
    None
}

/// Whether `args` run one of [`NETWORK_SUBCOMMANDS`].
pub(crate) fn reaches_a_remote(args: &[&str]) -> bool {
    subcommand_of(args).is_some_and(|(_, subcommand)| NETWORK_SUBCOMMANDS.contains(&subcommand))
}

/// What `args` can reach that a configuration key names a program for.
pub(crate) fn reach_of(args: &[&str]) -> Reach {
    let Some((at, subcommand)) = subcommand_of(args) else {
        return Reach::Nothing;
    };
    if TRANSPORT_SUBCOMMANDS.contains(&subcommand) {
        return Reach::Transport;
    }
    let rest = &args[at + 1..];
    let given = |options: &[&str]| rest.iter().any(|argument| options.contains(argument));
    if subcommand == "remote" {
        // Its verbs edit or read the remotes' configuration, but these
        // three, and an `add` that fetches, speak to the remote.
        let speaks = rest
            .first()
            .is_some_and(|verb| matches!(*verb, "show" | "update" | "prune"))
            || (rest.first() == Some(&"add") && given(&["-f", "--fetch"]));
        return if speaks {
            Reach::Transport
        } else {
            Reach::Nothing
        };
    }
    let inert = match subcommand {
        "cat-file" => !given(&["--filters", "--textconv"]),
        "worktree" => rest
            .first()
            .is_some_and(|verb| INERT_WORKTREE_VERBS.contains(verb)),
        // Index against a tree: no work tree file is read.
        "diff" | "diff-index" => given(&["--cached", "--staged"]),
        "ls-files" => !given(&["-m", "--modified"]),
        "hash-object" => given(&["--no-filters"]) || (given(&["--stdin"]) && !given(&["--path"])),
        "commit-tree" => given(&["--no-gpg-sign"]),
        plumbing => PLUMBING.contains(&plumbing),
    };
    if inert {
        Reach::Nothing
    } else {
        Reach::Content
    }
}

/// One entry of the configuration as Git reported it.
struct Entry<'a> {
    operator: bool,
    key: &'a str,
    value: Option<&'a str>,
}

/// What a command adds so the repository's [`PROGRAM_KEYS`] take no effect.
#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct Overrides {
    /// `key=value` settings, each given as a `-c`.
    pub(crate) settings: Vec<String>,
    /// Options placed right after the subcommand: the keys Git reads
    /// first-wins, which a `-c` cannot beat.
    pub(crate) options: Vec<String>,
}

impl Overrides {
    /// `args` with [`Overrides::options`] right after the subcommand.
    pub(crate) fn placed<'a>(&'a self, args: &[&'a str]) -> Vec<&'a str> {
        let mut placed: Vec<&str> = args.to_vec();
        if let Some((at, _)) = subcommand_of(args) {
            for (offset, option) in self.options.iter().enumerate() {
                placed.insert(at + 1 + offset, option);
            }
        }
        placed
    }

    fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key.contains(['=', '\n']) {
            return Err(format!(
                "the configuration names `{}`, which UZE cannot override",
                key.escape_debug()
            ));
        }
        let setting = format!("{key}={value}");
        if !self.settings.contains(&setting) {
            self.settings.push(setting);
        }
        Ok(())
    }
}

/// The signing programs, each with what Git runs when nobody names one.
const SIGNING_PROGRAMS: &[(&str, &str)] = &[
    ("gpg.program", "gpg"),
    ("gpg.openpgp.program", "gpg"),
    ("gpg.x509.program", "gpgsm"),
    ("gpg.ssh.program", "ssh-keygen"),
    // Fails a signing that needed it rather than running anything.
    ("gpg.ssh.defaultKeyCommand", "false"),
];

/// The overrides for `args`, from what `git config -z --show-scope
/// --get-regexp` printed for [`PROGRAM_KEYS`].
///
/// A key whose name is fixed is given on every command that can reach it,
/// whether or not the listing found it. The keys named under a name the
/// repository picks — a filter, a merge driver, a URL's cookie jar — are
/// answered from what the listing found.
/// `ssh_program` is the operator's `GIT_SSH`, which a configured
/// `core.sshCommand` would otherwise outrank.
pub(crate) fn overrides_for(
    listing: &str,
    args: &[&str],
    reach: Reach,
    side: Side,
    ssh_program: Option<&str>,
) -> Result<Overrides, String> {
    let entries = parse(listing)?;
    let operator_value = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|entry| entry.operator && entry.key.eq_ignore_ascii_case(key))
            .map(|entry| entry.value.unwrap_or("true"))
    };
    let mut overrides = Overrides::default();
    for entry in entries.iter().filter(|entry| !entry.operator) {
        let canonical = entry.key.to_ascii_lowercase();
        let named_by_the_repository = canonical.starts_with("filter.")
            || (canonical.starts_with("merge.") && canonical.ends_with(".driver"))
            || (canonical.starts_with("http.") && canonical.ends_with("savecookies"));
        if !named_by_the_repository {
            continue;
        }
        match operator_value(entry.key) {
            Some(value) => overrides.set(entry.key, value)?,
            None if canonical.starts_with("filter.") => {
                overrides.set(entry.key, "")?;
                if let Some(driver) = filter_driver(entry.key) {
                    overrides.set(&format!("filter.{driver}.required"), "false")?;
                }
            }
            // A driver that fails leaves a conflict for the person to see,
            // rather than silently keeping one side as an empty one would.
            None => overrides.set(entry.key, "false")?,
        }
    }
    if reach == Reach::Content && side == Side::Write {
        for (key, default) in SIGNING_PROGRAMS {
            overrides.set(key, operator_value(key).unwrap_or(default))?;
        }
    }
    if reach == Reach::Transport {
        transport_overrides(&mut overrides, &entries, args, ssh_program)?;
    }
    Ok(overrides)
}

fn transport_overrides(
    overrides: &mut Overrides,
    entries: &[Entry<'_>],
    args: &[&str],
    ssh_program: Option<&str>,
) -> Result<(), String> {
    let operator_value = |key: &str| {
        entries
            .iter()
            .rev()
            .find(|entry| entry.operator && entry.key.eq_ignore_ascii_case(key))
            .map(|entry| entry.value.unwrap_or("true"))
    };
    let quoted_ssh = ssh_program.map(|program| format!("'{}'", program.replace('\'', r"'\''")));
    overrides.set(
        "core.sshCommand",
        operator_value("core.sshCommand")
            .or(quoted_ssh.as_deref())
            .unwrap_or("ssh"),
    )?;
    // An empty askpass or alternate refs command is no command at all.
    overrides.set("core.askPass", operator_value("core.askPass").unwrap_or(""))?;
    overrides.set(
        "core.alternateRefsCommand",
        operator_value("core.alternateRefsCommand").unwrap_or(""),
    )?;
    // A proxy is first-wins, so the repository's cannot be outranked: the
    // one transport that runs it is not spoken at all.
    overrides.set("protocol.git.allow", "never")?;
    // An empty helper empties the list Git built so far, the repository's
    // URL-scoped helpers included; the operator's are then given again, in
    // their order.
    overrides.settings.push("credential.helper=".to_owned());
    for entry in entries.iter().filter(|entry| {
        let canonical = entry.key.to_ascii_lowercase();
        entry.operator && canonical.starts_with("credential.") && canonical.ends_with("helper")
    }) {
        match entry.value {
            Some(value) => {
                if entry.key.contains(['=', '\n']) {
                    return Err(format!(
                        "the configuration names `{}`, which UZE cannot pass on",
                        entry.key.escape_debug()
                    ));
                }
                overrides.settings.push(format!("{}={value}", entry.key));
            }
            None => overrides.settings.push(entry.key.to_owned()),
        }
    }
    // Upload and receive packs are first-wins too, so they are given on the
    // command line, which outranks the configuration.
    if let Some((at, subcommand)) = subcommand_of(args) {
        let pack = match subcommand {
            "fetch" | "pull" | "ls-remote" => {
                Some(("uploadpack", "--upload-pack", "git-upload-pack"))
            }
            "push" => Some(("receivepack", "--receive-pack", "git-receive-pack")),
            _ => None,
        };
        if let Some((name, option, default)) = pack {
            let program = remote_named(&args[at + 1..])
                .and_then(|remote| operator_value(&format!("remote.{remote}.{name}")))
                .unwrap_or(default);
            overrides.options.push(format!("{option}={program}"));
        }
    }
    Ok(())
}

/// The remote a transport subcommand's operands name first.
fn remote_named<'a>(operands: &[&'a str]) -> Option<&'a str> {
    let mut after_end = false;
    operands.iter().copied().find(|operand| {
        if after_end {
            return true;
        }
        if *operand == "--" {
            after_end = true;
            return false;
        }
        !operand.starts_with('-')
    })
}

fn filter_driver(key: &str) -> Option<&str> {
    let (section, rest) = key.split_once('.')?;
    if !section.eq_ignore_ascii_case("filter") {
        return None;
    }
    rest.rsplit_once('.').map(|(driver, _)| driver)
}

/// Entries of `git config -z --show-scope --get-regexp`: a scope, then the
/// key and its value separated by a line break, each NUL-terminated; a key
/// set with no value has no line break.
fn parse(listing: &str) -> Result<Vec<Entry<'_>>, String> {
    let mut fields = listing.split('\0');
    let mut entries = Vec::new();
    while let Some(scope) = fields.next() {
        if scope.is_empty() {
            break;
        }
        let Some(field) = fields.next() else {
            return Err("Git's configuration listing ended mid-entry".to_owned());
        };
        let (key, value) = match field.split_once('\n') {
            Some((key, value)) => (key, Some(value)),
            None => (field, None),
        };
        entries.push(Entry {
            operator: matches!(scope, "system" | "global" | "command"),
            key,
            value,
        });
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(entries: &[(&str, &str)]) -> String {
        entries
            .iter()
            .map(|(scope, field)| format!("{scope}\0{field}\0"))
            .collect()
    }

    fn settings_for(entries: &[(&str, &str)], args: &[&str], side: Side) -> Overrides {
        overrides_for(&listing(entries), args, reach_of(args), side, None).unwrap()
    }

    #[test]
    fn a_filter_or_merge_driver_the_repository_defines_is_answered_with_nothing() {
        let overrides = settings_for(
            &[
                ("local", "filter.Evil.smudge\nsh -c pwn"),
                ("local", "merge.evil.driver\nsh -c pwn"),
                ("worktree", "filter.other.process\nsh -c pwn"),
            ],
            &["status"],
            Side::Read,
        );
        assert_eq!(
            overrides.settings,
            [
                "filter.Evil.smudge=",
                "filter.Evil.required=false",
                "merge.evil.driver=false",
                "filter.other.process=",
                "filter.other.required=false",
            ]
        );
        assert!(overrides.options.is_empty());
    }

    #[test]
    fn the_operator_s_own_value_wins_over_the_repository_s() {
        let overrides = settings_for(
            &[
                ("global", "filter.lfs.process\ngit-lfs filter-process"),
                ("local", "filter.lfs.process\nsh -c pwn"),
            ],
            &["worktree", "add", "x"],
            Side::Write,
        );
        assert_eq!(
            overrides.settings[0],
            "filter.lfs.process=git-lfs filter-process"
        );
        assert!(
            overrides.settings.contains(&"gpg.program=gpg".to_owned()),
            "a write that can sign names the signing program: {:?}",
            overrides.settings
        );
    }

    /// The fixed-name keys are given whether or not the repository set them
    /// when Git was asked: it can be rewritten before the command reads it.
    #[test]
    fn a_transport_is_always_the_operator_s() {
        let overrides = settings_for(
            &[
                ("global", "credential.helper\nstore"),
                ("global", "credential.https://forge.example.helper\nmanager"),
                (
                    "global",
                    "remote.origin.uploadpack\n/opt/git/bin/git-upload-pack",
                ),
            ],
            &["fetch", "--quiet", "--", "origin", "main"],
            Side::Write,
        );
        assert_eq!(
            overrides.settings,
            [
                "core.sshCommand=ssh",
                "core.askPass=",
                "core.alternateRefsCommand=",
                "protocol.git.allow=never",
                "credential.helper=",
                "credential.helper=store",
                "credential.https://forge.example.helper=manager",
            ]
        );
        assert_eq!(
            overrides.options,
            ["--upload-pack=/opt/git/bin/git-upload-pack"]
        );
    }

    #[test]
    fn the_operator_s_ssh_is_kept() {
        let configured = overrides_for(
            &listing(&[("global", "core.sshcommand\nssh -i key")]),
            &["push", "origin"],
            Reach::Transport,
            Side::Write,
            Some("/usr/bin/plink"),
        )
        .unwrap();
        assert_eq!(configured.settings[0], "core.sshCommand=ssh -i key");
        let from_the_environment = overrides_for(
            "",
            &["push", "origin"],
            Reach::Transport,
            Side::Write,
            Some("/opt/it's/ssh"),
        )
        .unwrap();
        assert_eq!(
            from_the_environment.settings[0],
            r"core.sshCommand='/opt/it'\''s/ssh'"
        );
        assert_eq!(
            from_the_environment.options,
            ["--receive-pack=git-receive-pack"]
        );
        assert_eq!(
            from_the_environment.placed(&["-c", "a=b", "push", "--quiet", "origin"]),
            [
                "-c",
                "a=b",
                "push",
                "--receive-pack=git-receive-pack",
                "--quiet",
                "origin"
            ]
        );
    }

    #[test]
    fn a_key_that_cannot_be_written_back_refuses_the_command() {
        assert!(
            overrides_for(
                &listing(&[("local", "filter.x.smudge=pwn;#.clean\ncat")]),
                &["status"],
                Reach::Content,
                Side::Read,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn only_commands_that_can_reach_a_program_ask_for_the_configuration() {
        assert_eq!(reach_of(&["status", "--porcelain"]), Reach::Content);
        assert_eq!(reach_of(&["ls-remote", "origin"]), Reach::Transport);
        assert_eq!(reach_of(&["credential", "fill"]), Reach::Transport);
        assert_eq!(reach_of(&["clone", "--", "x", "y"]), Reach::Nothing);
        assert_eq!(reach_of(&["remote", "get-url", "origin"]), Reach::Nothing);
        assert_eq!(reach_of(&["remote", "update"]), Reach::Transport);
        assert_eq!(
            reach_of(&["cat-file", "--filters", "HEAD:x"]),
            Reach::Content
        );
        assert_eq!(reach_of(&["cat-file", "-p", "HEAD:x"]), Reach::Nothing);
        assert_eq!(reach_of(&["rev-parse", "HEAD"]), Reach::Nothing);
        assert_eq!(reach_of(&["log", "-1"]), Reach::Nothing);
        assert_eq!(
            reach_of(&["diff-index", "--cached", "HEAD"]),
            Reach::Nothing
        );
        assert_eq!(reach_of(&["diff-index", "HEAD"]), Reach::Content);
        assert_eq!(reach_of(&["hash-object", "--stdin"]), Reach::Nothing);
        assert_eq!(
            reach_of(&["hash-object", "--stdin", "--path", "x"]),
            Reach::Content
        );
        assert_eq!(
            reach_of(&["commit-tree", "--no-gpg-sign", "t"]),
            Reach::Nothing
        );
        assert_eq!(reach_of(&["commit-tree", "t"]), Reach::Content);
        assert_eq!(
            reach_of(&["-c", "a=b", "worktree", "add", "x"]),
            Reach::Content
        );
        assert_eq!(reach_of(&["rebase", "main"]), Reach::Content);
        assert_eq!(reach_of(&["worktree", "prune"]), Reach::Nothing);
        assert_eq!(reach_of(&["update-ref", "-d", "x"]), Reach::Nothing);
    }

    #[test]
    fn a_subcommand_that_reaches_a_remote_is_found_past_the_global_options() {
        assert!(reaches_a_remote(&["fetch", "--quiet", "origin"]));
        assert!(reaches_a_remote(&["-c", "http.proxy=x", "push", "origin"]));
        assert!(reaches_a_remote(&["--no-pager", "-C", "push", "ls-remote"]));
        assert!(!reaches_a_remote(&["-c", "fetch.prune=true", "status"]));
        assert!(!reaches_a_remote(&["log", "push"]));
        assert!(!reaches_a_remote(&[]));
    }
}
