//! The actions a plugin's extension adds to the keyboard.
//!
//! [`Action`](crate::Action) stays closed, `Copy` and listed once in
//! [`ALL_ACTIONS`](crate::ALL_ACTIONS): what a plugin declares is not known
//! when this crate is built. Each declared action is one interned number,
//! [`ExtensionActionId`], carried by the one variant
//! `Action::Extension`; the table here maps it to its keymap name,
//! `ext.<plugin>@<marketplace>/<id>.<action>`, and to the words the
//! manifest gave it. The words arrive already sanitized: this crate names
//! no contract, and the host cleans every third-party string before it
//! reaches a surface.
//!
//! The interner only grows. A name is a short string written by a manifest
//! or by the operator's file, and an id that outlives its extension still
//! names it rightly; what an extension's keys reach is the keymap's
//! [`ExtensionTable`] for it, which leaves with the extension.

use std::collections::HashMap;
use std::sync::{OnceLock, PoisonError, RwLock};

use crate::chord::{Chord, ChordProblem};

/// What every extension action's name starts with.
pub const PREFIX: &str = "ext.";

/// The longest name read as an extension action: a plugin, a marketplace
/// and two ids of at most 32 characters, with room to spare.
const NAME_MAX: usize = 256;

/// One extension action, as a number the keyboard can copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ExtensionActionId(u32);

/// What a manifest says of an action, cleaned by the host.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Described {
    pub label: String,
    pub description: String,
    /// It asks before running (`confirm`): the same standing the host's
    /// destructive actions have.
    pub destructive: bool,
}

struct Entry {
    name: String,
    described: Option<Described>,
}

#[derive(Default)]
struct Interner {
    entries: Vec<Entry>,
    by_name: HashMap<String, ExtensionActionId>,
}

fn interner() -> &'static RwLock<Interner> {
    static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();
    INTERNER.get_or_init(|| RwLock::new(Interner::default()))
}

/// `ext.<owner>.<action>`, where the owner is `plugin@marketplace/id`.
pub fn action_name(owner: &str, action: &str) -> String {
    format!("{PREFIX}{owner}.{action}")
}

/// Whether `name` is spelled as an extension action: the prefix, an owner
/// `plugin@marketplace/id`, a dot and the action's id, with nothing blank
/// or unprintable in it.
pub fn is_name(name: &str) -> bool {
    if name.len() > NAME_MAX || name.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        return false;
    }
    let Some((owner, action)) = name
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.rsplit_once('.'))
    else {
        return false;
    };
    let Some((plugin, rest)) = owner.split_once('@') else {
        return false;
    };
    let Some((marketplace, id)) = rest.split_once('/') else {
        return false;
    };
    [plugin, marketplace, id, action]
        .iter()
        .all(|part| !part.is_empty() && !part.contains(['@', '/']))
}

/// The number `name` is known by, handed out on first sight. The caller
/// says it is a name ([`is_name`]); a name is never refused here.
pub fn intern(name: &str) -> ExtensionActionId {
    if let Some(id) = read().by_name.get(name) {
        return *id;
    }
    let mut interner = interner().write().unwrap_or_else(PoisonError::into_inner);
    if let Some(id) = interner.by_name.get(name) {
        return *id;
    }
    let id = ExtensionActionId(interner.entries.len() as u32);
    interner.entries.push(Entry {
        name: name.to_owned(),
        described: None,
    });
    interner.by_name.insert(name.to_owned(), id);
    id
}

fn read() -> std::sync::RwLockReadGuard<'static, Interner> {
    interner().read().unwrap_or_else(PoisonError::into_inner)
}

/// The keymap name of `id`.
pub fn name(id: ExtensionActionId) -> String {
    read()
        .entries
        .get(id.0 as usize)
        .map(|entry| entry.name.clone())
        .unwrap_or_default()
}

/// The extension `id` belongs to: `plugin@marketplace/id`.
pub fn owner(id: ExtensionActionId) -> String {
    let name = name(id);
    split(&name).0.to_owned()
}

/// The action's own id inside its extension.
pub fn local(id: ExtensionActionId) -> String {
    let name = name(id);
    split(&name).1.to_owned()
}

