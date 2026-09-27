//! The operator's own settings: one TOML file, one section per concern.
//!
//! Choices someone makes and may want to make by hand, so they live beside
//! `keys.json` and `theme-overrides.json` at the root rather than under
//! `state/`, in a format with comments.
//!
//! This module knows files and sections, never what a setting means: a
//! concern (`appearance`, `notifications`) owns its section and reads
//! and writes it through here. A write touches only the key it names and
//! leaves the rest of the document — the operator's comments, ordering and
//! whatever sections this build does not know — exactly as it was, down to
//! the comment beside the value it replaces. A section may be written as a
//! `[table]` or inline; both are TOML, and both are the operator's to pick.
//! A file linked in from a dotfiles repository stays a link: the write goes
//! through it to the file it points at.
//!
//! A file that does not parse is an error on read *and* on write: reading
//! on as if it were empty would silently undo every choice in it, and
//! writing over it would destroy the text the operator was in the middle of
//! fixing.

use std::{collections::BTreeMap, fs};

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::{
    error::{Result, UzeError},
    home::UzeHome,
};

/// The string at `section.key`, or `None` when the operator never set it.
pub fn get(home: &UzeHome, section: &str, key: &str) -> Result<Option<String>> {
    let document = read(home)?;
    Ok(document
        .get(section)
        .and_then(Item::as_table_like)
        .and_then(|table| table.get(key))
        .and_then(Item::as_str)
        .map(str::to_owned))
}

pub fn set(home: &UzeHome, section: &str, key: &str, setting: &str) -> Result<()> {
    set_value(home, section, key, setting.into())
}

/// Every boolean in `section`, by key. A key holding anything else is left
/// out rather than guessed at: the section's owner reads its absence as
/// its own default.
pub fn bools(home: &UzeHome, section: &str) -> Result<BTreeMap<String, bool>> {
    let document = read(home)?;
    Ok(document
        .get(section)
        .and_then(Item::as_table_like)
        .map(|table| {
            table
                .iter()
                .filter_map(|(key, item)| Some((key.to_owned(), item.as_bool()?)))
                .collect()
        })
        .unwrap_or_default())
}

pub fn set_bool(home: &UzeHome, section: &str, key: &str, setting: bool) -> Result<()> {
    set_value(home, section, key, setting.into())
}

fn set_value(home: &UzeHome, section: &str, key: &str, setting: Value) -> Result<()> {
    let mut document = read(home)?;
    let table = document
        .entry(section)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| malformed(home, format!("`{section}` is not a table")))?;
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = setting;
            *existing.decor_mut() = decor;
        }
        None => {
            table.insert(key, Item::Value(setting));
        }
    }
    write(home, &document)
}

fn read(home: &UzeHome) -> Result<DocumentMut> {
    let path = home.config_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => return Err(UzeError::Read { path, source }),
    };
    text.parse::<DocumentMut>()
        .map_err(|error| malformed(home, error.to_string()))
}

fn write(home: &UzeHome, document: &DocumentMut) -> Result<()> {
    home.ensure_layout()?;
    let path = home.config_path();
    crate::persistence::write_atomic_preserving(&path, document.to_string().as_bytes())
}

fn malformed(home: &UzeHome, reason: String) -> UzeError {
    UzeError::MalformedConfig {
        path: home.config_path(),
        reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(label: &str) -> UzeHome {
        UzeHome::at(uze_testkit::temp::scratch(label))
    }

    #[test]
    fn an_unwritten_setting_is_unset() {
        let home = home("config-none");
        assert_eq!(get(&home, "appearance", "theme").expect("readable"), None);
    }

    #[test]
    fn a_write_keeps_everything_it_was_not_asked_to_change() {
        let home = home("config-preserve");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        let written = "# mine\n[appearance]\ntheme = \"nocturne\" # dark\n\n[future]\nknob = 3\n";
        fs::write(home.config_path(), written).unwrap();

        set(&home, "notifications", "agent_finished", "always").expect("written");

        let text = fs::read_to_string(home.config_path()).unwrap();
        assert!(
            text.starts_with(written),
            "the operator's text moved:\n{text}"
        );
        assert_eq!(
            get(&home, "notifications", "agent_finished").expect("readable"),
            Some("always".to_owned())
        );
    }

    #[test]
    fn replacing_a_value_keeps_the_comment_beside_it() {
        let home = home("config-decor");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[appearance]\ntheme = \"nocturne\" # dark\n",
        )
        .unwrap();

        set(&home, "appearance", "theme", "dawn").expect("written");

        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[appearance]\ntheme = \"dawn\" # dark\n"
        );
    }

    #[test]
    fn an_inline_section_is_read_and_written_in_place() {
        let home = home("config-inline");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "appearance = { theme = \"nocturne\" }\n",
        )
        .unwrap();

        assert_eq!(
            get(&home, "appearance", "theme").expect("readable"),
            Some("nocturne".to_owned())
        );
        set(&home, "appearance", "glyphs", "nerd").expect("written");
        assert_eq!(
            get(&home, "appearance", "glyphs").expect("readable"),
            Some("nerd".to_owned())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_file_stays_linked() {
        let home = home("config-link");
        let dotfiles = uze_testkit::temp::scratch("config-link-dotfiles").join("config.toml");
        fs::create_dir_all(dotfiles.parent().unwrap()).unwrap();
        fs::write(&dotfiles, "").unwrap();
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&dotfiles, home.config_path()).unwrap();

        set(&home, "notifications", "agent_finished", "always").expect("written");

        assert!(
            fs::symlink_metadata(home.config_path())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&dotfiles).unwrap().contains("always"));
    }

    #[test]
    fn a_file_that_does_not_parse_is_neither_read_through_nor_written_over() {
        let home = home("config-malformed");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(home.config_path(), "[appearance\ntheme = ").unwrap();

        assert!(get(&home, "appearance", "theme").is_err());
        assert!(set(&home, "appearance", "theme", "default").is_err());
        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[appearance\ntheme = "
        );
    }
}
