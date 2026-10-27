//! What a packaged program is, read from its bytes: a script whose first
//! line names its interpreter, or a native program of its own.
//!
//! A packaged script is run as its interpreter's pinned path followed by
//! the script, never by the kernel reading its `#!`, so `env` never runs
//! and the interpreter is a program the operator granted by name.

use uze_contract::{Envelope, Output};

/// The programs that run whatever they are handed: granting one is
/// granting any code as the operator, and the consent step says so (D6).
pub const GENERAL_RUNNERS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "fish", "env", "xargs", "python", "python3", "node", "deno",
    "bun", "ruby", "perl", "php", "lua", "npx", "uvx", "pipx", "docker", "podman", "ssh", "git",
    "make", "sudo", "doas",
];

/// Whether `program` runs whatever it is handed. `python3.12` is a
/// `python`.
pub fn is_general_runner(program: &str) -> bool {
    GENERAL_RUNNERS.contains(&program)
        || ["python", "ruby", "perl", "node", "php", "lua"]
            .iter()
            .any(|family| {
                program.strip_prefix(family).is_some_and(|version| {
                    version.chars().all(|ch| ch.is_ascii_digit() || ch == '.')
                })
            })
}

/// A program's standard output read as its declaration says: in `json`
/// mode an envelope when it is an object holding `data` and nothing but
/// envelope members, else the bare data; in `lines` mode one object per
/// non-empty line, its fields named in order, missing ones left out.
pub fn answer_of(output: &Output, stdout: &[u8]) -> Result<Envelope, String> {
    let text = std::str::from_utf8(stdout)
        .map_err(|_| "it answered something that is not text".to_owned())?;
    match output {
        // A composed tree is JSON too; the engine validates its nodes.
        Output::Json | Output::View => {
            let value: serde_json::Value = serde_json::from_str(text)
                .map_err(|error| format!("it answered something that is not JSON ({error})"))?;
            envelope_of(value)
        }
        Output::Lines { fields, separator } => {
            let items = text
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let object: serde_json::Map<String, serde_json::Value> = fields
                        .iter()
                        .zip(line.split(*separator))
                        .map(|(field, value)| {
                            (
                                field.clone(),
                                serde_json::Value::String(value.trim().to_owned()),
                            )
                        })
                        .collect();
                    serde_json::Value::Object(object)
                })
                .collect();
            Ok(Envelope::data(serde_json::Value::Array(items)))
        }
    }
}

fn envelope_of(value: serde_json::Value) -> Result<Envelope, String> {
    let is_envelope = value.as_object().is_some_and(|object| {
        object.contains_key("data")
            && object
                .keys()
                .all(|key| Envelope::MEMBERS.contains(&key.as_str()))
    });
    if !is_envelope {
        return Ok(Envelope::data(value));
    }
    let serde_json::Value::Object(mut object) = value else {
        unreachable!("checked above");
    };
    let text = |object: &mut serde_json::Map<String, serde_json::Value>, name: &str| match object
        .remove(name)
    {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("its `{name}` is not text")),
    };
    let refresh = match object.remove("refresh") {
        None | Some(serde_json::Value::Null) => Vec::new(),
        Some(serde_json::Value::Array(ids)) => ids
            .into_iter()
            .map(|id| match id {
                serde_json::Value::String(id) => Ok(id),
                _ => Err("its `refresh` is not a list of source ids".to_owned()),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("its `refresh` is not a list of source ids".to_owned()),
    };
    let toast = text(&mut object, "toast")?;
    let notice = text(&mut object, "notice")?;
    let store = match object.remove("store") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Object(patch)) => Some(Box::new(patch)),
        Some(_) => return Err("its `store` is not an object".to_owned()),
    };
    let notify = match object.remove("notify") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value::<Box<uze_contract::Notification>>(value)
                .map_err(|_| "its `notify` is not `{title, body}`".to_owned())?,
        ),
    };
    let sound = match object.remove("sound") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(cue)) => {
            Some(uze_contract::Sound::parse(&cue).ok_or_else(|| {
                format!(
                    "its `sound` is `{cue}`, not one of {}",
                    uze_contract::Sound::NAMES.join(", ")
                )
            })?)
        }
        Some(_) => return Err("its `sound` is not a cue's name".to_owned()),
    };
    Ok(Envelope {
        data: object.remove("data").unwrap_or_default(),
        refresh,
        toast,
        notice,
        store,
        notify,
        sound,
        timer: None,
    })
}

/// How many bytes of a program are read to find its `#!` line.
pub const SHEBANG_MAX: usize = 256;

/// What a packaged program's first bytes say runs it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Packaged {
    /// A script: the interpreter's name, as `exec:` grants it, and the
    /// arguments its `#!` line passes before the script.
    Script {
        interpreter: String,
        args: Vec<String>,
    },
    /// Anything else: a program of the extension's own.
    Native,
}

