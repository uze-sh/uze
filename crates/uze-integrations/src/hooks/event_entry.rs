//! Hook entries keyed by event in a harness's config: merged, inspected and removed without touching anything foreign.

use super::*;

/// The exact entries one integration already owns for one hook entry name
/// in the receipt ledger — the "previous version" contents for idempotent
/// re-attach, and the proof of ownership for a later replacement.
pub(crate) fn previous_hook_entry_content(
    uze_home: &UzeHome,
    integration_id: &str,
    hook_entry_name: &str,
) -> Result<Vec<String>> {
    // An unreadable ledger has not said "no previous version"; answering
    // that would merge a second copy of the group beside the first.
    let ledger = uze_core::state::receipts(uze_home, None)?;
    Ok(ledger
        .into_iter()
        .filter(|receipt| {
            receipt.integration == integration_id
                && matches!(
                    &receipt.artifact,
                    uze_core::integration::ManagedArtifact::HookConfigEntry {
                        entry_name,
                        ..
                    } if entry_name == hook_entry_name
                )
        })
        .filter_map(|receipt| match receipt.artifact {
            uze_core::integration::ManagedArtifact::HookConfigEntry { expected, .. } => {
                Some(expected)
            }
            _ => None,
        })
        .collect())
}

/// The event's group array inside `{"hooks": {...}}`, creating it when
/// absent and refusing to merge into a non-array shape (a foreign schema
/// UZE must not rewrite).
pub(super) fn event_array<'a>(
    config: &'a mut serde_json::Value,
    event: HookEvent,
    config_path: &Path,
) -> std::result::Result<&'a mut Vec<serde_json::Value>, String> {
    let hooks = config.as_object_mut().ok_or_else(|| {
        format!(
            "hook config `{}` root must be an object",
            config_path.display()
        )
    })?;
    let hooks = hooks
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            format!(
                "hook config `{}` has a non-object `hooks` key; preserved",
                config_path.display()
            )
        })?;
    let event_key = hook_event_name(event);
    let array = hooks
        .entry(event_key.to_owned())
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| {
            format!(
                "hook config `{}` has a non-array `hooks.{event_key}`; preserved",
                config_path.display()
            )
        })?;
    Ok(array)
}

/// Merges one group entry into the shared config's event array. Entries
/// matching any `previous` expected content are removed first (an earlier
/// version of this same UZE group being replaced); an identical entry is
/// left untouched (idempotence). Foreign groups and ordering are preserved.
pub(crate) fn merge_event_entry(
    config_path: &Path,
    event: HookEvent,
    entry: &serde_json::Value,
    previous: &[String],
) -> Result<PathBuf> {
    let mut config = json_config::read_object(config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot merge hook entry: {reason}")))?;
    let array = event_array(&mut config, event, config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot merge hook entry: {reason}")))?;
    for expected in previous {
        if let Ok(old) = serde_json::from_str::<serde_json::Value>(expected) {
            array.retain(|candidate| candidate != &old);
        }
    }
    if !array.iter().any(|candidate| candidate == entry) {
        array.push(entry.clone());
    }
    json_config::write_object(config_path, &config)?;
    Ok(config_path.to_path_buf())
}

/// Whether the exact expected group entry is present in the shared config's
/// event array — content identity is the receipt's fingerprint.
pub(crate) fn inspect_event_entry(
    config_path: &Path,
    event: HookEvent,
    expected: &str,
) -> AttachmentInspection {
    let Ok(config) = json_config::read_object(config_path) else {
        return blocked("hook config is missing or unreadable");
    };
    let Some(entries) = config
        .get("hooks")
        .and_then(serde_json::Value::as_object)
        .and_then(|hooks| hooks.get(hook_event_name(event)))
        .and_then(serde_json::Value::as_array)
    else {
        return AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "the managed hook entry is absent".to_owned(),
        };
    };
    let Ok(expected) = serde_json::from_str::<serde_json::Value>(expected) else {
        return blocked("receipt carries an unreadable expected hook entry");
    };
    if entries.iter().any(|candidate| candidate == &expected) {
        return AttachmentInspection {
            state: AttachmentState::Matched,
            reason: "managed hook entry matches the receipt".to_owned(),
        };
    }
    // Content identity alone would read an edited entry as absent, the
    // receipt would be forgotten, and the edited entry would keep running
    // this package's handlers after `uze remove`. One that still starts the
    // wrapper on this package's root is this delivery, changed.
    let delivery = invocation_heads(&expected).next();
    if delivery.is_some_and(|delivery| {
        entries
            .iter()
            .any(|candidate| invocation_heads(candidate).any(|head| head == delivery))
    }) {
        return AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "the managed hook entry was edited after UZE wrote it".to_owned(),
        };
    }
    AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "the managed hook entry is absent".to_owned(),
    }
}