fn split(name: &str) -> (&str, &str) {
    name.strip_prefix(PREFIX)
        .and_then(|rest| rest.rsplit_once('.'))
        .unwrap_or(("", name))
}

/// Gives `id` the words its manifest wrote, replacing any earlier ones: a
/// recompile may rename an action.
pub fn describe(id: ExtensionActionId, described: Described) {
    let mut interner = interner().write().unwrap_or_else(PoisonError::into_inner);
    if let Some(entry) = interner.entries.get_mut(id.0 as usize) {
        entry.described = Some(described);
    }
}

fn described(id: ExtensionActionId) -> Option<Described> {
    read()
        .entries
        .get(id.0 as usize)
        .and_then(|entry| entry.described.clone())
}

/// What a menu or the index calls it: the manifest's label, or its id
/// before any manifest said one.
pub fn label(id: ExtensionActionId) -> String {
    described(id)
        .map(|described| described.label)
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| local(id))
}

pub fn description(id: ExtensionActionId) -> String {
    described(id)
        .map(|described| described.description)
        .filter(|description| !description.is_empty())
        .unwrap_or_else(|| format!("An action of {}", owner(id)))
}

pub fn destructive(id: ExtensionActionId) -> bool {
    described(id).is_some_and(|described| described.destructive)
}

/// Where a declared surface stands in front of the keyboard: which
/// extension, and which of its surfaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Front<'a> {
    pub owner: &'a str,
    pub surface: &'a str,
}

/// The surface an action is offered on when it is live over the pane and
/// every surface alike: an overlay's door, reached from anywhere.
pub const EVERYWHERE: &str = "*";

/// What a door's name ends with, after the extension's own: `open:` and
/// the surface it opens.
pub const DOOR: &str = "open:";

/// One action an extension offers the keyboard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Offered {
    pub action: ExtensionActionId,
    /// The surface it is offered on, and live only while in front.
    pub surface: String,
    /// The chord the manifest suggests, already read in the keymap's
    /// grammar, or why it is no chord.
    pub suggested: Option<Result<Chord, ChordProblem>>,
}

