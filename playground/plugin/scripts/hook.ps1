# hook.ps1 <log|deny>: the Windows twin of scripts/hook, which hooks.json
# names once: Windows runs this file under that name.
param([string]$Mode)
$log = Join-Path $env:TEMP 'uze-playground-hooks.log'
"$(Get-Date -Format HH:mm:ss) $env:HOOK_HARNESS $env:HOOK_EVENT $env:HOOK_COMMAND" |
  Add-Content -Path $log
if ($Mode -eq 'deny' -and "$env:HOOK_COMMAND" -like '*ping-deny*') {
  [Console]::Error.WriteLine('pong (UZE_PLAYGROUND_HOOK: deny-ping refused this command)')
  exit 3
}
exit 0
