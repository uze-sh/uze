//! The Windows PowerShell wrapper: what Windows runs at hook time.

use super::*;

/// The wrapper template for Windows PowerShell 5.1.
pub(crate) struct PowerShellWrapper;

impl WrapperTemplate for PowerShellWrapper {
    const RELATIVE_PATH: &'static str = "hooks/exec.ps1";
    const HEADER: &'static str = HEADER;

    /// The Windows twin of the POSIX wrapper: the same contract, compiled into a
    /// script Windows PowerShell 5.1 runs with nothing else installed. The
    /// payload is parsed by .NET's own JSON reader (no `jq`, and none of
    /// `ConvertFrom-Json`'s size limit); each handler runs as its own
    /// PowerShell process under its deadline, and what it started is ended
    /// with it.
    fn source(target: HookTarget) -> Option<String> {
        let dialect = target.dialect()?;
        let Decisions {
            deny: deny_document,
            allow: allow_document,
        } = dialect.powershell?;
        let deny_exit = dialect.deny_exit;
        let harness = target.key();
        let deny_exit_code = uze_core::hook::DENY_EXIT_CODE;
        let reason_limit = HANDLER_REASON_LIMIT;
        let tool = powershell_filter(dialect.payload.tool, "$payload");
        let cwd = powershell_filter(dialect.payload.cwd, "$payload");
        let input = powershell_filter(dialect.payload.input, "$payload");
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

function Deny-Native([string]$reason) {{
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

# --- the harness's payload becomes the hook context ----------------------
function Pick($value, [object[]]$path) {{
  foreach ($step in $path) {{
    if ($null -eq $value) {{ return $null }}
    if ($step -is [int]) {{
      if ($value -is [System.Collections.IList] -and $step -lt $value.Count) {{ $value = $value[$step] }} else {{ return $null }}
    }} elseif ($value -is [System.Collections.IDictionary] -and $value.Contains($step)) {{
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
  [System.IO.File]::WriteAllText($script, "`$ErrorActionPreference = 'Stop'`n$handler`nif (`$LASTEXITCODE) {{ exit `$LASTEXITCODE }}`n", (New-Object System.Text.UTF8Encoding $true))
  $start = New-Object System.Diagnostics.ProcessStartInfo
  $start.FileName = 'powershell.exe'
  $start.Arguments = "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`""
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
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
    {deny_exit_code} {{ if ($reason) {{ Deny-Native $reason }} else {{ Deny-Native "$handler denied the operation" }} }}
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
/// winning, `empty` nothing and `{}` an empty object.
fn powershell_filter(filter: &str, subject: &str) -> String {
    let alternatives: Vec<String> = filter
        .split("//")
        .map(str::trim)
        .map(|alternative| match alternative {
            "empty" => "$null".to_owned(),
            "{}" => "@{}".to_owned(),
            path => {
                let steps: Vec<String> = path
                    .trim_start_matches('.')
                    .split('.')
                    .flat_map(|segment| {
                        let mut parts = segment.split('[');
                        let name = parts.next().unwrap_or_default();
                        std::iter::once(format!("'{name}'"))
                            .chain(parts.map(|index| index.trim_end_matches(']').to_owned()))
                    })
                    .filter(|step| step != "''")
                    .collect();
                format!("(Pick {subject} @({}))", steps.join(", "))
            }
        })
        .collect();
    format!("(First @({}))", alternatives.join(", "))
}
