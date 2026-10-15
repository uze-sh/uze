# hook.ps1 <log|deny>: the Windows spelling of scripts/hook.
param([string]$Mode)
$log = Join-Path $env:TEMP 'uze-playground-hooks.log'
"$(Get-Date -Format HH:mm:ss) $env:HOOK_HARNESS $env:HOOK_EVENT $env:HOOK_COMMAND" |
  Add-Content -Path $log
if ($Mode -eq 'deny' -and "$env:HOOK_COMMAND" -like '*ping-deny*') {
  [Console]::Error.WriteLine('pong (UZE_PLAYGROUND_HOOK: deny-ping refused this command)')
  exit 3
}
exit 0
