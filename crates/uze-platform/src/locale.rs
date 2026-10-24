//! The language the person reads in, as the two- or three-letter code a
//! table of labels is keyed by (`pt`, `en`): never the region, the
//! encoding or the variant, which no label UZE draws depends on.

/// What is drawn in when the system says nothing usable.
pub const FALLBACK: &str = "en";

/// The person's language. [`FALLBACK`] when the system names none, or
/// names one this cannot read.
///
/// On Unix it is the first of `LC_ALL`, `LC_MESSAGES` and `LANG` that is
/// set and not empty, the order the C library resolves messages in. On
/// Windows it is the user's locale (`Control Panel\International`'s
/// `LocaleName`, what Settings → Time & language writes), read from the
/// registry rather than through the globalisation API this crate does not
/// compile in.
pub fn language() -> String {
    imp::tag()
        .as_deref()
        .and_then(language_of)
        .unwrap_or_else(|| FALLBACK.to_owned())
}

/// The language a locale tag names: `pt_BR.UTF-8`, `pt-BR` and
/// `pt@euro` are all `pt`. The C and POSIX locales are the untranslated
/// one, English. `None` for a tag that names no language.
pub fn language_of(tag: &str) -> Option<String> {
    let tag = tag.trim();
    if tag == "C" || tag.starts_with("C.") || tag == "POSIX" {
        return Some(FALLBACK.to_owned());
    }
    let language = tag.split(['_', '-', '.', '@']).next().unwrap_or_default();
    let readable = (2..=3).contains(&language.len())
        && language.chars().all(|letter| letter.is_ascii_alphabetic());
    readable.then(|| language.to_ascii_lowercase())
}

/// The first tag among `names` that `lookup` answers with text, in order.
#[cfg_attr(windows, allow(dead_code))]
fn first_set(names: &[&str], lookup: impl Fn(&str) -> Option<String>) -> Option<String> {
    names
        .iter()
        .filter_map(|name| lookup(name))
        .find(|value| !value.trim().is_empty())
}

#[cfg(unix)]
mod imp {
    /// The variables the C library reads a message language from, most
    /// specific first.
    const VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

    pub(super) fn tag() -> Option<String> {
        super::first_set(&VARIABLES, |name| std::env::var(name).ok())
    }
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

    use crate::win::registry_string;

    pub(super) fn tag() -> Option<String> {
        registry_string(
            HKEY_CURRENT_USER,
            r"Control Panel\International",
            "LocaleName",
            0,
        )
        .ok()
        .flatten()?
        .into_string()
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_reduces_to_its_language() {
        for (tag, language) in [
            ("pt_BR.UTF-8", "pt"),
            ("pt_BR", "pt"),
            ("pt-BR", "pt"),
            ("de_DE@euro", "de"),
            ("en_US.UTF-8", "en"),
            ("fr", "fr"),
            ("FR_ca", "fr"),
            ("fil_PH", "fil"),
            ("C", "en"),
            ("C.UTF-8", "en"),
            ("POSIX", "en"),
        ] {
            assert_eq!(language_of(tag).as_deref(), Some(language), "{tag}");
        }
    }

    #[test]
    fn a_tag_naming_no_language_is_none() {
        for tag in ["", "  ", "x", "english", "p1_BR", ".UTF-8"] {
            assert_eq!(language_of(tag), None, "{tag:?}");
        }
    }

    #[test]
    fn the_first_variable_set_wins_and_an_empty_one_is_passed_over() {
        let set = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };
        let names = ["LC_ALL", "LC_MESSAGES", "LANG"];
        assert_eq!(
            first_set(&names, set(&[("LANG", "pt_BR.UTF-8"), ("LC_ALL", "de_DE")])).as_deref(),
            Some("de_DE")
        );
        assert_eq!(
            first_set(&names, set(&[("LANG", "pt_BR.UTF-8"), ("LC_ALL", "")])).as_deref(),
            Some("pt_BR.UTF-8")
        );
        assert_eq!(
            first_set(&names, set(&[("LANG", "en"), ("LC_MESSAGES", "fr_FR")])).as_deref(),
            Some("fr_FR")
        );
        assert_eq!(first_set(&names, set(&[])), None);
    }

    #[test]
    fn the_language_is_always_a_code() {
        let language = language();
        assert!(
            (2..=3).contains(&language.len()),
            "{language:?} is not a language code"
        );
    }
}
