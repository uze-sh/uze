# hooks/exec.ps1 — generated from hooks.json, one per harness. The harness runs
# this; it runs the author's handlers. The handlers never see a harness
# payload and never write harness JSON: the context arrives as HOOK_*
# environment and the decision leaves as an exit code: 0 allows, while
# 3 denies and the reason is read from stderr. Anything else
# is a failure that follows the group's effect. Only the first 4096
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
$env:HOOK_HARNESS = 'antigravity'
Add-Type -AssemblyName System.Web.Extensions
$json = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$json.MaxJsonLength = [int]::MaxValue

# --- this harness's decision dialect ------------------------------------
function Allow-Native {
  if ($hookEvent -ne 'pre_tool_use') { [Console]::Out.Write('{}') }
}

function Transform-Native {                     # the rewritten input
  $updatedJson = $env:HOOK_INPUT
  [Console]::Out.Write('{"decision":"allow","overwrite":' + $updatedJson + '}')
  exit 0
}

function Deny-Native([string]$reason, [string]$decision = 'deny') {
  [Console]::Error.WriteLine($reason)
  # A session start decides nothing: a denial there is a report, and the
  # session opens as if the handler had allowed.
  if ($hookEvent -eq 'session_start') { Allow-Native; exit 0 }
  $reasonJson = $json.Serialize($reason)
  [Console]::Out.Write('{"decision":"' + $decision + '","reason":' + $reasonJson + '}')
  exit 0
}

# fail-closed effects: a guard that cannot be evaluated denies.
function Test-Closed { $effect -in @('deny', 'ask', 'transform') }
function Fail([string]$reason) {
  if (Test-Closed) { Deny-Native $reason }
  [Console]::Error.WriteLine($reason)
  Allow-Native
  exit 0
}
# A fault of the wrapper itself is a failure like a handler's: any other
# exit would read to the harness as an error that lets the tool through.
trap { Fail "hooks/exec: $_" }

# --- the harness's payload becomes the hook context ----------------------
function Pick($value, [object[]]$path) {
  foreach ($step in $path) {
    if ($null -eq $value) { return $null }
    if ($step -is [int]) {
      if ($value -is [System.Collections.IList] -and $step -lt $value.Count) { $value = $value[$step] } else { return $null }
    } elseif ($value -is [System.Collections.IDictionary] -and $value.ContainsKey($step)) {
      $value = $value[$step]
    } else { return $null }
  }
  return ,$value
}
function First([object[]]$candidates) {
  foreach ($candidate in $candidates) { if ($null -ne $candidate) { return ,$candidate } }
  return $null
}
function Text($value) {
  if ($null -eq $value) { '' } elseif ($value -is [string]) { $value } else { $json.Serialize($value) }
}
# A value is held to the contract's bound before it is exported, the same
# bound in UTF-8 bytes as on every other platform; it also keeps it inside
# the 32,767 characters Windows lets a variable hold, past which setting it
# would fail with .NET's words rather than this wrapper's.
function Set-Hook([string]$name, [string]$value) {
  if ($utf8.GetByteCount($value) -gt 32000) {
    Fail "hooks/exec: the tool input is larger than the 32000 bytes a HOOK_* variable carries"
  }
  [Environment]::SetEnvironmentVariable($name, $value)
}

# A payload that does not parse leaves every field empty, and a guard
# written the documented way then sees nothing and allows: it is a failure
# like any other.
try {
  $payload = $json.DeserializeObject([Console]::In.ReadToEnd())
} catch {
  Fail 'hooks/exec: the harness payload is not JSON'
}
if ($payload -isnot [System.Collections.IDictionary]) { Fail 'hooks/exec: the harness payload is not JSON' }
Set-Hook 'HOOK_TOOL_NATIVE' (Text (First @((Pick $payload @('toolCall', 'name')), $null)))
Set-Hook 'HOOK_CWD' (Text (First @((Pick $payload @('workspacePaths', 0)), $null)))
$toolInput = (First @((Pick $payload @('toolCall', 'args')), @{}))
Set-Hook 'HOOK_INPUT' $(if ($null -eq $toolInput) { '{}' } else { $json.Serialize($toolInput) })
Set-Hook 'HOOK_SOURCE' $(if ($hookEvent -eq 'session_start') { Text (Pick $payload @('source')) } else { '' })
if ($hookEvent -eq 'session_start' -and -not $env:HOOK_SOURCE) { $env:HOOK_SOURCE = 'startup' }
# The portable fields are read from the input, so a rewrite reads them again.
function Portable-Fields {
$env:HOOK_TOOL = ''
$env:HOOK_COMMAND = ''
$env:HOOK_PATH = ''
$env:HOOK_QUERY = ''
switch -CaseSensitive ($env:HOOK_TOOL_NATIVE) {  # the portable vocabulary
  'run_command' { $env:HOOK_TOOL = 'shell'; Set-Hook 'HOOK_COMMAND' (Text (Pick $toolInput @('CommandLine'))) }
  'view_file' { $env:HOOK_TOOL = 'file.read'; Set-Hook 'HOOK_PATH' (Text (Pick $toolInput @('AbsolutePath'))) }
  'write_to_file' { $env:HOOK_TOOL = 'file.write'; Set-Hook 'HOOK_PATH' (Text (Pick $toolInput @('TargetFile'))) }
  'replace_file_content' { $env:HOOK_TOOL = 'file.edit'; Set-Hook 'HOOK_PATH' (Text (Pick $toolInput @('TargetFile'))) }
  'search_web' { $env:HOOK_TOOL = 'search.web'; Set-Hook 'HOOK_QUERY' (Text (Pick $toolInput @('query'))) }
  'invoke_subagent' { $env:HOOK_TOOL = 'agent.spawn' }
  'send_message' { $env:HOOK_TOOL = 'agent.message' }
}
}
Portable-Fields

