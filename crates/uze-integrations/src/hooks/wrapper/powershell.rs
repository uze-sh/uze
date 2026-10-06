//! The Windows PowerShell wrapper: what Windows runs at hook time.

use super::*;

/// The Windows twin of the POSIX wrapper: the same contract, compiled into a
/// script Windows PowerShell 5.1 runs with nothing else installed. The
/// payload is parsed by .NET's own JSON reader (no `jq`, and none of
/// `ConvertFrom-Json`'s size limit); each handler runs as its own
/// PowerShell process under its deadline, and what it started is ended
/// with it.
pub(crate) struct PowerShellWrapper;

impl WrapperTemplate for PowerShellWrapper {
    const RELATIVE_PATH: &'static str = "hooks/exec.ps1";
    const HEADER: &'static str = HEADER;

    fn unfired(target: HookTarget) -> &'static [Unfired] {
        target
            .dialect()
            .and_then(|dialect| dialect.powershell)
            .map_or(&[], |decisions| decisions.unfired)
    }

    fn source(target: HookTarget) -> Option<String> {
        let dialect = target.dialect()?;
        let Decisions {
            deny: deny_document,
            allow: allow_document,
            ..
        } = dialect.powershell?;
        let deny_exit = dialect.deny_exit;
        let harness = target.key();
        let deny_exit_code = uze_core::hook::DENY_EXIT_CODE;
        let reason_limit = HANDLER_REASON_LIMIT;
        let tool = powershell_filter(dialect.payload.tool, "$payload")?;
        let cwd = powershell_filter(dialect.payload.cwd, "$payload")?;
        let input = powershell_filter(dialect.payload.input, "$payload")?;
        let field_defaults: String = wrapper_field_variables(target)
            .iter()
            .map(|name| format!("$env:{name} = ''\n"))
            .collect();
        let mut aliases = String::new();
        for (native, binding) in vocabulary(target).native_names() {
            aliases.push_str(&format!(
                "  '{native}' {{ $env:HOOK_TOOL = '{}'",
                binding.alias
            ));
            for (portable, native_field) in binding.fields {
                let variable = uze_core::hook::hook_field_variable(portable);
                aliases.push_str(&format!(
                    "; $env:{variable} = Text (Pick $toolInput @('{native_field}'))"
                ));
            }
            aliases.push_str(" }\n");
        }
        Some(format!(
            r#"{HEADER}, one per harness. The harness runs
# this; it runs the author's handlers. The handlers never see a harness
# payload and never write harness JSON: the context arrives as HOOK_*
# environment and the decision leaves as an exit code: 0 allows, while
# {deny_exit_code} denies and the reason is read from stderr. Anything else
# is a failure that follows the group's effect. Only the first {reason_limit}
# characters of a handler's stderr become the reason a harness is handed.
#
#   usage: exec.ps1 <plugin-root> <event> <effect> <seconds>:<handler>...
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding $false
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$pluginRoot = $args[0]
$hookEvent = $args[1]
$effect = $args[2]
$handlers = @($args | Select-Object -Skip 3)
$env:PLUGIN_ROOT = $pluginRoot
$env:HOOK_EVENT = $hookEvent
$env:HOOK_HARNESS = '{harness}'
Add-Type -AssemblyName System.Web.Extensions
$json = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$json.MaxJsonLength = [int]::MaxValue

# --- this harness's decision dialect ------------------------------------
function Allow-Native {{
  {allow_document}
}}

function Deny-Native([string]$reason, [string]$decision = 'deny') {{
  [Console]::Error.WriteLine($reason)
  # A session start decides nothing: a denial there is a report, and the
  # session opens as if the handler had allowed.
  if ($hookEvent -eq 'session_start') {{ Allow-Native; exit 0 }}
  $reasonJson = $json.Serialize($reason)
  {deny_document}
  exit {deny_exit}
}}

# fail-closed effects: a guard that cannot be evaluated denies.
function Test-Closed {{ $effect -in @('deny', 'ask', 'transform') }}
function Fail([string]$reason) {{
  if (Test-Closed) {{ Deny-Native $reason }}
  [Console]::Error.WriteLine($reason)
  Allow-Native
  exit 0
}}
# A fault of the wrapper itself is a failure like a handler's: any other
# exit would read to the harness as an error that lets the tool through.
trap {{ Fail "hooks/exec: $_" }}

# --- the harness's payload becomes the hook context ----------------------
function Pick($value, [object[]]$path) {{
  foreach ($step in $path) {{
    if ($null -eq $value) {{ return $null }}
    if ($step -is [int]) {{
      if ($value -is [System.Collections.IList] -and $step -lt $value.Count) {{ $value = $value[$step] }} else {{ return $null }}
    }} elseif ($value -is [System.Collections.IDictionary] -and $value.ContainsKey($step)) {{
      $value = $value[$step]
    }} else {{ return $null }}
  }}
  return ,$value
}}
function First([object[]]$candidates) {{
  foreach ($candidate in $candidates) {{ if ($null -ne $candidate) {{ return ,$candidate }} }}
  return $null
}}
function Text($value) {{
  if ($null -eq $value) {{ '' }} elseif ($value -is [string]) {{ $value }} else {{ $json.Serialize($value) }}
}}

# A payload that does not parse leaves every field empty, and a guard
# written the documented way then sees nothing and allows: it is a failure
# like any other.
try {{
  $payload = $json.DeserializeObject([Console]::In.ReadToEnd())
}} catch {{
  Fail 'hooks/exec: the harness payload is not JSON'
}}
if ($payload -isnot [System.Collections.IDictionary]) {{ Fail 'hooks/exec: the harness payload is not JSON' }}
$env:HOOK_TOOL_NATIVE = Text {tool}
$env:HOOK_CWD = Text {cwd}
$toolInput = {input}
$env:HOOK_INPUT = if ($null -eq $toolInput) {{ '{{}}' }} else {{ $json.Serialize($toolInput) }}
$env:HOOK_SOURCE = if ($hookEvent -eq 'session_start') {{ Text (Pick $payload @('source')) }} else {{ '' }}
$env:HOOK_TOOL = ''
{field_defaults}switch -CaseSensitive ($env:HOOK_TOOL_NATIVE) {{  # the portable vocabulary
{aliases}}}

# --- the handlers, in order; the first denial stops the rest --------------
# A handler is a PowerShell command line, run from the package root. Each
# runs as a process of its own under its own deadline; past it, it and
# everything it started are stopped, and the group's effect decides.
if (-not (Test-Path -LiteralPath $pluginRoot -PathType Container)) {{
  Fail "hooks/exec: the package root is gone: $pluginRoot"
}}
Set-Location -LiteralPath $pluginRoot
foreach ($entry in $handlers) {{
  $separator = $entry.IndexOf(':')
  $seconds = 0
  if ($separator -lt 1 -or -not [int]::TryParse($entry.Substring(0, $separator), [ref]$seconds)) {{
    Fail "malformed handler argument: $entry"
  }}
  $handler = $entry.Substring($separator + 1)
  # The line runs from a script file of its own, never re-quoted onto a
  # command line: what the author wrote is what runs.
  $script = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "hooks-exec-$PID-$([guid]::NewGuid().ToString('N')).ps1")
  # It writes UTF-8, as this wrapper reads it: a console's own code page
  # would garble anything past ASCII in the reason it hands back.
  [System.IO.File]::WriteAllText($script, "`$ErrorActionPreference = 'Stop'`n[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding `$false`n$handler`nif (`$LASTEXITCODE) {{ exit `$LASTEXITCODE }}`n", (New-Object System.Text.UTF8Encoding $true))
  $start = New-Object System.Diagnostics.ProcessStartInfo
  $start.FileName = 'powershell.exe'
  $start.Arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`""
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.StandardOutputEncoding = $utf8
  $start.StandardErrorEncoding = $utf8
  $start.WorkingDirectory = $pluginRoot
  try {{
    $process = [System.Diagnostics.Process]::Start($start)
  }} catch {{
    Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
    Fail "handler could not start: $handler"
  }}
  $process.StandardInput.Close()
  $errors = $process.StandardError.ReadToEndAsync()
  $null = $process.StandardOutput.ReadToEndAsync()
  if ($process.WaitForExit($seconds * 1000)) {{
    $process.WaitForExit()
    $status = $process.ExitCode
  }} else {{
    # Still running, so its pid is still its own: end it with its tree.
    & taskkill.exe /T /F /PID $process.Id 2>&1 | Out-Null
    $status = 124
  }}
  Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
  if ($status -eq 0) {{ continue }}
  $reason = ''
  if ($errors.Wait(1000)) {{ $reason = $errors.Result.Trim() }}
  if ($reason.Length -gt {reason_limit}) {{ $reason = $reason.Substring(0, {reason_limit}) }}
  switch ($status) {{
    {deny_exit_code} {{
      # A handler's own denial in an `ask` group asks the person; only a
      # failure falls back to denying.
      $decision = if ($effect -eq 'ask') {{ 'ask' }} else {{ 'deny' }}
      if ($reason) {{ Deny-Native $reason $decision }} else {{ Deny-Native "$handler denied the operation" $decision }}
    }}
    124 {{ Fail "handler timed out after ${{seconds}}s: $handler" }}
    default {{ if ($reason) {{ Fail "handler failed (exit $status): $handler — $reason" }} else {{ Fail "handler failed (exit $status): $handler" }} }}
  }}
}}
Allow-Native
exit 0
"#
        ))
    }
}

/// Opens with a byte-order mark: Windows PowerShell 5.1 reads a script
/// without one in the ANSI code page, which garbles any non-ASCII path or
/// reason the wrapper carries.
const HEADER: &str = "\u{feff}# hooks/exec.ps1 — generated from hooks.json";

/// A `jq` path filter of a dialect (`.a.b[0] // .c // empty`, `... // {}`)
/// as the PowerShell expression the Windows wrapper evaluates: each
/// alternative a [`Pick`] over the parsed payload, the first one present
/// winning, `empty` nothing and `{}` an empty object. `None` for anything
/// outside that grammar, which would otherwise be translated into a
/// PowerShell expression meaning something else.
fn powershell_filter(filter: &str, subject: &str) -> Option<String> {
    let alternatives = filter
        .split("//")
        .map(str::trim)
        .map(|alternative| match alternative {
            "empty" => Some("$null".to_owned()),
            "{}" => Some("@{}".to_owned()),
            path => Some(format!(
                "(Pick {subject} @({}))",
                path_steps(path)?.join(", ")
            )),
        })
        .collect::<Option<Vec<String>>>()?;
    Some(format!("(First @({}))", alternatives.join(", ")))
}

/// `.a.b[0]` as the steps `'a', 'b', 0`: names of word characters, indexes
/// of digits.
fn path_steps(path: &str) -> Option<Vec<String>> {
    let is_name = |name: &str| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut steps = Vec::new();
    for segment in path.strip_prefix('.')?.split('.') {
        let mut parts = segment.split('[');
        let name = parts.next()?;
        if !is_name(name) {
            return None;
        }
        steps.push(format!("'{name}'"));
        for index in parts {
            let index = index.strip_suffix(']')?;
            if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            steps.push(index.to_owned());
        }
    }
    Some(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every dialect's paths are inside the grammar the translator reads:
    /// one that was not would leave that harness with no Windows wrapper,
    /// found here rather than on a machine.
    #[test]
    fn every_dialect_s_payload_paths_translate() {
        for target in [
            crate::claude::HOOKS,
            crate::codex::HOOKS,
            crate::antigravity::HOOKS,
            crate::opencode::HOOKS,
        ] {
            let Some(dialect) = target.dialect() else {
                continue;
            };
            for path in [
                dialect.payload.tool,
                dialect.payload.cwd,
                dialect.payload.input,
            ] {
                assert!(
                    powershell_filter(path, "$payload").is_some(),
                    "{}: {path}",
                    target.key()
                );
            }
        }
    }

    #[test]
    fn a_filter_outside_the_grammar_is_refused() {
        assert_eq!(
            powershell_filter(".a.b[0] // .c // empty", "$p").as_deref(),
            Some("(First @((Pick $p @('a', 'b', 0)), (Pick $p @('c')), $null))")
        );
        for unread in [".a | length", ".a[]", "a.b", ".a.\"b c\"", ".a[-1]"] {
            assert_eq!(powershell_filter(unread, "$p"), None, "{unread}");
        }
    }
}
