//! What every agent-context channel writes the same way: a file only its
//! owner reads, and a handler line that prints one.

use std::{fs, io::Write as _, path::Path};

use uze_core::{Result, UzeError};

/// Writes `text` at `path`, readable by its owner alone, replacing what was
/// there.
pub(crate) fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(directory) = path.parent() {
        uze_platform::fs::create_private_dir_all(directory).map_err(UzeError::write(directory))?;
    }
    let _ = fs::remove_file(path);
    uze_platform::fs::private_file(fs::OpenOptions::new().write(true).create_new(true))
        .open(path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(UzeError::write(path))
}

/// A line this platform's shell runs to print the file at `path` and
/// nothing else. The path is the only thing in it: the text never is.
pub(crate) fn print_line(path: &Path) -> String {
    let quoted = uze_platform::shell::quote(&path.to_string_lossy());
    uze_platform::shell::spelling(
        &format!("cat {quoted}"),
        &format!("Get-Content -Raw -LiteralPath {quoted}"),
    )
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_print_line_names_the_path_and_nothing_else() {
        let line = print_line(Path::new("/home/a b/context.md"));
        assert!(line.contains("context.md"));
        assert!(uze_platform::shell::words(&line).is_some_and(|words| words.len() >= 2));
    }

    #[test]
    fn a_private_file_replaces_what_was_there() {
        let root = uze_testkit::temp::scratch("integrations-agent-context-write");
        let path = root.join("dir").join("CLAUDE.md");
        write_private(&path, "first").unwrap();
        write_private(&path, "second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        let _ = fs::remove_dir_all(root);
    }
}
