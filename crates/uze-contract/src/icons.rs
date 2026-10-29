//! The icons the host draws for an extension's `Icon` node: one idea each,
//! in every glyph set the operator may choose (`[appearance] glyphs`). The
//! host keeps a glyph per set for each; an extension names the idea and
//! never the glyph, which is how a Nerd Font icon never reaches a terminal
//! without one.

/// Each icon by name, and the idea it stands for.
pub const ICONS: &[(&str, &str)] = &[
    ("clock", "Time of day, a duration, a schedule."),
    ("timer", "Time running out: a countdown, an hourglass."),
    ("calendar", "A day, a date, a sprint."),
    ("check", "Done, passing, accepted."),
    ("cross", "Failed, refused, closed."),
    ("warning", "Needs attention."),
    ("info", "A note, more to read."),
    ("error", "An error, something blocked."),
    ("bell", "A notification, an alert."),
    ("git-branch", "A branch."),
    ("commit", "A commit."),
    ("merge", "A merge."),
    ("pull-request", "A pull or merge request."),
    ("issue", "An issue, a ticket."),
    ("tag", "A tag, a label, a release."),
    ("star", "A favourite, a rating."),
    ("heart", "Liked, healthy."),
    ("user", "A person."),
    ("folder", "A directory."),
    ("file", "A file."),
    ("lock", "Locked, private, secret."),
    ("link", "A link, a reference."),
    ("download", "Pulled in, received."),
    ("upload", "Pushed out, sent."),
    ("play", "Run, start."),
    ("pause", "Paused, held."),
    ("stop", "Stopped."),
    ("refresh", "Ask again, sync."),
    ("search", "Find."),
    ("settings", "Settings, configuration."),
    ("home", "Home, the start."),
    ("terminal", "A shell, a command."),
    ("code", "Source code."),
    ("database", "A database."),
    ("server", "A server, a host."),
    ("cloud", "A remote, a service."),
    ("bug", "A bug."),
    ("rocket", "A deploy, a launch."),
    ("bolt", "Fast, power, an event."),
    ("chart", "A chart, a metric."),
    ("list", "A list."),
    ("mail", "A message, an inbox."),
    ("comment", "A comment, a reply."),
    ("eye", "Watched, visible."),
    ("package", "A package, a build."),
    ("shield", "Protected, security."),
    ("key", "A key, a credential."),
    ("cpu", "A processor, load."),
    ("gauge", "A meter, usage."),
];

/// Whether `name` is one of [`ICONS`].
pub fn is_vocabulary(name: &str) -> bool {
    ICONS.iter().any(|(icon, _)| *icon == name)
}

/// Whether `c` is a glyph an extension may not write loose: one of a Nerd
/// Font's private use areas, or of the blocks pictographs, dingbats and
/// technical symbols come from, which terminals draw from a font they may
/// not have. An extension names it with `Icon` instead, so the operator's
/// glyph set decides what is drawn. Arrows, box drawing, geometric marks
/// and typography stay text.
pub fn is_icon_glyph(c: char) -> bool {
    matches!(
        u32::from(c),
        0xE000..=0xF8FF
            | 0xF_0000..=0xF_FFFD
            | 0x10_0000..=0x10_FFFD
            | 0x2300..=0x23FF
            | 0x2600..=0x27BF
            | 0x2B00..=0x2BFF
            | 0x1_F000..=0x1_FAFF
    )
}

/// The first loose icon glyph in `text`, if it holds one.
pub fn loose_glyph(text: &str) -> Option<char> {
    text.chars().find(|c| is_icon_glyph(*c))
}

/// Why a declared icon's `nerd` glyph is not one: it must be one character
/// of a Nerd Font's private use area.
pub fn nerd_glyph_problem(glyph: &str) -> Option<&'static str> {
    let mut chars = glyph.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if matches!(u32::from(c), 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD) => {
            None
        }
        _ => Some("one character of a Nerd Font's private use area"),
    }
}

/// Why a declared icon's `unicode` glyph is not one: one character, out of
/// a Nerd Font's private use area and drawn as text rather than from the
/// emoji font, whose width terminals disagree on (the rule the host's own
/// glyphs keep, `no_bundled_glyph_is_an_emoji`).
pub fn unicode_glyph_problem(glyph: &str) -> Option<&'static str> {
    const EMOJI_DINGBATS: &[u32] = &[
        0x2702, 0x2705, 0x2708, 0x2709, 0x270A, 0x270B, 0x270C, 0x270D, 0x270F, 0x2712, 0x2714,
        0x2716, 0x271D, 0x2721, 0x2728, 0x2733, 0x2734, 0x2744, 0x2747, 0x274C, 0x274E, 0x2753,
        0x2754, 0x2755, 0x2757, 0x2763, 0x2764, 0x2795, 0x2796, 0x2797, 0x27B0, 0x27BF,
    ];
    let mut chars = glyph.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return Some("one character");
    };
    let point = u32::from(c);
    let emoji = matches!(point, 0x2600..=0x26FF | 0x2B00..=0x2BFF | 0xFE00..=0xFE0F | 0x1_F000..=0x1_FAFF)
        || EMOJI_DINGBATS.contains(&point);
    let private = matches!(point, 0xE000..=0xF8FF | 0xF_0000..=0xF_FFFD | 0x10_0000..=0x10_FFFD);
    match (c.is_control() || c.is_whitespace(), emoji, private) {
        (false, false, false) => None,
        (_, true, _) => Some("a character drawn as text, not an emoji"),
        _ => Some("one visible character outside a Nerd Font's private use area"),
    }
}

/// Why a declared icon's `ascii` glyph is not one: one or two printable
/// ASCII characters.
pub fn ascii_glyph_problem(glyph: &str) -> Option<&'static str> {
    let printable = glyph.chars().all(|c| c.is_ascii_graphic());
    match (glyph.len(), printable) {
        (1..=2, true) => None,
        _ => Some("one or two printable ASCII characters"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_names_each_idea_once() {
        for (at, (name, doc)) in ICONS.iter().enumerate() {
            assert!(!doc.is_empty(), "{name}");
            assert!(
                ICONS[at + 1..].iter().all(|(other, _)| other != name),
                "{name} twice"
            );
        }
    }

    #[test]
    fn a_nerd_glyph_and_a_pictograph_are_loose_and_text_is_not() {
        assert_eq!(loose_glyph("\u{f017} hoje"), Some('\u{f017}'));
        assert_eq!(loose_glyph("⏱ hoje"), Some('⏱'));
        assert_eq!(loose_glyph("🚀 deploy"), Some('🚀'));
        assert_eq!(loose_glyph("↑↓ move · 96h de 144h…"), None);
        assert_eq!(loose_glyph("▶ ● ○ ━━ ▁▃▅"), None);
        assert_eq!(loose_glyph("Fases é ação"), None);
    }

    #[test]
    fn a_declared_icons_glyphs_are_held_to_their_set() {
        assert_eq!(nerd_glyph_problem("\u{f017}"), None);
        assert!(nerd_glyph_problem("⏱").is_some());
        assert!(nerd_glyph_problem("\u{f017}\u{f017}").is_some());
        assert_eq!(unicode_glyph_problem("◷"), None);
        assert!(
            unicode_glyph_problem("⚠").is_some(),
            "an emoji presentation"
        );
        assert!(unicode_glyph_problem("🐕").is_some());
        assert!(unicode_glyph_problem("\u{f017}").is_some());
        assert_eq!(ascii_glyph_problem("PR"), None);
        assert!(ascii_glyph_problem("abc").is_some());
        assert!(ascii_glyph_problem("é").is_some());
    }
}
