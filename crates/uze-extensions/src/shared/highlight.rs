//! Syntax highlighting, for the modes that show a file's own text.
//!
//! Its own module because the code surface's diff and editor and the
//! code blocks [`super::markdown`] renders all colour source code, and the alternative was a copy of the syntect
//! plumbing each — the shape where a fallback theme is fixed in one place
//! and left wrong in the others.
//!
//! Everything here is loaded once per process and shared: `syntect`'s
//! default syntax and theme sets are megabytes of parsed data, and
//! rebuilding them per file is what makes a highlighter feel slow.
//!
//! Colour produced here travels to the host as [`Rgb`] rather than as a
//! [`crate::view::Role`], because it comes from the syntax theme the way
//! an image's colour comes from its pixels — see [`crate::view::Rgb`].

use std::{path::Path, sync::OnceLock};

use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet, SyntaxSetBuilder},
};

use crate::view::Rgb;

/// The palette highlighting falls back to when the host names one syntect
/// does not bundle.
pub const FALLBACK_SYNTAX_THEME: &str = "base16-ocean.dark";

/// The grammars: syntect's own set and the ones `bat` adds to it
/// (TypeScript, TOML, Dockerfile, Kotlin, Swift, Zig, Nix, Terraform and
/// the rest), from `two-face`'s precompiled dump — syntect's defaults
/// alone draw a `.ts` or a `Cargo.toml` as plain text.
fn bundled() -> &'static SyntaxSet {
    static BUNDLED: OnceLock<SyntaxSet> = OnceLock::new();
    BUNDLED.get_or_init(two_face::syntax::extra_newlines)
}

/// Grammars `two-face` ships only for onig: their upstream regexes once
/// used what `fancy-regex` cannot compile, and onig is C, which this crate
/// keeps out of the tree. Each is the upstream file with its licence
/// beside it in `grammars/`.
///
/// A set of their own rather than added to [`bundled`]: adding one
/// relinks every grammar in the dump, half a second before the first line
/// is coloured, where the dump loads in milliseconds and this set in
/// under a tenth of a second, only once a lookup gets past the dump.
fn vendored() -> &'static SyntaxSet {
    static VENDORED: OnceLock<SyntaxSet> = OnceLock::new();
    VENDORED.get_or_init(|| {
        let mut builder = SyntaxSetBuilder::new();
        for definition in vendored_definitions() {
            builder.add(definition);
        }
        builder.build()
    })
}

const VENDORED_SOURCES: &[&str] = &[include_str!("../../grammars/PowerShell.sublime-syntax")];

fn vendored_definitions() -> impl Iterator<Item = SyntaxDefinition> {
    VENDORED_SOURCES.iter().map(|source| {
        SyntaxDefinition::load_from_str(source, true, None)
            .expect("a vendored grammar is compiled in and parsed by a test")
    })
}

/// A grammar and the set it was found in, which is the set every line it
/// highlights must be parsed against.
#[derive(Clone, Copy)]
struct Grammar {
    set: &'static SyntaxSet,
    syntax: &'static SyntaxReference,
}

/// The first answer `lookup` gives, asking the bundled set before the
/// vendored one, which only fills what the bundled set lacks.
fn find(
    lookup: impl Fn(&'static SyntaxSet) -> Option<&'static SyntaxReference>,
) -> Option<Grammar> {
    [bundled(), vendored()].into_iter().find_map(|set| {
        Some(Grammar {
            set,
            syntax: lookup(set)?,
        })
    })
}

/// The grammar [`highlighter`] reads `path` with.
#[cfg(test)]
pub(crate) fn syntax_for(path: &Path, first_line: Option<&str>) -> &'static SyntaxReference {
    grammar_for(path, first_line).syntax
}

fn plain_text() -> Grammar {
    let set = bundled();
    Grammar {
        set,
        syntax: set.find_syntax_plain_text(),
    }
}

/// One stream of lines being coloured: syntect's highlighter and the set
/// its grammar belongs to.
pub(crate) struct Highlighter {
    lines: HighlightLines<'static>,
    set: &'static SyntaxSet,
}

impl Highlighter {
    fn new(grammar: Grammar, theme_name: &str) -> Self {
        Self {
            lines: HighlightLines::new(grammar.syntax, theme(theme_name)),
            set: grammar.set,
        }
    }
}

fn theme_set() -> &'static ThemeSet {
    static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// The named theme, or the fallback.
///
/// A name the host gave us that syntect does not know would otherwise be
/// a panic on the first line drawn, which is a poor way to learn about a
/// typo — fall back and keep rendering.
pub(crate) fn theme(name: &str) -> &'static Theme {
    let themes = &theme_set().themes;
    themes
        .get(name)
        .unwrap_or_else(|| &themes[FALLBACK_SYNTAX_THEME])
}