/// Everything one extension offers the keyboard, in the order it declares
/// it. Registered while the extension is enabled; see
/// [`crate::Keymap::with_extensions`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionTable {
    /// `plugin@marketplace/id`.
    pub owner: String,
    pub offered: Vec<Offered>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, Keymap, KeymapFile, Resolution, Scope, default_keymap, load};

    const OPEN: [Scope; 4] = [
        Scope::Global,
        Scope::Workspace,
        Scope::Declared,
        Scope::Surfaces,
    ];

    fn chord(text: &str) -> Chord {
        Chord::parse(text).expect(text)
    }

    fn offered(owner: &str, action: &str, surface: &str, key: Option<&str>) -> Offered {
        Offered {
            action: intern(&action_name(owner, action)),
            surface: surface.to_owned(),
            suggested: key.map(Chord::parse),
        }
    }

    fn table(owner: &str, offered: Vec<Offered>) -> ExtensionTable {
        ExtensionTable {
            owner: owner.to_owned(),
            offered,
        }
    }

    fn front<'a>(owner: &'a str, surface: &'a str) -> Option<Front<'a>> {
        Some(Front { owner, surface })
    }

    fn act(owner: &str, action: &str) -> Resolution {
        Resolution::Act(Action::Extension(intern(&action_name(owner, action))))
    }

    fn written(json: &str) -> Keymap {
        let file: KeymapFile = serde_json::from_str(json).expect("a keymap file");
        load::resolve(&file).expect("it loads").keymap
    }

    #[test]
    fn an_overlay_door_opens_from_over_the_pane_but_never_takes_a_typed_key() {
        let owner = "jump@studio/jump";
        let keymap = default_keymap().with_extensions(vec![table(
            owner,
            vec![
                offered(owner, "open:picker", EVERYWHERE, Some("alt+j")),
                offered(owner, "open:other", EVERYWHERE, Some("j")),
            ],
        )]);
        let over_the_pane = [
            Scope::Global,
            Scope::Workspace,
            Scope::Pane,
            Scope::Surfaces,
        ];
        assert_eq!(
            keymap.resolve_for(chord("alt+j"), &over_the_pane, None),
            act(owner, "open:picker")
        );
        assert_eq!(
            keymap.resolve_for(chord("j"), &over_the_pane, None),
            Resolution::Fallthrough,
            "a bare letter stays the agent's"
        );
        assert!(
            keymap.extension_problems()[0]
                .message
                .contains("would be typing in an agent's pane")
        );
        assert_eq!(
            keymap.resolve_for(chord("alt+j"), &[Scope::Global, Scope::Confirm], None),
            Resolution::Fallthrough,
            "a question seals it out"
        );
    }

    #[test]
    fn two_extensions_doors_on_one_chord_leave_it_to_the_first() {
        let first = "jump@studio/jump";
        let second = "leap@studio/leap";
        let keymap = default_keymap().with_extensions(vec![
            table(
                first,
                vec![offered(first, "open:picker", EVERYWHERE, Some("alt+j"))],
            ),
            table(
                second,
                vec![offered(second, "open:picker", EVERYWHERE, Some("alt+j"))],
            ),
        ]);
        assert_eq!(
            keymap.resolve_for(chord("alt+j"), &OPEN, None),
            act(first, "open:picker")
        );
        let said: Vec<&str> = keymap
            .extension_problems()
            .iter()
            .map(|problem| problem.message.as_str())
            .collect();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains(&action_name(first, "open:picker")),
            "{said:?}"
        );
        assert!(
            said[0].contains(&action_name(second, "open:picker")),
            "{said:?}"
        );
    }

    #[test]
    fn a_free_suggestion_answers_only_while_its_surface_is_in_front() {
        let owner = "gsd@luizfbalves/gsd-band";
        let keymap = default_keymap().with_extensions(vec![table(
            owner,
            vec![offered(owner, "refresh", "board", Some("ctrl+r"))],
        )]);
        assert!(keymap.extension_problems().is_empty());
        assert_eq!(
            keymap.resolve_for(chord("ctrl+r"), &OPEN, front(owner, "board")),
            act(owner, "refresh")
        );
        assert_eq!(
            keymap.resolve_for(chord("ctrl+r"), &OPEN, front(owner, "list")),
            Resolution::Fallthrough,
            "another surface of the same extension does not offer it"
        );
        assert_eq!(
            keymap.resolve_for(chord("ctrl+r"), &OPEN, None),
            Resolution::Fallthrough
        );
        let refresh = Action::Extension(intern(&action_name(owner, "refresh")));
        assert_eq!(keymap.chord_for(refresh, &OPEN), Some(chord("ctrl+r")));
        assert_eq!(
            keymap.chord_for(refresh, &[Scope::Global, Scope::Workspace, Scope::Pane]),
            None,
            "no chord is printed where its surface is not"
        );
    }

    #[test]
    fn a_suggestion_the_host_holds_is_not_bound_and_is_said() {
        let owner = "taken@m/x";
        let keymap = default_keymap().with_extensions(vec![table(
            owner,
            vec![
                offered(owner, "quit-it", "main", Some("ctrl+q")),
                offered(owner, "inspect", "main", Some("i")),
                offered(owner, "door", "main", Some("alt+g")),
            ],
        )]);
        assert_eq!(
            keymap.resolve_for(chord("ctrl+q"), &OPEN, front(owner, "main")),
            Resolution::Act(Action::Quit),
            "the host always wins"
        );
        assert_eq!(
            keymap.resolve_for(chord("i"), &OPEN, front(owner, "main")),
            Resolution::Act(Action::ToggleInspector)
        );
        let offers: Vec<_> = keymap.extension_actions(Front {
            owner,
            surface: "main",
        });
        assert_eq!(
            offers.len(),
            3,
            "still offered, by pointer and in the index"
        );
        assert!(offers.iter().all(|(_, chord)| chord.is_none()));
        let said: Vec<&str> = keymap
            .extension_problems()
            .iter()
            .map(|problem| problem.message.as_str())
            .collect();
        assert_eq!(said.len(), 3, "{said:?}");
        assert!(
            said[0].contains("`ctrl+q`") && said[0].contains("quit"),
            "{said:?}"
        );
    }

    #[test]
    fn two_extensions_suggesting_one_chord_never_meet() {
        let (first, second) = ("one@m/a", "two@m/b");
        let keymap = default_keymap().with_extensions(vec![
            table(first, vec![offered(first, "go", "main", Some("g"))]),
            table(second, vec![offered(second, "run", "main", Some("g"))]),
        ]);
        assert!(keymap.extension_problems().is_empty());
        assert_eq!(
            keymap.resolve_for(chord("g"), &OPEN, front(first, "main")),
            act(first, "go")
        );
        assert_eq!(
            keymap.resolve_for(chord("g"), &OPEN, front(second, "main")),
            act(second, "run")
        );
    }

    #[test]
    fn a_conflict_inside_one_extension_drops_only_that_binding() {
        let owner = "clash@m/x";
        let keymap = default_keymap().with_extensions(vec![table(
            owner,
            vec![
                offered(owner, "first", "main", Some("x")),
                offered(owner, "second", "main", Some("x")),
                offered(owner, "third", "main", Some("y")),
                offered(owner, "elsewhere", "other", Some("x")),
            ],
        )]);
        assert_eq!(
            keymap.resolve_for(chord("x"), &OPEN, front(owner, "main")),
            act(owner, "first")
        );
        assert_eq!(
            keymap.resolve_for(chord("y"), &OPEN, front(owner, "main")),
            act(owner, "third"),
            "the rest of the table stands"
        );
        assert_eq!(
            keymap.resolve_for(chord("x"), &OPEN, front(owner, "other")),
            act(owner, "elsewhere"),
            "two surfaces never share a screen"
        );
        let said = keymap.extension_problems();
        assert_eq!(said.len(), 1, "{said:?}");
        assert_eq!(said[0].severity, load::Severity::Warning);
        assert!(said[0].message.contains("ext.clash@m/x.second"), "{said:?}");
    }

    #[test]
    fn the_operators_file_beats_the_suggestion() {
        let owner = "gsd@luizfbalves/gsd-band";
        let keymap = written(
            r#"{"bindings":{"declared":{
                "ext.gsd@luizfbalves/gsd-band.refresh":["r"],
                "ext.gsd@luizfbalves/gsd-band.inspect":["i"],
                "ext.gsd@luizfbalves/gsd-band.archive":[]
            }}}"#,
        )
        .with_extensions(vec![table(
            owner,
            vec![
                offered(owner, "refresh", "board", Some("ctrl+r")),
                offered(owner, "inspect", "board", None),
                offered(owner, "archive", "board", Some("ctrl+d")),
            ],
        )]);
        assert!(
            keymap.extension_problems().is_empty(),
            "{:?}",
            keymap.extension_problems()
        );
        let board = front(owner, "board");
        assert_eq!(
            keymap.resolve_for(chord("r"), &OPEN, board),
            act(owner, "refresh")
        );
        assert_eq!(
            keymap.resolve_for(chord("ctrl+r"), &OPEN, board),
            Resolution::Fallthrough,
            "the suggestion is replaced, not kept beside"
        );
        assert_eq!(
            keymap.resolve_for(chord("i"), &OPEN, board),
            act(owner, "inspect"),
            "the operator may take a letter of the surfaces' shared vocabulary"
        );
        assert_eq!(
            keymap.resolve_for(chord("ctrl+d"), &OPEN, board),
            Resolution::Fallthrough,
            "an empty list unbinds"
        );
        // What is saved back keeps what the operator wrote.
        let saved = load::difference_from_default(&keymap);
        assert_eq!(
            saved.bindings["declared"]["ext.gsd@luizfbalves/gsd-band.refresh"],
            vec!["r".to_owned()]
        );
        // And a rebind of a host key keeps the tables.
        let rebound = keymap
            .rebind(Action::NewShellTab, Scope::Workspace, Some(chord("f4")))
            .expect("no conflict");
        assert_eq!(
            rebound.resolve_for(chord("r"), &OPEN, board),
            act(owner, "refresh")
        );
    }

    #[test]
    fn the_operator_cannot_take_a_key_the_host_holds_everywhere() {
        let owner = "greedy@m/x";
        let keymap = written(r#"{"bindings":{"declared":{"ext.greedy@m/x.leave":["ctrl+q"]}}}"#)
            .with_extensions(vec![table(
                owner,
                vec![offered(owner, "leave", "main", None)],
            )]);
        assert_eq!(
            keymap.resolve_for(chord("ctrl+q"), &OPEN, front(owner, "main")),
            Resolution::Act(Action::Quit)
        );
        assert_eq!(keymap.extension_problems().len(), 1);
    }

    #[test]
    fn a_name_for_an_extension_not_installed_is_a_warning() {
        let file: KeymapFile = serde_json::from_str(
            r#"{"bindings":{
                "declared":{"ext.gone@m/x.refresh":["r"]},
                "workspace":{"ext.gone@m/x.other":["f6"]}
            }}"#,
        )
        .expect("a keymap file");
        let loaded = load::resolve(&file).expect("a warning never stops the load");
        assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
        assert!(loaded.problems[0].message.contains("`declared`"));
        let keymap = loaded.keymap.with_extensions(Vec::new());
        let said = keymap.extension_problems();
        assert_eq!(said.len(), 1);
        assert_eq!(said[0].severity, load::Severity::Warning);
        assert!(said[0].message.contains("not installed"), "{said:?}");
    }

    #[test]
    fn an_extension_left_out_takes_its_keys_with_it() {
        let owner = "leaving@m/x";
        let keymap = default_keymap().with_extensions(vec![table(
            owner,
            vec![offered(owner, "go", "main", Some("g"))],
        )]);
        let without = keymap.with_extensions(Vec::new());
        assert_eq!(
            without.resolve_for(chord("g"), &OPEN, front(owner, "main")),
            Resolution::Fallthrough
        );
        assert!(
            without
                .extension_actions(Front {
                    owner,
                    surface: "main"
                })
                .is_empty()
        );
    }

    #[test]
    fn an_extension_action_is_never_part_of_the_vocabulary() {
        let id = intern(&action_name("listed@m/x", "go"));
        assert!(!crate::ALL_ACTIONS.contains(&Action::Extension(id)));
        assert_eq!(
            Action::parse("ext.listed@m/x.go"),
            Some(Action::Extension(id)),
            "but its name reads back as one"
        );
    }

    #[test]
    fn a_name_is_its_owner_and_its_action() {
        let name = action_name("gsd@luizfbalves/gsd-band", "refresh");
        assert_eq!(name, "ext.gsd@luizfbalves/gsd-band.refresh");
        assert!(is_name(&name));
        let id = intern(&name);
        assert_eq!(intern(&name), id, "one name, one number");
        assert_eq!(self::name(id), name);
        assert_eq!(owner(id), "gsd@luizfbalves/gsd-band");
        assert_eq!(local(id), "refresh");
    }

    #[test]
    fn what_is_not_spelled_as_one_is_not_one() {
        for bad in [
            "refresh",
            "ext.refresh",
            "ext.gsd/gsd-band.refresh",
            "ext.gsd@luizfbalves.refresh",
            "ext.gsd@luizfbalves/gsd-band.",
            "ext.@luizfbalves/gsd-band.refresh",
            "ext.gsd@luizfbalves/gsd band.refresh",
        ] {
            assert!(!is_name(bad), "{bad}");
        }
        assert!(!is_name(&action_name("p@m/x", &"a".repeat(NAME_MAX))));
    }

    #[test]
    fn the_words_are_the_manifests_once_it_said_them() {
        let id = intern(&action_name("words@m/x", "archive"));
        assert_eq!(label(id), "archive", "its id, before a manifest names it");
        assert!(!destructive(id));
        describe(
            id,
            Described {
                label: "Archive".to_owned(),
                description: "Archive the selected phase".to_owned(),
                destructive: true,
            },
        );
        assert_eq!(label(id), "Archive");
        assert_eq!(description(id), "Archive the selected phase");
        assert!(destructive(id));
    }
}