# --- the handlers, in order; the first denial stops the rest --------------
# A handler is a PowerShell command line, run from the package root. Each
# runs as a process of its own under its own deadline; past it, it and
# everything it started are stopped, and the group's effect decides.
if (-not (Test-Path -LiteralPath $pluginRoot -PathType Container)) {
  Fail "hooks/exec: the package root is gone: $pluginRoot"
}
Set-Location -LiteralPath $pluginRoot
foreach ($entry in $handlers) {
  $separator = $entry.IndexOf(':')
  $seconds = 0
  if ($separator -lt 1 -or -not [int]::TryParse($entry.Substring(0, $separator), [ref]$seconds)) {
    Fail "malformed handler argument: $entry"
  }
  $handler = $entry.Substring($separator + 1)
  # The line runs from a script file of its own, never re-quoted onto a
  # command line: what the author wrote is what runs.
  $script = [System.IO.Path]::Combine([System.IO.Path]::GetTempPath(), "hooks-exec-$PID-$([guid]::NewGuid().ToString('N')).ps1")
  # It writes UTF-8, as this wrapper reads it: a console's own code page
  # would garble anything past ASCII in the reason it hands back.
  [System.IO.File]::WriteAllText($script, "`$ErrorActionPreference = 'Stop'`n[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding `$false`n$handler`nif (`$LASTEXITCODE) { exit `$LASTEXITCODE }`n", (New-Object System.Text.UTF8Encoding $true))
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
  try {
    $process = [System.Diagnostics.Process]::Start($start)
  } catch {
    Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
    Fail "handler could not start: $handler"
  }
  $process.StandardInput.Close()
  $errors = $process.StandardError.ReadToEndAsync()
  $output = $process.StandardOutput.ReadToEndAsync()
  if ($process.WaitForExit($seconds * 1000)) {
    $process.WaitForExit()
    $status = $process.ExitCode
  } else {
    # Still running, so its pid is still its own: end it with its tree.
    & taskkill.exe /T /F /PID $process.Id 2>&1 | Out-Null
    $status = 124
  }
  Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
  if ($status -eq 0 -and $effect -eq 'transform' -and $output.Wait(1000) -and $output.Result.Trim()) {
    # A rewrite: the complete input, as one JSON object. The next handler
    # reads it as its HOOK_INPUT, and the last one is what the tool runs.
    if ($utf8.GetByteCount($output.Result) -gt 32000) { Fail "handler wrote more than 32000 bytes: $handler" }
    $rewritten = $null
    try { $rewritten = $json.DeserializeObject($output.Result) } catch { $rewritten = $null }
    if ($rewritten -isnot [System.Collections.IDictionary]) { Fail "handler did not write a JSON object: $handler" }
    $toolInput = $rewritten
    Set-Hook 'HOOK_INPUT' ($json.Serialize($toolInput))
    $changed = $true
    Portable-Fields
  }
  if ($status -eq 0) { continue }
  $reason = ''
  if ($errors.Wait(1000)) { $reason = $errors.Result.Trim() }
  if ($reason.Length -gt 4096) { $reason = $reason.Substring(0, 4096) }
  switch ($status) {
    3 {
      # A handler's own denial in an `ask` group asks the person; only a
      # failure falls back to denying.
      $decision = if ($effect -eq 'ask') { 'ask' } else { 'deny' }
      if ($reason) { Deny-Native $reason $decision } else { Deny-Native "$handler denied the operation" $decision }
    }
    124 { Fail "handler timed out after ${seconds}s: $handler" }
    default { if ($reason) { Fail "handler failed (exit $status): $handler — $reason" } else { Fail "handler failed (exit $status): $handler" } }
  }
}
if ($changed) { Transform-Native }
Allow-Native
exit 0