/// Reads `head`, the first bytes of a packaged program. `#!/bin/sh` and
/// `#!/usr/bin/env python3` both name their last word's program;
/// `env` with options (`-S`) and a `#!` line that is not one are refused,
/// since what they would run is not a name a grant can cover.
pub fn packaged(head: &[u8]) -> Result<Packaged, String> {
    let Some(rest) = head.strip_prefix(b"#!") else {
        return Ok(Packaged::Native);
    };
    let line = rest.split(|byte| *byte == b'\n').next().unwrap_or_default();
    let line = std::str::from_utf8(line)
        .map_err(|_| "its `#!` line is not text".to_owned())?
        .trim();
    let mut words = line.split_whitespace();
    let path = words
        .next()
        .ok_or_else(|| "its `#!` line names no interpreter".to_owned())?;
    let named = path.rsplit('/').next().unwrap_or(path);
    let (interpreter, args): (&str, Vec<&str>) = if named == "env" {
        let program = words
            .next()
            .ok_or_else(|| "its `#!/usr/bin/env` line names no program".to_owned())?;
        if program.starts_with('-') || program.contains('=') {
            return Err(format!(
                "its `#!` line passes `{program}` to `env`; name the interpreter alone, as `#!/usr/bin/env python3`"
            ));
        }
        (program, words.collect())
    } else {
        (named, words.collect())
    };
    if !uze_contract::is_program_name(interpreter) {
        return Err(format!(
            "its `#!` line names `{interpreter}`, which is not a program's name"
        ));
    }
    Ok(Packaged::Script {
        interpreter: interpreter.to_owned(),
        args: args.into_iter().map(str::to_owned).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_is_an_envelope_only_when_it_is_shaped_as_one() {
        let bare = answer_of(&Output::Json, br#"[{"name": "support-macos"}]"#).unwrap();
        assert_eq!(bare.data[0]["name"], "support-macos");
        let document = answer_of(&Output::Json, br#"{"data": 1, "title": "x"}"#).unwrap();
        assert_eq!(
            document.data["title"], "x",
            "a document with a `data` field is data"
        );
        let wrapped = answer_of(
            &Output::Json,
            br#"{"data": [3], "store": {"history": [3, 5]}, "toast": "ok", "refresh": ["board"]}"#,
        )
        .unwrap();
        assert_eq!(wrapped.data, serde_json::json!([3]));
        assert_eq!(wrapped.store.unwrap()["history"], serde_json::json!([3, 5]));
        assert_eq!(wrapped.toast.as_deref(), Some("ok"));
        assert_eq!(wrapped.refresh, ["board"]);
        assert!(
            answer_of(&Output::Json, b"hello")
                .unwrap_err()
                .contains("not JSON")
        );
        assert!(answer_of(&Output::Json, br#"{"data": 1, "toast": 3}"#).is_err());
    }

    #[test]
    fn lines_become_items_named_by_their_fields() {
        let output = Output::Lines {
            fields: vec!["name".into(), "status".into()],
            separator: '\t',
        };
        let answer = answer_of(&output, b"build\tok\n\ntest\n").unwrap();
        assert_eq!(
            answer.data,
            serde_json::json!([{"name": "build", "status": "ok"}, {"name": "test"}])
        );
        let csv = Output::Lines {
            fields: vec!["gpu".into(), "load".into()],
            separator: ',',
        };
        let answer = answer_of(&csv, b"NVIDIA GeForce RTX 4060 Ti, 9\n").unwrap();
        assert_eq!(answer.data[0]["load"], "9");
    }

    #[test]
    fn a_runner_is_known_by_its_family() {
        for runner in ["sh", "python3", "python3.12", "node", "git", "docker"] {
            assert!(is_general_runner(runner), "{runner}");
        }
        for program in ["nvidia-smi", "gh", "openspec", "pythonic"] {
            assert!(!is_general_runner(program), "{program}");
        }
    }

    fn script(interpreter: &str, args: &[&str]) -> Packaged {
        Packaged::Script {
            interpreter: interpreter.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }

    #[test]
    fn a_shebang_names_the_program_that_runs_the_script() {
        assert_eq!(packaged(b"#!/bin/sh\necho hi\n"), Ok(script("sh", &[])));
        assert_eq!(
            packaged(b"#!/usr/bin/env python3\n"),
            Ok(script("python3", &[]))
        );
        assert_eq!(packaged(b"#!/bin/bash -eu\n"), Ok(script("bash", &["-eu"])));
        assert_eq!(packaged(b"\x7fELF\x02\x01"), Ok(Packaged::Native));
    }

    #[test]
    fn a_shebang_that_runs_what_no_grant_names_is_refused() {
        for refused in [
            &b"#!/usr/bin/env -S python3 -u\n"[..],
            b"#!/usr/bin/env\n",
            b"#!\n",
            b"#!/usr/bin/env FOO=1 python3\n",
        ] {
            assert!(
                packaged(refused).is_err(),
                "{}",
                String::from_utf8_lossy(refused)
            );
        }
    }
}