/// A highlighter for `path`'s language, drawn from `theme_name`.
///
/// One per stream of lines, never one per line: syntect's highlighter
/// carries state across calls — an open block comment, an unterminated
/// string — and a fresh one at every line is how a doc comment stops
/// being one halfway down a file.
///
/// `first_line` is the file's own, when there is one to hand — see
/// [`grammar_for`].
pub(crate) fn highlighter(path: &Path, first_line: Option<&str>, theme_name: &str) -> Highlighter {
    Highlighter::new(grammar_for(path, first_line), theme_name)
}

/// The grammar for `path`: by its whole name, then as a lockfile, then by
/// its extension, then by `first_line`, where a shebang or a modeline says what an unnamed
/// script is. The name comes first because the grammars list some files
/// by it — `Makefile`, `Dockerfile`, `.bashrc` — and one of them,
/// `CMakeLists.txt`, has an extension that would find plain text.
fn grammar_for(path: &Path, first_line: Option<&str>) -> Grammar {
    let by_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(|extension| {
            preferred_for(extension)
                .and_then(|name| find(|set| set.find_syntax_by_name(name)))
                .or_else(|| find(|set| set.find_syntax_by_extension(extension)))
                .or_else(|| {
                    let same = same_language_as(extension)?;
                    find(|set| set.find_syntax_by_extension(same))
                })
        });
    let by_name = || {
        let name = path.file_name()?.to_str()?;
        find(|set| set.find_syntax_by_extension(name))
    };
    let by_first_line = || {
        let line = first_line?;
        find(|set| set.find_syntax_by_first_line(line))
    };
    let as_lockfile = || {
        let language = lockfile_language(path.file_name()?.to_str()?, first_line)?;
        find(|set| set.find_syntax_by_extension(language))
    };
    by_name()
        .or_else(as_lockfile)
        .or(by_extension)
        .or_else(by_first_line)
        .unwrap_or_else(plain_text)
}

/// The extension of the format a lockfile is written in.
///
/// `.lock` names no format: `Cargo.lock` is TOML, `composer.lock` JSON,
/// `yarn.lock` YAML. The package managers that write them are known by
/// name; a lockfile none of them wrote is read from its first line, where
/// JSON opens a brace, TOML a table or an assignment, and YAML a key.
fn lockfile_language(name: &str, first_line: Option<&str>) -> Option<&'static str> {
    let known = match name {
        "Cargo.lock" | "poetry.lock" | "uv.lock" | "pdm.lock" => Some("toml"),
        "composer.lock" | "flake.lock" | "Pipfile.lock" | "deno.lock" | "bun.lock" => Some("json"),
        "yarn.lock" | "pubspec.lock" | "Podfile.lock" | "agents.lock" => Some("yaml"),
        "mix.lock" => Some("ex"),
        _ => None,
    };
    if known.is_some() || !name.ends_with(".lock") {
        return known;
    }
    let line = first_line?.trim();
    match line.chars().next()? {
        '{' => Some("json"),
        '[' => Some("toml"),
        _ if line.contains(" = ") => Some("toml"),
        _ if line.ends_with(':') || line.contains(": ") => Some("yaml"),
        _ => None,
    }
}

/// The grammar an extension more than one grammar claims is meant as,
/// where the one the set lists first is the rarer reading: a `.fs` in a
/// checkout is F# far more often than a GLSL fragment shader, which has
/// `.frag` of its own, and a `.h` is a C or C++ header, which the C++
/// grammar reads both of, rather than Objective-C.
fn preferred_for(extension: &str) -> Option<&'static str> {
    match extension {
        "fs" => Some("F#"),
        "h" => Some("C++"),
        _ => None,
    }
}

/// The extension whose grammar `extension` is written in, for the
/// spellings no grammar lists: module variants of JavaScript, JSON with
/// comments, markdown with components. JSX goes to the TSX grammar,
/// which is a superset of it and the only one that parses its markup.
fn same_language_as(extension: &str) -> Option<&'static str> {
    match extension {
        "mjs" | "cjs" => Some("js"),
        "jsx" => Some("tsx"),
        "jsonc" | "json5" => Some("json"),
        "mdx" => Some("md"),
        _ => None,
    }
}

