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
/// beside it in `grammars/`, except KDL, which `two-face` lacks and whose
/// community grammar needs Oniguruma and Sublime's `extends`, and Rhai,
/// which `two-face` lacks and whose community grammars are TextMate's:
/// UZE wrote those two, since an extension's views are KDL and its
/// scripts Rhai.
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

const VENDORED_SOURCES: &[&str] = &[
    include_str!("../../grammars/PowerShell.sublime-syntax"),
    include_str!("../../grammars/KDL.sublime-syntax"),
    include_str!("../../grammars/Rhai.sublime-syntax"),
];

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

    use syntect::parsing::{ParseState, ScopeStack, ScopeStackOp};

    use super::{
        FALLBACK_SYNTAX_THEME, highlighter, highlighter_for_language, line, lines, plain,
        syntax_for, vendored_definitions,
    };

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
                            // A back reference to the match that pushed
                            // the context is filled in when a line
                            // reaches it; any text stands in for it here.
                            let regex = if pattern.has_captures {
                                (1..=9).fold(regex, |regex, group| {
                                    regex.replace(&format!("\\{group}"), "#")
                                })
                            } else {
                                regex
                            };
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

    const KDL_SAMPLE: &str = r###"lookup "tone" default="muted" {
    done "success"
}
Stack gap=1 {
    Text "GSD · {version}" role="muted" grow=#true
    Segment weight=19 ratio=0.89
    /- Button "hidden" action="x"
    Log lines=#"raw {x}"#
    Badge (u8)3
    Text """
        multi
        """
}
"###;

    /// The scopes syntect gives each piece of `text` read as `path`, one
    /// `(scope stack, piece)` per piece, the innermost scope last.
    fn scoped(path: &str, text: &str) -> Vec<(String, String)> {
        let grammar = super::grammar_for(Path::new(path), text.lines().next());
        let mut state = ParseState::new(grammar.syntax);
        let mut stack = ScopeStack::new();
        let mut pieces = Vec::new();
        for line in text.lines() {
            let line = format!("{line}\n");
            let ops = state
                .parse_line(&line, grammar.set)
                .unwrap_or_else(|error| panic!("{line:?}: {error}"));
            let mut start = 0;
            for (end, op) in ops.into_iter().chain([(line.len(), ScopeStackOp::Noop)]) {
                if end > start {
                    let scopes = stack
                        .as_slice()
                        .iter()
                        .map(|scope| scope.build_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    pieces.push((scopes, line[start..end].to_owned()));
                    start = end;
                }
                stack.apply(&op).expect("the grammar's scope ops balance");
            }
        }
        pieces
    }

    /// The scopes of the piece that is `needle`, or else of the first that
    /// holds it.
    fn scope_of<'a>(pieces: &'a [(String, String)], needle: &str) -> &'a str {
        pieces
            .iter()
            .find(|(_, piece)| piece == needle)
            .or_else(|| pieces.iter().find(|(_, piece)| piece.contains(needle)))
            .map(|(scopes, _)| scopes.as_str())
            .unwrap_or_else(|| panic!("{needle:?} is not one piece of {pieces:#?}"))
    }

    #[test]
    fn a_kdl_file_resolves_to_the_vendored_kdl_grammar() {
        for path in ["board.kdl", "extension.kdl", "board.test.kdl"] {
            assert_eq!(syntax_for(Path::new(path), None).name, "KDL", "{path}");
        }
    }

    #[test]
    fn a_kdl_fence_is_read_with_the_kdl_grammar() {
        let mut highlighter = highlighter_for_language("kdl", FALLBACK_SYNTAX_THEME);
        let spans = line(
            &mut highlighter,
            "Text \"hi\" grow=#true",
            FALLBACK_SYNTAX_THEME,
        );
        assert!(spans.len() > 1, "a kdl fence is coloured: {spans:?}");
    }

    #[test]
    fn a_kdl_v2_document_is_scoped_piece_by_piece() {
        let pieces = scoped("board.kdl", KDL_SAMPLE);
        let expectations = [
            ("lookup", "entity.name.tag"),
            ("Stack", "entity.name.tag"),
            ("done", "entity.name.tag"),
            ("default", "entity.other.attribute-name"),
            ("success", "string.quoted.double"),
            ("GSD", "string.quoted.double"),
            ("version", "variable.other"),
            ("#true", "constant.language"),
            ("19", "constant.numeric"),
            ("0.89", "constant.numeric"),
            ("Button", "comment.block.slashdash"),
            ("raw ", "string.quoted.double.raw"),
            ("u8", "storage.type"),
            ("3", "constant.numeric"),
            ("multi", "string.quoted.triple"),
        ];
        for (needle, scope) in expectations {
            let scopes = scope_of(&pieces, needle);
            assert!(
                scopes.contains(scope),
                "{needle:?} is {scopes:?}, not {scope}"
            );
        }
        assert!(
            !scope_of(&pieces, "Log").contains("comment"),
            "a slashdash comments out one node, not the ones after it"
        );
    }

    #[test]
    fn kdl_v2_comments_numbers_and_raw_strings_of_every_shape_are_scoped() {
        let pieces = scoped(
            "edge.kdl",
            concat!(
                "/* outer /* inner */ still */ node 0xFF_ff 0o17 0b1_0 #-inf\n",
                "node2 r=##\"has \"# inside\"## key=#nan /-{\n",
                "    gone 1\n",
                "} kept // tail\n",
                "after #\"\"\"\n",
                "raw lines\n",
                "\"\"\"#\n",
            ),
        );
        let expectations = [
            ("still", "comment.block"),
            ("node", "entity.name.tag"),
            ("0xFF_ff", "constant.numeric"),
            ("0o17", "constant.numeric"),
            ("0b1_0", "constant.numeric"),
            ("#-inf", "constant.language"),
            ("#nan", "constant.language"),
            ("inside", "string.quoted.double.raw"),
            ("gone", "comment.block.slashdash"),
            ("kept", "string.unquoted"),
            ("tail", "comment.line"),
            ("after", "entity.name.tag"),
            ("raw lines", "string.quoted.triple.raw"),
        ];
        for (needle, scope) in expectations {
            let scopes = scope_of(&pieces, needle);
            assert!(
                scopes.contains(scope),
                "{needle:?} is {scopes:?}, not {scope}"
            );
        }
    }

    /// Scopes the theme has no colour for would still draw as plain text,
    /// so each kind of piece is checked against the colour it is drawn in.
    #[test]
    fn a_kdl_v2_document_is_coloured_apart_from_plain_text() {
        let coloured = lines(
            KDL_SAMPLE,
            Path::new("board.kdl"),
            FALLBACK_SYNTAX_THEME,
            100,
        );
        let colour_of = |needle: &str| {
            coloured
                .iter()
                .flatten()
                .find(|(_, piece)| piece.contains(needle))
                .map(|(colour, _)| *colour)
                .unwrap_or_else(|| panic!("{needle:?} was not drawn"))
        };
        let foreground = plain(FALLBACK_SYNTAX_THEME, "")[0].0;
        let kinds = [
            ("node name", "Stack"),
            ("string", "GSD"),
            ("keyword", "#true"),
            ("number", "19"),
            ("raw string", "raw "),
            ("multi-line string", "multi"),
            ("slashdash", "Button"),
            ("type annotation", "u8"),
        ];
        for (kind, needle) in kinds {
            assert_ne!(colour_of(needle), foreground, "a {kind} is coloured");
        }
        assert_ne!(
            colour_of("Stack"),
            colour_of("GSD"),
            "a node name is not a string"
        );
        assert_ne!(
            colour_of("Button"),
            colour_of("Text"),
            "a commented node is not a node"
        );
    }

    const RHAI_SAMPLE: &str = r###"/// The board's answer.
