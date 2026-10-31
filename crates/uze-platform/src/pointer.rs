//! The key a person holds to follow a link with the pointer in a terminal.
//!
//! A terminal reports shift, alt and ctrl with a click and nothing else, so
//! a program inside one never sees the key a Mac follows links with (cmd),
//! which its terminal keeps for its own links anyway. On a Mac either of
//! the two it can see may be the terminal's: some turn ctrl+click into a
//! secondary click, others move the prompt's cursor on option+click. Both
//! follow a link there, and whichever the terminal lets through works.
//! Elsewhere ctrl is the terminals' own convention, and alt is often the
//! window manager's or a block selection's.

/// A key that, held with a click, follows a link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkKey {
    Ctrl,
    Alt,
}

/// The keys that follow a link here, the conventional one first.
pub const LINK_KEYS: &[LinkKey] = imp::LINK_KEYS;

#[cfg(target_os = "macos")]
mod imp {
    use super::LinkKey;

    pub(super) const LINK_KEYS: &[LinkKey] = &[LinkKey::Ctrl, LinkKey::Alt];
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::LinkKey;

    pub(super) const LINK_KEYS: &[LinkKey] = &[LinkKey::Ctrl];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_follows_a_link_everywhere() {
        assert_eq!(LINK_KEYS.first(), Some(&LinkKey::Ctrl));
    }
}
