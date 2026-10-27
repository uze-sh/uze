//! What a packaged program is, read from its bytes: a script whose first
//! line names its interpreter, or a native program of its own.
//!
//! A packaged script is run as its interpreter's pinned path followed by
//! the script, never by the kernel reading its `#!`, so `env` never runs
//! and the interpreter is a program the operator granted by name.

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
