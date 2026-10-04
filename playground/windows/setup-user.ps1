# The part of the world that belongs to one user, run as that user: Git's
# identity, uze through install.ps1 from the staged release, the playground
# MCP server, the playground marketplace and a demo project. Its progress
# goes to the log named on the command line.
param(
    [Parameter(Mandatory = $true)][string]$Stage,
    [Parameter(Mandatory = $true)][string]$Log
)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Note([string]$text) { Add-Content $Log $text -Encoding UTF8 }
function Add-UserPath([string]$directory) {
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    [Environment]::SetEnvironmentVariable('Path', "$directory;$current", 'User')
    $env:Path = "$directory;$env:Path"
}

try {
    Note "user: $(whoami), administrator: $(([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator))"
    git config --global user.name 'Playground Person'
    git config --global user.email 'person@example.invalid'
    git config --global init.defaultBranch main

    $sessions = Join-Path $stage 'sessions'
    if (Test-Path $sessions) {
        Note 'sessions: lent from the host'
        Copy-Item "$sessions\*" $env:USERPROFILE -Recurse -Force
    }

    Note 'uze: install.ps1'
    $env:UZE_BASE_URL = 'file:///' + ("$stage\release" -replace '\\', '/')
    $env:UZE_VERSION = (Get-Content "$stage\version" -Raw).Trim()
    $env:UZE_WINDOWS_PREVIEW = '1'
    Invoke-Expression (Get-Content "$stage\install.ps1" -Raw) *>&1 | ForEach-Object { Note "  $_" }
    $env:Path = "$env:LOCALAPPDATA\Programs\uze\bin;$env:Path"

    Note 'mcp: playground-mcp'
    New-Item -ItemType Directory -Force "$env:USERPROFILE\.local\bin" | Out-Null
    Copy-Item "$stage\playground-mcp.exe" "$env:USERPROFILE\.local\bin\"
    Add-UserPath "$env:USERPROFILE\.local\bin"

    Note 'market: playground'
    $market = "$env:USERPROFILE\playground-market"
    Copy-Item "$stage\market" $market -Recurse
    git -C $market init -q
    git -C $market add -A
    git -C $market commit -q -m 'playground marketplace'
    uze market add $market *>&1 | ForEach-Object { Note "  $_" }

    $project = "$env:USERPROFILE\projects\demo"
    New-Item -ItemType Directory -Force $project | Out-Null
    git -C $project init -q
    Set-Content "$project\AGENTS.md" "# Demo`n`nA project to try uze in.`n"
    git -C $project add -A
    git -C $project commit -q -m init
    Note 'user: ready'
} catch {
    Note "user: failed: $_"
    exit 1
}