/// A highlighter for a language named the way a markdown fence names it
/// — `rust`, `js`, `sh` — rather than by a file's extension.
///
/// Its own entry point because the two names genuinely differ: a fence
/// says `rust` where a file says `.rs`, and looking one up as the other
/// silently produces plain text, which is a block of code that renders
/// but is not coloured. Falls back to plain text for a language syntect
/// does not know, and for an unfenced block, which names none.
pub(crate) fn highlighter_for_language(language: &str, theme_name: &str) -> Highlighter {
    // A fence's info string carries more than the language: rustdoc's
    // `rust,ignore`, a title after a space, Pandoc's `{.python}`.
    let token = language
        .trim()
        .trim_start_matches(['{', '.'])
        .split([',', ' ', '{', '}'])
        .next()
        .unwrap_or_default();
    let token = fence_alias(token).unwrap_or(token);
    let grammar = find(|set| set.find_syntax_by_token(token))
        .or_else(|| {
            let same = same_language_as(token)?;
            find(|set| set.find_syntax_by_extension(same))
        })
        .unwrap_or_else(plain_text);
    Highlighter::new(grammar, theme_name)
}

/// The names fences use for a language no grammar is listed under:
/// a terminal session is shell, whatever the prompt.
fn fence_alias(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "shell" | "console" | "shellsession" | "terminal" => Some("sh"),
        _ => None,
    }
}

/// One line, highlighted in `highlighter`'s ongoing state — or, where
/// syntect gives up on it, the text uncoloured in `theme_name`'s own
/// foreground: an empty answer is drawn as a blank line, and a line whose
/// grammar failed still has its text.
pub(crate) fn line(
    highlighter: &mut Highlighter,
    text: &str,
    theme_name: &str,
) -> Vec<(Rgb, String)> {
    // syntect's line-oriented highlighter expects a trailing newline
    // (matches `load_defaults_newlines` above) to track multi-line
    // constructs correctly across calls.
    let newline_terminated = format!("{text}\n");
    let Ok(ranges) = highlighter
        .lines
        .highlight_line(&newline_terminated, highlighter.set)
    else {
        return plain(theme_name, text);
    };
    ranges
        .into_iter()
        .map(|(style, piece)| {
            let foreground = style.foreground;
            (
                Rgb(foreground.r, foreground.g, foreground.b),
                piece.trim_end_matches('\n').to_owned(),
            )
        })
        .collect()
}

/// `text` uncoloured: one span in the theme's own foreground, for a line
/// past the point where colouring stopped paying for itself.
pub(crate) fn plain(theme_name: &str, text: &str) -> Vec<(Rgb, String)> {
    let foreground = theme(theme_name)
        .settings
        .foreground
        .unwrap_or(syntect::highlighting::Color::WHITE);
    vec![(
        Rgb(foreground.r, foreground.g, foreground.b),
        text.to_owned(),
    )]
}

/// The first `limit` lines of `text`, highlighted as one continuous
/// stream.
///
/// Bounded because the cost is per line and the screen is not: see
/// the code surface's file requests. Taken from the front rather than from a window
/// anywhere else, because syntect's state is the reason a doc comment
/// stays one for the lines below it — there is no way to colour line
/// five hundred without having walked the four hundred and ninety-nine
/// above it.
pub(crate) fn lines(
    text: &str,
    path: &Path,
    theme_name: &str,
    limit: usize,
) -> Vec<Vec<(Rgb, String)>> {
    let mut highlighter = highlighter(path, text.lines().next(), theme_name);
    text.lines()
        .take(limit)
        .map(|text| line(&mut highlighter, text, theme_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use syntect::parsing::{Regex, syntax_definition::Pattern};

    use super::{FALLBACK_SYNTAX_THEME, highlighter, vendored_definitions};

    /// syntect compiles a pattern the first time a line reaches it, and a
    /// pattern `fancy-regex` refuses turns the rest of the file plain —
    /// so every one is compiled here, not only those a fixture reaches.
    #[test]
    fn every_vendored_pattern_compiles() {
        let refused: Vec<String> = vendored_definitions()
            .flat_map(|definition| {
                definition
                    .contexts
                    .into_iter()
                    .flat_map(|(name, context)| {
                        context.patterns.into_iter().filter_map(move |pattern| {
                            let Pattern::Match(pattern) = pattern else {
                                return None;
                            };
                            let regex = pattern.regex.regex_str().to_owned();
                            Regex::try_compile(&regex)
                                .map(|error| format!("{name}: {regex}\n  {error}"))
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(refused.is_empty(), "{}", refused.join("\n"));
    }

    /// A reference a vendored grammar makes to a grammar its own set does
    /// not hold fails the line that reaches it, and [`super::line`] hides
    /// that as plain text — so the parse is asked directly.
    #[test]
    fn every_line_of_a_vendored_language_parses() {
        let text = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/_fixtures/highlight/powershell.ps1"
        ));
        let mut highlighter = highlighter(
            Path::new("powershell.ps1"),
            text.lines().next(),
            FALLBACK_SYNTAX_THEME,
        );
        for line in text.lines() {
            highlighter
                .lines
                .highlight_line(&format!("{line}\n"), highlighter.set)
                .unwrap_or_else(|error| panic!("{line:?}: {error}"));
        }
    }
}
