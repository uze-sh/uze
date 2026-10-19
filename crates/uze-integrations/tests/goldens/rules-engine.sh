#!/bin/sh
# rules/engine — generated; do not edit, it is written again.
#
# Hands a coding agent the rules its project keeps in .agents/rules/ of the
# checkout it is working in, and those its .agents/rules.json registers, at
# the moment each one applies. The harness
# runs this as a command hook with its hook payload on stdin; the answer is
#   {"hookSpecificOutput":{"hookEventName":<event>,"additionalContext":<text>}}
# on stdout, or nothing at all. It only ever adds context: it never denies,
# never alters a tool's input, and every failure (no jq, no git, a rule it
# cannot read, a payload it does not know) exits 0 having said nothing.
#
#   usage: engine <sessions-dir>
#     SessionStart  every always_on rule's body, and one line per
#                   model_decision rule naming its file and description
#     PostToolUse   every glob rule matching a path the tool touched, once
#                   per session; a compact or clear start forgets them
#
# Each rule arrives as <rule path="…" trigger="…" matched="…">body</rule>.
sessions=${1:-}
LC_ALL=C
export LC_ALL

# The same dependency, and the same override, as the portable hook wrapper.
JQ=${HOOK_JQ:-jq}
command -v "$JQ" >/dev/null 2>&1 || exit 0
command -v git >/dev/null 2>&1 || exit 0

CANDIDATES='
def header: startswith("*** Add File: ") or startswith("*** Update File: ")
  or startswith("*** Delete File: ") or startswith("*** Move to: ");
def unheader: ltrimstr("*** Add File: ") | ltrimstr("*** Update File: ")
  | ltrimstr("*** Delete File: ") | ltrimstr("*** Move to: ");
