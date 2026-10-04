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
$env:HOOK_HARNESS = 'claude'
Add-Type -AssemblyName System.Web.Extensions
$json = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$json.MaxJsonLength = [int]::MaxValue

# --- this harness's decision dialect ------------------------------------
function Allow-Native {
  
}

function Deny-Native([string]$reason) {
  [Console]::Error.WriteLine($reason)
  # A session start decides nothing: a denial there is a report, and the
  # session opens as if the handler had allowed.
  if ($hookEvent -eq 'session_start') { Allow-Native; exit 0 }
  $reasonJson = $json.Serialize($reason)
  $name = @{ pre_tool_use = 'PreToolUse'; post_tool_use = 'PostToolUse'; stop = 'Stop' }[$hookEvent]
  [Console]::Out.Write('{"hookSpecificOutput":{"hookEventName":"' + $name + '","permissionDecision":"deny","permissionDecisionReason":' + $reasonJson + '}}')
  exit 2
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

# A payload that does not parse leaves every field empty, and a guard
# written the documented way then sees nothing and allows: it is a failure
# like any other.
try {
  $payload = $json.DeserializeObject([Console]::In.ReadToEnd())
} catch {
  Fail 'hooks/exec: the harness payload is not JSON'
}
if ($payload -isnot [System.Collections.IDictionary]) { Fail 'hooks/exec: the harness payload is not JSON' }
$env:HOOK_TOOL_NATIVE = Text (First @((Pick $payload @('tool_name')), $null))
$env:HOOK_CWD = Text (First @((Pick $payload @('cwd')), (Pick $payload @('context', 'cwd')), $null))
$toolInput = (First @((Pick $payload @('tool_input')), @{}))
$env:HOOK_INPUT = if ($null -eq $toolInput) { '{}' } else { $json.Serialize($toolInput) }
$env:HOOK_SOURCE = if ($hookEvent -eq 'session_start') { Text (Pick $payload @('source')) } else { '' }
$env:HOOK_TOOL = ''
$env:HOOK_COMMAND = ''
$env:HOOK_PATH = ''
$env:HOOK_QUERY = ''
switch -CaseSensitive ($env:HOOK_TOOL_NATIVE) {  # the portable vocabulary
  'Bash' { $env:HOOK_TOOL = 'shell'; $env:HOOK_COMMAND = Text (Pick $toolInput @('command')) }
  'Read' { $env:HOOK_TOOL = 'file.read'; $env:HOOK_PATH = Text (Pick $toolInput @('file_path')) }
  'Write' { $env:HOOK_TOOL = 'file.write'; $env:HOOK_PATH = Text (Pick $toolInput @('file_path')) }
  'MultiEdit' { $env:HOOK_TOOL = 'file.edit'; $env:HOOK_PATH = Text (Pick $toolInput @('file_path')) }
  'Edit' { $env:HOOK_TOOL = 'file.edit'; $env:HOOK_PATH = Text (Pick $toolInput @('file_path')) }
  'Grep' { $env:HOOK_TOOL = 'search.files'; $env:HOOK_QUERY = Text (Pick $toolInput @('pattern')) }
  'WebSearch' { $env:HOOK_TOOL = 'search.web'; $env:HOOK_QUERY = Text (Pick $toolInput @('query')) }
  'Task' { $env:HOOK_TOOL = 'agent.spawn' }
}

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
  $null = $process.StandardOutput.ReadToEndAsync()
  if ($process.WaitForExit($seconds * 1000)) {
    $process.WaitForExit()
    $status = $process.ExitCode
  } else {
    # Still running, so its pid is still its own: end it with its tree.
    & taskkill.exe /T /F /PID $process.Id 2>&1 | Out-Null
    $status = 124
  }
  Remove-Item -LiteralPath $script -ErrorAction SilentlyContinue
  if ($status -eq 0) { continue }
  $reason = ''
  if ($errors.Wait(1000)) { $reason = $errors.Result.Trim() }
  if ($reason.Length -gt 4096) { $reason = $reason.Substring(0, 4096) }
  switch ($status) {
    3 { if ($reason) { Deny-Native $reason } else { Deny-Native "$handler denied the operation" } }
    124 { Fail "handler timed out after ${seconds}s: $handler" }
    default { if ($reason) { Fail "handler failed (exit $status): $handler — $reason" } else { Fail "handler failed (exit $status): $handler" } }
  }
}
Allow-Native
exit 0