/// The wrapper and package root each handler of a group entry starts the
/// wrapper with, in either invocation form ([`HookInvocation`]).
pub(super) fn invocation_heads(
    entry: &serde_json::Value,
) -> impl Iterator<Item = (String, String)> + '_ {
    let handlers = match entry.get("hooks").and_then(serde_json::Value::as_array) {
        Some(handlers) => handlers.as_slice(),
        None => std::slice::from_ref(entry),
    };
    handlers.iter().filter_map(|handler| {
        let command = handler.get("command")?.as_str()?;
        if let Some(args) = handler.get("args").and_then(serde_json::Value::as_array) {
            return Some((command.to_owned(), args.first()?.as_str()?.to_owned()));
        }
        let mut words = shell_words(command)?.into_iter();
        Some((words.next()?, words.next()?))
    })
}

/// Splits a command line in the grammar [`shell_quote`] writes: bare
/// words, single-quoted runs and backslash-escaped characters. `None` for
/// an unterminated quote.
pub(super) fn shell_words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word: Option<String> = None;
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        match character {
            '\'' => {
                let quoted = word.get_or_insert_with(String::new);
                loop {
                    match characters.next()? {
                        '\'' => break,
                        inside => quoted.push(inside),
                    }
                }
            }
            '\\' => word
                .get_or_insert_with(String::new)
                .push(characters.next()?),
            separator if separator.is_whitespace() => words.extend(word.take()),
            other => word.get_or_insert_with(String::new).push(other),
        }
    }
    words.extend(word);
    Some(words)
}

/// Removes exactly one matching entry, then prunes empty event arrays, an
/// empty `hooks` key, and finally the file itself when it holds nothing but
/// UZE's own content. A non-matched receipt blocks removal, and foreign
/// entries never change.
pub(crate) fn remove_event_entry(
    config_path: &Path,
    event: HookEvent,
    expected: &str,
) -> Result<AttachmentInspection> {
    let inspection = inspect_event_entry(config_path, event, expected);
    if inspection.state != AttachmentState::Matched {
        return Ok(inspection);
    }
    let mut config = json_config::read_object(config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot detach hook entry: {reason}")))?;
    let array = event_array(&mut config, event, config_path)
        .map_err(|reason| UzeError::HarnessConfig(format!("cannot detach hook entry: {reason}")))?;
    let expected: serde_json::Value =
        serde_json::from_str(expected).map_err(|source| UzeError::Json {
            path: config_path.to_path_buf(),
            source,
        })?;
    if let Some(index) = array.iter().position(|candidate| candidate == &expected) {
        array.remove(index);
    }
    // Prune a now-empty event array, then an empty `hooks` key.
    if array.is_empty()
        && let Some(hooks) = config
            .get_mut("hooks")
            .and_then(serde_json::Value::as_object_mut)
    {
        hooks.remove(hook_event_name(event));
        if hooks.is_empty() {
            config
                .as_object_mut()
                .expect("root is an object")
                .remove("hooks");
        }
    }
    // A file that now holds nothing but an empty object was created by UZE
    // and is safe to remove entirely; anything else stays.
    if config.as_object().is_some_and(|root| root.is_empty()) {
        match fs::remove_file(config_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(UzeError::Write {
                    path: config_path.to_path_buf(),
                    source,
                });
            }
        }
    } else {
        json_config::write_object(config_path, &config)?;
    }
    Ok(AttachmentInspection {
        state: AttachmentState::Missing,
        reason: "managed hook entry detached".to_owned(),
    })
}

/// What the wrapper on disk is, measured against the one this build writes.
#[derive(Debug)]
pub(super) enum WrapperState {
    Current,
    /// Generated by UZE, from another build's template. The wrapper is
    /// generated tier: an upgrade that changes the template must not turn
    /// every hook receipt into drift that blocks `uze remove`, when the
    /// next attach reproduces the file anyway.
    Stale,
    Broken(AttachmentInspection),
}

/// The wrapper is the other half of every merged delivery: an entry
/// pointing at a missing, foreign or unrunnable wrapper is not a match.
pub(super) fn inspect_wrapper(target: HookTarget, path: &Path) -> WrapperState {
    let Ok(current) = fs::read_to_string(path) else {
        return WrapperState::Broken(AttachmentInspection {
            state: AttachmentState::Missing,
            reason: "the generated hook wrapper is absent".to_owned(),
        });
    };
    let generated = wrapper_source(target).is_some_and(|expected| expected == current);
    if !generated && !current.starts_with(WRAPPER_HEADER) {
        return WrapperState::Broken(AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "the generated hook wrapper does not match what UZE writes".to_owned(),
        });
    }
    if !is_executable(path) {
        return WrapperState::Broken(AttachmentInspection {
            state: AttachmentState::Drifted,
            reason: "the generated hook wrapper is not executable".to_owned(),
        });
    }
    if generated {
        WrapperState::Current
    } else {
        WrapperState::Stale
    }
}

// ============================================================================
// OpenCode bridge (generated TypeScript, no author toolchain)
// ============================================================================