(.tool_input // {}) as $input
| (if ($input | type) == "string" then [[]] else [$input | paths(type == "string")] end)[]
| . as $path
| ($input | getpath($path) | split("\n")) as $lines
| ($lines | map(select(header))) as $headers
| if ($headers | length) > 0 then $headers[] | "x:" + unheader
  elif ($path | length) == 0 or any($path[]; . == "command" or . == "cmd") then $lines[] | "s:" + .
  elif ($lines | length) == 1 then "e:" + $lines[0]
  else empty end
'

NORMALIZE='
function relative(p,   n, parts, kept, k, i, s, r) {
  if (substr(p, 1, 1) == "/") {
    if (index(p, root "/") == 1) p = substr(p, length(root) + 2)
    else if (cwd != "" && index(p, cwd "/") == 1) p = base "/" substr(p, length(cwd) + 2)
    else return ""
  } else p = base "/" p
  n = split(p, parts, "/"); k = 0
  for (i = 1; i <= n; i++) {
    s = parts[i]
    if (s == "" || s == ".") continue
    if (s == "..") { if (k == 0) return ""; k--; continue }
    kept[++k] = s
  }
  r = ""
  for (i = 1; i <= k; i++) r = r (i > 1 ? "/" : "") kept[i]
  return r
}
function emit(kind, p,   q) {
  if (p == "" || length(p) > 4096) return
  q = relative(p)
  if (q == "" || (kind ":" q) in seen) return
  seen[kind ":" q] = 1
  print kind ":" q
}
BEGIN {
  root = ENVIRON["ROOT"]; base = ENVIRON["BASE"]; cwd = ENVIRON["CWD"]
  sub(/\/+$/, "", cwd)
  separators = "[ \t\r\047\"`;|&<>()=]"
}
/^x:/ { emit("x", substr($0, 3)); next }
/^e:/ { v = substr($0, 3); sub(/^[ \t]+/, "", v); sub(/[ \t\r]+$/, "", v); emit("e", v); next }
/^s:/ {
  line = substr($0, 3); gsub(separators, " ", line)
  n = split(line, words, " ")
  for (i = 1; i <= n; i++) emit("e", words[i])
}
'

# One line per thing an entry asks for, every path relative to the checkout
# root: `x:` a file it excludes, then `f:` each file it includes only, or
# `e:` the directory or file it names, then `-` where the entry ends. An
# entry whose path leaves the checkout, or is no clean string, says nothing.
MANIFEST='
def clean: type == "string" and length > 0
  and (explode | any(. < 32 or (. >= 127 and . < 160)) | not);
def strings: if type == "array" then map(select(clean)) else [] end;
def within: (if startswith("/") then
    (if . == $root then "" elif startswith($root + "/") then .[($root | length) + 1:] else null end)
  else . end)
  | if . == null then null else
      reduce split("/")[] as $s ([];
        if . == null then null
        elif $s == "" or $s == "." then .
        elif $s == ".." then (if length == 0 then null else .[:-1] end)
        else . + [$s] end)
      | if . == null then null else join("/") end
    end;
def under($t): if $t == "" then . else $t + "/" + . end;
if (.entries | type) == "array" then .entries[] else empty end
| select(type == "object" and (.path | clean))
| (.path | within) as $t
| select($t != null)
| ((.exclude | strings)[] | under($t) | within | select(. != null and . != "") | "x:" + .),
  (if (.include_only | type) == "array"
   then (.include_only | strings)[] | under($t) | within | select(. != null and . != "") | "f:" + .
   else "e:" + $t end),
  "-"
'

RULES='
function trim(s) { sub(/^[ \t\r]+/, "", s); sub(/[ \t\r]+$/, "", s); return s }
# A frontmatter value as the Rust rule model reads it: trimmed, an unquoted
# ` #` comment cut, one pair of quotes stripped.
function scalar(s,   q, at) {
  s = trim(s); q = substr(s, 1, 1)
  if (q == "\"" || q == "\047") {
    at = index(substr(s, 2), q)
    if (at > 0) return substr(s, 2, at - 1)
    return s
  }
  at = index(s, " #"); if (at > 0) s = substr(s, 1, at - 1)
  at = index(s, "\t#"); if (at > 0) s = substr(s, 1, at - 1)
  return trim(s)
}
function attr(s) {
  gsub(/&/, "\\&amp;", s); gsub(/"/, "\\&quot;", s); gsub(/</, "\\&lt;", s); gsub(/>/, "\\&gt;", s)
  return s
}
# The dialect is what Antigravity honours natively: a pattern with no `/`
# matches a file name at any depth (`*.rs`, `Makefile`); any other starts
# with `**/` (`**/src/ui/**`), whose leading `**` may also be no segment at
# all. Within it, `**` (whole segments), `*` and `?` (within one segment),
# `[...]` classes and literal characters. Anything else is outside it, and
# the rule is not delivered.
function ere(p,   n, segs, i, s, re, sep, j, c, t, end, class, k) {
  if (p == "" || p ~ /[{}\\!()|]/ || p ~ /[\001-\037\177]/) return ""
  if (p == "**") return "^.*$"
  if (index(p, "/") == 0) p = "**/" p
  else if (substr(p, 1, 3) != "**/") return ""
  n = split(p, segs, "/"); re = ""; sep = 0
  for (i = 1; i <= n; i++) {
    s = segs[i]
    if (s == "" || s == "." || s == "..") return ""
    if (s == "**") {
      if (i < n && segs[i + 1] == "**") continue
      if (n == 1) re = ".*"
      else if (i == 1) re = "(.*/)?"
      else re = re "(/.*)?"
      continue
    }
    if (index(s, "**")) return ""
    t = ""
    for (j = 1; j <= length(s); j++) {
      c = substr(s, j, 1)
      if (c == "*") t = t "[^/]*"
      else if (c == "?") t = t "[^/]"
      else if (c == "[") {
        # A set or range of one character. Empty, nested or negated sets
        # are refused, as the Rust model refuses them: readers disagree on
        # what `[!a]` and `[^a]` mean.
        if (substr(s, j + 1, 1) == "]") return ""
        end = index(substr(s, j + 2), "]")
        if (end == 0) return ""
        class = substr(s, j + 1, end)
        j += end + 1
        if (substr(class, 1, 1) == "^" || index(class, "[")) return ""
        for (k = 1; k + 2 <= length(class); k++)
          if (substr(class, k + 1, 1) == "-" && substr(class, k, 1) > substr(class, k + 2, 1)) return ""
        t = t "[" class "]"
      }
      else if (c == "]") return ""
      else if (c == "^") t = t "\\^"
      else if (index(".+()|$", c)) t = t "[" c "]"
      else t = t c
    }
    re = re (sep ? "/" : "") t; sep = 1
  }
  return "^" re "$"
}
function block(id, trigger, matched, text) {
  return "<rule path=\"" attr(id) "\" trigger=\"" trigger "\"" \
    (matched == "" ? "" : " matched=\"" attr(matched) "\"") ">\n" text "\n</rule>"
}
function finish(   id, n, pats, m, res, i, p, re, hit, k) {
  if (file == "" || state != 2 || broken) { file = ""; return }
  id = substr(file, length(root) + 2); file = ""
  sub(/^\n+/, "", body); sub(/\n+$/, "", body)
  if (trigger == "always_on") {
    if (event == "SessionStart" && body != "") always[++na] = block(id, "always_on", "", body)
  } else if (trigger == "model_decision") {
    if (event == "SessionStart" && description != "")
      listed[++nl] = "<rule path=\"" attr(id) "\" trigger=\"model_decision\">" description "</rule>"
  } else if (trigger == "glob") {
    n = split(globs, pats, ","); m = 0
    for (i = 1; i <= n; i++) {
      p = trim(pats[i])
      if (p == "") return
      re = ere(p)
      if (re == "") return
      res[++m] = re
    }
    if (m == 0 || event != "PostToolUse" || body == "" || (id in delivered)) return
    hit = ""
    for (k = 1; k <= np && hit == ""; k++) {
      if (paths[k] == "") continue
      for (i = 1; i <= m; i++) if (paths[k] ~ res[i]) { hit = paths[k]; break }
    }
    if (hit == "") return
    globbed[++ng] = block(id, "glob", hit, body)
    delivered[id] = 1
    if (ledger != "") print id >> ledger
  }
}
BEGIN {
  root = ENVIRON["ROOT"]; event = ENVIRON["EVENT"]; ledger = ENVIRON["LEDGER"]
  np = split(ENVIRON["PATHS"], paths, "\n")
  if (ledger != "") { while ((getline seen < ledger) > 0) delivered[seen] = 1; close(ledger) }
}
FNR == 1 {
  finish()
  file = FILENAME; state = 0; trigger = ""; globs = ""; description = ""; body = ""
  broken = 0; open_key = 0; split("", counted)
}
{ line = $0; sub(/\r$/, "", line) }
FNR == 1 { sub("^\357\273\277", "", line) }
state == 0 { state = (line == "---") ? 1 : 3; next }
# Only top-level `trigger`, `globs` and `description` lines count. One of
# them repeated, written as a block or continued on an indented line makes
# the file no rule, exactly as the Rust model decides.
state == 1 {
  if (line == "---") { state = 2; next }
  if (trim(line) ~ /^#/) next
  if (line ~ /^[ \t]/ || line ~ /^-( |$)/) { if (open_key && trim(line) != "") broken = 1; next }
  open_key = 0
  # A line that is not `key: value` makes the file no rule.
  if (trim(line) != "" && index(line, ":") == 0) { broken = 1; next }
  if (line ~ /^[A-Za-z_][A-Za-z0-9_-]*[ \t]*:/) {
    key = line; sub(/[ \t]*:.*/, "", key)
    value = line; sub(/^[^:]*:/, "", value)
    if (key == "trigger" || key == "globs" || key == "description") {
      if (key in counted) broken = 1
      counted[key] = 1; open_key = 1
      if (trim(value) ~ /^[|>]/) broken = 1
      value = scalar(value)
      if (key == "trigger") trigger = value
      else if (key == "globs") globs = value
      else description = value
    }
  }
  next
}
state == 2 { body = body line "\n" }
END {
  finish()
  if (ledger != "") close(ledger)
  out = ""
  for (i = 1; i <= na; i++) out = out (out == "" ? "" : "\n\n") always[i]
  if (nl > 0) {
    out = out (out == "" ? "" : "\n\n") \
      "The project keeps these rules for particular tasks; read a rule'"'"'s file when its description fits the task:"
    for (i = 1; i <= nl; i++) out = out "\n" listed[i]
  }
  for (i = 1; i <= ng; i++) out = out (out == "" ? "" : "\n\n") globbed[i]
  if (out != "") printf "%s", out
}
'

payload=$(cat) || exit 0
context=$(printf '%s' "$payload" | "$JQ" -r '@sh "event=\(.hook_event_name // "" | tostring) cwd=\(.cwd // "" | tostring) session=\(.session_id // "" | tostring) source=\(.source // "" | tostring)"' 2>/dev/null) || exit 0
eval "$context"
case $event in
  SessionStart|PostToolUse) ;;
  *) exit 0 ;;
esac

# The checkout the session is in, and the two stats a checkout without rules
# costs.
[ -n "$cwd" ] && cd "$cwd" 2>/dev/null || exit 0
root=$(git rev-parse --show-toplevel 2>/dev/null) && [ -n "$root" ] || exit 0
manifest=$root/.agents/rules.json
[ -d "$root/.agents/rules" ] || [ -f "$manifest" ] || exit 0
nl='
'

# Prints `$1`, relative to the checkout root, when it is a rule file as the
# Rust model decides one: a readable file named `<something>.md`.
rule_file() {
  case $1 in ''|*"$nl"*) return 0 ;; esac
  case ${1##*/} in ?*.md) ;; *) return 0 ;; esac
  [ -f "$root/$1" ] && [ -r "$root/$1" ] && printf '%s\n' "$1"
  return 0
}

# Every rule file once, relative to the checkout root: those directly under
# .agents/rules/, and those the manifest registers. A manifest jq cannot
# read registers nothing. A function rather than the body of `$(...)`: the
# bash 3.2 that is macOS's /bin/sh ends a substitution at the first `)` of a
# case pattern.
rule_files() {
  for rule in "$root"/.agents/rules/*.md; do rule_file "${rule#"$root"/}"; done
  if [ -f "$manifest" ]; then
    entries=$("$JQ" -r --arg root "$root" "$MANIFEST" "$manifest" 2>/dev/null) || entries=
    excluded=
    printf '%s\n' "$entries" | while IFS= read -r line; do
      case $line in
        x:*) excluded=$excluded${line#x:}$nl ;;
        f:*) rule_file "${line#f:}" ;;
        e:*)
          target=${line#e:}
          if [ -d "$root/$target" ]; then
            for rule in "$root/${target:+$target/}"*.md; do
              rel=${rule#"$root"/}
              case $nl$excluded in *"$nl$rel$nl"*) ;; *) rule_file "$rel" ;; esac
            done
          else
            case $nl$excluded in *"$nl$target$nl"*) ;; *) rule_file "$target" ;; esac
          fi ;;
        -) excluded= ;;
      esac
    done
  fi
}
found=$(rule_files)

# The touched paths that count, read from stdin: each `x:` line as it is,
# each `e:` line only while it names something in the checkout. A function
# for the same reason as `rule_files`.
touched_paths() {
  while IFS= read -r line; do
    case $line in
      x:*) printf '%s\n' "${line#x:}" ;;
      e:*) if [ -e "$root/${line#e:}" ]; then printf '%s\n' "${line#e:}"; fi ;;
    esac
  done
}
set --
while IFS= read -r rel; do
  [ -n "$rel" ] && set -- "$@" "$root/$rel"
done <<RULE_FILES
$(printf '%s\n' "$found" | sort -u)
RULE_FILES
[ $# -gt 0 ] || exit 0

# What this session was already handed: generated state, so losing it costs
# at most a rule handed twice.
ledger=
case $session in
  ''|.*|*[!A-Za-z0-9._-]*) ;;
  *)
    case $sessions in
      /*) (umask 077; mkdir -p "$sessions") 2>/dev/null && ledger=$sessions/$session ;;
    esac ;;
esac

paths=
if [ "$event" = SessionStart ]; then
  case $source in
    compact|clear) [ -n "$ledger" ] && (: > "$ledger") 2>/dev/null ;;
  esac
else
  here=$(pwd -P) || exit 0
  case $here in
    "$root") base= ;;
    "$root"/*) base=${here#"$root"/} ;;
    *) exit 0 ;;
  esac
  candidates=$(printf '%s' "$payload" | "$JQ" -r "$CANDIDATES" 2>/dev/null) || exit 0
  [ -n "$candidates" ] || exit 0
  touched=$(printf '%s\n' "$candidates" | ROOT=$root BASE=$base CWD=$cwd awk "$NORMALIZE" 2>/dev/null) || exit 0
  paths=$(printf '%s\n' "$touched" | touched_paths)
  [ -n "$paths" ] || exit 0
fi

text=$(ROOT=$root EVENT=$event PATHS=$paths LEDGER=$ledger awk "$RULES" "$@" 2>/dev/null) || exit 0
[ -n "$text" ] || exit 0
"$JQ" -nc --arg event "$event" --arg text "$text" \
  '{hookSpecificOutput: {hookEventName: $event, additionalContext: $text}}' 2>/dev/null
exit 0