fn answer(input, context) {
    let state = read_checkout(".planning/STATE.md");
    const LIMIT = 0x1F_ff;
    let shown = `${state.len()} of ${ #{ n: LIMIT }.n } lines`;
    /* outer /* inner */ still */
    for line in 0..=10 { if line == () { break; } }
    let next = |x| x + 1.5e3;
    let raw = #"no \n escape"#;
    let ch = 'x';
    input?.title ?? this.title;
    #{ data: #{ lines: state.split("\n").len(), done: true }, mask: 0b1_0 }
}
"###;

    #[test]
    fn a_rhai_file_resolves_to_the_vendored_rhai_grammar() {
        for path in ["board.rhai", "state.rhai", "gsd-band/board.rhai"] {
            assert_eq!(syntax_for(Path::new(path), None).name, "Rhai", "{path}");
        }
    }

    #[test]
    fn a_rhai_fence_is_read_with_the_rhai_grammar() {
        let mut highlighter = highlighter_for_language("rhai", FALLBACK_SYNTAX_THEME);
        let spans = line(
            &mut highlighter,
            "let x = #{ a: \"hi\" };",
            FALLBACK_SYNTAX_THEME,
        );
        assert!(spans.len() > 1, "a rhai fence is coloured: {spans:?}");
    }

    #[test]
    fn a_rhai_script_is_scoped_piece_by_piece() {
        let pieces = scoped("board.rhai", RHAI_SAMPLE);
        let expectations = [
            ("The board's answer.", "comment.line.documentation"),
            ("fn", "storage.type.function"),
            ("answer", "entity.name.function"),
            ("let", "storage.type"),
            ("const", "storage.type"),
            ("read_checkout", "variable.function"),
            (".planning/STATE.md", "string.quoted.double"),
            ("0x1F_ff", "constant.numeric.integer.hexadecimal"),
            (" of ", "string.quoted.other.template"),
            ("${", "punctuation.section.interpolation.begin"),
            ("#{", "punctuation.section.mapping.begin"),
            ("n", "entity.other.attribute-name"),
            ("still", "comment.block"),
            ("for", "keyword.control"),
            ("in", "keyword.control"),
            ("..=", "keyword.operator.range"),
            ("10", "constant.numeric"),
            ("()", "constant.language.unit"),
            ("break", "keyword.control"),
            ("x", "variable.parameter"),
            ("1.5e3", "constant.numeric.decimal"),
            ("no \\n escape", "string.quoted.double.raw"),
            ("'x'", "constant.character"),
            ("?.", "punctuation.accessor"),
            ("??", "keyword.operator.null-coalescing"),
            ("this", "variable.language.this"),
            ("split", "variable.function.method"),
            ("\\n", "constant.character.escape"),
            ("data", "entity.other.attribute-name"),
            ("true", "constant.language.boolean"),
            ("0b1_0", "constant.numeric.integer.binary"),
        ];
        for (needle, scope) in expectations {
            let scopes = scope_of(&pieces, needle);
            assert!(
                scopes.contains(scope),
                "{needle:?} is {scopes:?}, not {scope}"
            );
        }
    }

    /// Inside `${ … }` the text is code again, not the string around it,
    /// and the string resumes after the `}` that closes it, however many
    /// braces the code opened.
    #[test]
    fn a_rhai_interpolation_reads_code_inside_a_template_string() {
        let pieces = scoped("board.rhai", RHAI_SAMPLE);
        let interpolated = |needle: &str| {
            pieces
                .iter()
                .find(|(scopes, piece)| piece == needle && scopes.contains("meta.interpolation"))
                .map(|(scopes, _)| scopes.as_str())
                .unwrap_or_else(|| panic!("{needle:?} is not interpolated in {pieces:#?}"))
        };
        for needle in ["state", "len", "LIMIT"] {
            assert!(
                !interpolated(needle).contains("string"),
                "{needle:?} inside ${{…}} is code"
            );
        }
        assert!(scope_of(&pieces, " lines").contains("string.quoted.other.template"));
        assert!(
            !scope_of(&pieces, "/*").contains("string"),
            "the template string ends at its back-tick"
        );
    }

    /// A `|` between two operands is an operator, and the parentheses of a
    /// call are not the unit value.
    #[test]
    fn a_rhai_bitwise_or_and_an_empty_call_are_not_mistaken() {
        let pieces = scoped("edge.rhai", "let m = a | b | c;\nlet t = now();\n");
        assert!(
            !scope_of(&pieces, "b").contains("variable.parameter"),
            "{pieces:#?}"
        );
        assert!(
            !scope_of(&pieces, "(").contains("constant.language.unit"),
            "{pieces:#?}"
        );
    }

    /// Scopes the theme has no colour for would still draw as plain text,
    /// so each kind of piece is checked against the colour it is drawn in.
    #[test]
    fn a_rhai_script_is_coloured_apart_from_plain_text() {
        let coloured = lines(
            RHAI_SAMPLE,
            Path::new("board.rhai"),
            FALLBACK_SYNTAX_THEME,
            100,
        );
        let colour_of = |needle: &str| {
            coloured
                .iter()
                .flatten()
                .find(|(_, piece)| piece.contains(needle))
                .map(|(colour, _)| *colour)
                .unwrap_or_else(|| panic!("{needle:?} was not drawn"))
        };
        let foreground = plain(FALLBACK_SYNTAX_THEME, "")[0].0;
        let kinds = [
            ("keyword", "let"),
            ("function name", "answer"),
            ("string", ".planning/STATE.md"),
            ("template string", " of "),
            ("number", "0x1F_ff"),
            ("comment", "still"),
            ("doc comment", "The board's answer."),
            ("map key", "data"),
        ];
        for (kind, needle) in kinds {
            assert_ne!(colour_of(needle), foreground, "a {kind} is coloured");
        }
        assert_ne!(
            colour_of("let"),
            colour_of(".planning/STATE.md"),
            "a keyword is not a string"
        );
        assert_ne!(
            colour_of("still"),
            colour_of("0x1F_ff"),
            "a comment is not a number"
        );
    }

    /// The scripts extensions ship are read end to end: a pattern that
    /// fails on one of their lines would turn the rest of the file plain.
    #[test]
    fn every_line_of_a_shipped_extension_script_parses_as_rhai() {
        let scripts = [
            (
                "board.rhai",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../uze-core/src/package/authoring/board.rhai"
                )),
            ),
            (
                "state.rhai",
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/_fixtures/canonical/gsd/extensions/gsd-band/state.rhai"
                )),
            ),
        ];
        for (name, text) in scripts {
            let path = Path::new(name);
            assert_eq!(syntax_for(path, text.lines().next()).name, "Rhai");
            let pieces = scoped(name, text);
            assert!(
                pieces
                    .iter()
                    .any(|(scopes, _)| scopes.contains("entity.name.function")),
                "{name} names its function"
            );
            assert!(
                pieces
                    .iter()
                    .any(|(scopes, _)| scopes.contains("meta.mapping")),
                "{name} answers a map"
            );
        }
    }
}
