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

use std::{collections::BTreeMap, fs, time::Duration};

use toml_edit::{DocumentMut, Item, Table, TableLike, Value};

use crate::{
    error::{Result, UzeError},
    home::UzeHome,
    persistence::FileLock,
};

/// How long a write waits for another writer of `config.toml`. Changing
/// one key takes milliseconds; a holder past this is a stuck process.
const WRITE_WAIT: Duration = Duration::from_secs(10);
const WRITE_RETRY: Duration = Duration::from_millis(10);

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

/// The string `key` holds in each table of `section`, by the table's name:
/// `[section."name"] key = "…"`. A table without the key, or holding
/// anything else in it, is left out.
pub fn strings_by_table(
    home: &UzeHome,
    section: &str,
    key: &str,
) -> Result<BTreeMap<String, String>> {
    let document = read(home)?;
    Ok(document
        .get(section)
        .and_then(Item::as_table_like)
        .map(|tables| {
            tables
                .iter()
                .filter_map(|(name, table)| {
                    let value = table.as_table_like()?.get(key)?.as_str()?;
                    Some((name.to_owned(), value.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default())
}

/// Sets `key` in `[section."table"]`. A `section` this creates is only
/// the tables' parent, so the file gains `[section."table"]` and no bare
/// `[section]` header above it.
pub fn set_in_table(
    home: &UzeHome,
    section: &str,
    table: &str,
    key: &str,
    setting: &str,
) -> Result<()> {
    let _held = FileLock::acquire(&home.config_lock_path(), WRITE_WAIT, WRITE_RETRY)?;
    let mut document = read(home)?;
    let parent = table_like_mut(document.as_table_mut(), section, true)
        .ok_or_else(|| malformed(home, format!("`{section}` is not a table")))?;
    let target = table_like_mut(parent, table, false)
        .ok_or_else(|| malformed(home, format!("`{section}.{table}` is not a table")))?;
    replace_keeping_decor(target, key, setting.into());
    write(home, &document)
}

/// Every scalar `[section."table"]` holds, by key, as JSON: a string, a
/// number or a boolean. A date, an array or a table is left out: no
/// owner of a table read this way keeps one.
pub fn scalars_in_table(
    home: &UzeHome,
    section: &str,
    table: &str,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let document = read(home)?;
    Ok(document
        .get(section)
        .and_then(Item::as_table_like)
        .and_then(|tables| tables.get(table))
        .and_then(Item::as_table_like)
        .map(|values| {
            values
                .iter()
                .filter_map(|(key, item)| Some((key.to_owned(), scalar_of(item.as_value()?)?)))
                .collect()
        })
        .unwrap_or_default())
}

/// Sets `key` in `[section."table"]` to a JSON scalar, refusing anything
/// else: the file holds what a person can read and type back.
pub fn set_scalar_in_table(
    home: &UzeHome,
    section: &str,
    table: &str,
    key: &str,
    setting: &serde_json::Value,
) -> Result<()> {
    let setting = value_of(setting).ok_or_else(|| UzeError::Write {
        path: home.config_path(),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("`{setting}` is not a value `{key}` may hold"),
        ),
    })?;
    let _held = FileLock::acquire(&home.config_lock_path(), WRITE_WAIT, WRITE_RETRY)?;
    let mut document = read(home)?;
    let parent = table_like_mut(document.as_table_mut(), section, true)
        .ok_or_else(|| malformed(home, format!("`{section}` is not a table")))?;
    let target = table_like_mut(parent, table, false)
        .ok_or_else(|| malformed(home, format!("`{section}.{table}` is not a table")))?;
    replace_keeping_decor(target, key, setting);
    write(home, &document)
}

fn scalar_of(value: &Value) -> Option<serde_json::Value> {
    match value {
        Value::String(text) => Some(text.value().clone().into()),
        Value::Integer(number) => Some((*number.value()).into()),
        Value::Float(number) => serde_json::Number::from_f64(*number.value()).map(Into::into),
        Value::Boolean(flag) => Some((*flag.value()).into()),
        _ => None,
    }
}

fn value_of(setting: &serde_json::Value) -> Option<Value> {
    match setting {
        serde_json::Value::String(text) => Some(text.as_str().into()),
        serde_json::Value::Bool(flag) => Some((*flag).into()),
        serde_json::Value::Number(number) => match number.as_i64() {
            Some(whole) => Some(whole.into()),
            None => number.as_f64().map(Into::into),
        },
        _ => None,
    }
}

/// Takes `key` out of `[section."table"]`, and the table and the section
/// with it when that leaves them empty, so a removal leaves no trace. Whether
/// there was anything to take: when there was not, the file is not written.
pub fn remove_in_table(home: &UzeHome, section: &str, table: &str, key: &str) -> Result<bool> {
    let _held = FileLock::acquire(&home.config_lock_path(), WRITE_WAIT, WRITE_RETRY)?;
    let mut document = read(home)?;
    let Some(parent) = document.get_mut(section).and_then(Item::as_table_like_mut) else {
        return Ok(false);
    };
    let Some(target) = parent.get_mut(table).and_then(Item::as_table_like_mut) else {
        return Ok(false);
    };
    if target.remove(key).is_none() {
        return Ok(false);
    }
    if target.is_empty() {
        parent.remove(table);
    }
    if parent.is_empty() {
        document.remove(section);
    }
    write(home, &document)?;
    Ok(true)
}

fn set_value(home: &UzeHome, section: &str, key: &str, setting: Value) -> Result<()> {
    let _held = FileLock::acquire(&home.config_lock_path(), WRITE_WAIT, WRITE_RETRY)?;
    let mut document = read(home)?;
    let table = table_like_mut(document.as_table_mut(), section, false)
        .ok_or_else(|| malformed(home, format!("`{section}` is not a table")))?;
    replace_keeping_decor(table, key, setting);
    write(home, &document)
}

/// The table `name` in `parent`, made when missing; `None` when `name`
/// holds something that is not a table.
fn table_like_mut<'a>(
    parent: &'a mut dyn TableLike,
    name: &str,
    implicit: bool,
) -> Option<&'a mut dyn TableLike> {
    parent
        .entry(name)
        .or_insert_with(|| {
            let mut table = Table::new();
            table.set_implicit(implicit);
            Item::Table(table)
        })
        .as_table_like_mut()
}

fn replace_keeping_decor(table: &mut dyn TableLike, key: &str, setting: Value) {
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

    /// Each writer changes one key of the whole file, so two at once would
    /// each keep only their own: a write waits for the one in progress.
    #[test]
    fn a_write_waits_for_the_one_in_progress() {
        let home = home("config-locked");
        let held = FileLock::acquire(&home.config_lock_path(), WRITE_WAIT, WRITE_RETRY).unwrap();
        let (done, finished) = std::sync::mpsc::channel();
        let writer = {
            let home = home.clone();
            std::thread::spawn(move || {
                set(&home, "appearance", "theme", "nocturne").expect("written");
                done.send(()).unwrap();
            })
        };

        assert!(
            finished.recv_timeout(Duration::from_millis(200)).is_err(),
            "the write went ahead while another held the file"
        );
        drop(held);
        writer.join().unwrap();
        assert_eq!(
            get(&home, "appearance", "theme").unwrap().as_deref(),
            Some("nocturne")
        );
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

    // A symbolic link, which Windows lets an ordinary account make only in developer mode.
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

    #[test]
    fn a_table_of_a_section_is_written_under_its_own_header() {
        let home = home("config-nested-write");

        set_in_table(&home, "projects", "/work/uze", "label", "uze core").expect("written");

        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[projects.\"/work/uze\"]\nlabel = \"uze core\"\n"
        );
        assert_eq!(
            strings_by_table(&home, "projects", "label").expect("readable"),
            BTreeMap::from([("/work/uze".to_owned(), "uze core".to_owned())])
        );
    }

    #[test]
    fn tables_without_the_key_are_left_out() {
        let home = home("config-nested-read");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.a]\nlabel = \"A\"\n[projects.b]\nother = 1\n[projects.c]\nlabel = 3\n",
        )
        .unwrap();

        assert_eq!(
            strings_by_table(&home, "projects", "label").expect("readable"),
            BTreeMap::from([("a".to_owned(), "A".to_owned())])
        );
    }

    #[test]
    fn removing_the_last_key_leaves_no_trace() {
        let home = home("config-nested-remove");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        let mine = "# mine\n[appearance]\ntheme = \"dawn\"\n";
        fs::write(home.config_path(), mine).unwrap();
        set_in_table(&home, "projects", "/work/uze", "label", "uze core").expect("written");

        assert!(remove_in_table(&home, "projects", "/work/uze", "label").expect("removed"));

        assert_eq!(fs::read_to_string(home.config_path()).unwrap(), mine);
    }

    #[test]
    fn removing_what_is_not_there_writes_nothing() {
        let home = home("config-nested-absent");

        assert!(!remove_in_table(&home, "projects", "/work/uze", "label").expect("read"));

        assert!(!home.config_path().exists());
    }

    #[test]
    fn a_nested_write_keeps_the_rest_of_its_table() {
        let home = home("config-nested-keep");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[projects.\"/work/uze\"]\nlabel = \"old\" # mine\nfuture = true\n",
        )
        .unwrap();

        set_in_table(&home, "projects", "/work/uze", "label", "new").expect("written");
        assert!(remove_in_table(&home, "projects", "/work/uze", "label").expect("removed"));

        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[projects.\"/work/uze\"]\nfuture = true\n"
        );
    }

    #[test]
    fn scalars_round_trip_through_a_table_and_the_rest_is_left_out() {
        let home = home("config-scalars");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(
            home.config_path(),
            "[preferences.review]\nlist = [1]\nwhen = 1979-05-27\n",
        )
        .unwrap();

        for (key, value) in [
            ("flag", serde_json::json!(false)),
            ("count", serde_json::json!(3)),
            ("ratio", serde_json::json!(0.5)),
            ("unit", serde_json::json!("GiB")),
        ] {
            set_scalar_in_table(&home, "preferences", "review", key, &value).expect("written");
        }
        assert!(
            set_scalar_in_table(
                &home,
                "preferences",
                "review",
                "nested",
                &serde_json::json!({})
            )
            .is_err()
        );

        let read = scalars_in_table(&home, "preferences", "review").expect("readable");
        assert_eq!(
            serde_json::Value::Object(read),
            serde_json::json!({ "flag": false, "count": 3, "ratio": 0.5, "unit": "GiB" })
        );
        assert!(
            scalars_in_table(&home, "preferences", "other")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_nested_write_refuses_a_file_that_does_not_parse() {
        let home = home("config-nested-malformed");
        fs::create_dir_all(home.config_path().parent().unwrap()).unwrap();
        fs::write(home.config_path(), "[projects\n").unwrap();

        assert!(strings_by_table(&home, "projects", "label").is_err());
        assert!(set_in_table(&home, "projects", "/x", "label", "x").is_err());
        assert!(remove_in_table(&home, "projects", "/x", "label").is_err());
        assert_eq!(
            fs::read_to_string(home.config_path()).unwrap(),
            "[projects\n"
        );
    }
}
