//! Names the host filesystem holds as ordinary names.

/// `label` as a name this filesystem holds as an ordinary file or
/// directory name.
///
/// The identity on Unix. On Windows the characters NTFS refuses or reads as
/// syntax (`<>:"/\|?*`, and controls) become `-`, a trailing dot or space is
/// dropped, and a reserved device name gains a `_` after its stem:
/// `flow:review` written as-is would not fail, it would create an alternate
/// data stream `review` on a file named `flow`, which no harness reads.
pub fn file_name_for(label: &str) -> String {
    imp::file_name_for(label)
}

#[cfg(unix)]
mod imp {
    pub(super) fn file_name_for(label: &str) -> String {
        label.to_owned()
    }
}

#[cfg(windows)]
mod imp {
    pub(super) fn file_name_for(label: &str) -> String {
        let mut name: String = label
            .chars()
            .map(|character| match character {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
                control if control.is_control() => '-',
                other => other,
            })
            .collect();
        while name.ends_with(['.', ' ']) {
            name.pop();
        }
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit());
        if reserved {
            // After the stem, not at the end: `COM1.md_` is still `COM1`.
            let at = name.find('.').unwrap_or(name.len());
            name.insert(at, '_');
        }
        name
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn a_label_becomes_a_name_ntfs_holds() {
            assert_eq!(super::file_name_for("flow:review"), "flow-review");
            assert_eq!(super::file_name_for("con"), "con_");
            assert_eq!(super::file_name_for("COM1.md"), "COM1_.md");
            assert_eq!(super::file_name_for("trailing. "), "trailing");
        }
    }
}
