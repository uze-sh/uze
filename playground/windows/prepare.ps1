# Runs inside Windows Sandbox at logon (see up.sh): Git, Windows Terminal,
# uze installed through install.ps1 from the staged release, the playground
# marketplace registered, then a terminal in a demo project. Progress goes to
# C:\playground\prepare.log, which the host sees.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$stage = 'C:\playground'
$log = "$stage\prepare.log"
Set-Content $log "started $(Get-Date -Format o)" -Encoding UTF8
function Note([string]$text) { Add-Content $log $text -Encoding UTF8 }
function Add-UserPath([string]$directory) {
    $current = [Environment]::GetEnvironmentVariable('Path', 'User')
    [Environment]::SetEnvironmentVariable('Path', "$directory;$current", 'User')
    $env:Path = "$directory;$env:Path"
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Expand-Zip([string]$archive, [string]$into) {
    Remove-Item -Recurse -Force $into -ErrorAction SilentlyContinue
    [IO.Compression.ZipFile]::ExtractToDirectory($archive, $into)
}
function Latest-Asset([string]$repository, [string]$pattern) {
    $release = Invoke-RestMethod "https://api.github.com/repos/$repository/releases/latest" -UseBasicParsing
    $release.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
}

$project = "$env:USERPROFILE\projects\demo"
try {
    Note 'git: MinGit'
    $mingit = Latest-Asset 'git-for-windows/git' '^MinGit-[\d.]+-64-bit\.zip$'
    Invoke-WebRequest $mingit.browser_download_url -OutFile "$env:TEMP\mingit.zip" -UseBasicParsing
    Expand-Zip "$env:TEMP\mingit.zip" "$env:LOCALAPPDATA\MinGit"
    Add-UserPath "$env:LOCALAPPDATA\MinGit\cmd"
    git config --global user.name 'Playground Person'
    git config --global user.email 'person@example.invalid'
    git config --global init.defaultBranch main

    # Windows Terminal's preinstall kit carries the framework packages it
    # needs, which the Sandbox does not have and has no Store to fetch.
    Note 'terminal: Windows Terminal'
    try {
        $kit = Latest-Asset 'microsoft/terminal' '_Windows10_PreinstallKit\.zip$'
        Invoke-WebRequest $kit.browser_download_url -OutFile "$env:TEMP\terminal.zip" -UseBasicParsing
        Expand-Zip "$env:TEMP\terminal.zip" "$env:TEMP\terminal"
        Get-ChildItem "$env:TEMP\terminal" -Filter '*x64*.appx' | ForEach-Object { Add-AppxPackage $_.FullName }
        Add-AppxPackage (Get-ChildItem "$env:TEMP\terminal" -Filter '*.msixbundle' | Select-Object -First 1).FullName
        Note 'terminal: installed'
    } catch {
        Note "terminal: not installed ($_); a PowerShell window is opened instead"
    }

    Note 'uze: install.ps1'
    $env:UZE_BASE_URL = 'file:///C:/playground/release'
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

    New-Item -ItemType Directory -Force $project | Out-Null
    git -C $project init -q
    Set-Content "$project\AGENTS.md" "# Demo`n`nA project to try uze in.`n"
    git -C $project add -A
    git -C $project commit -q -m init
    Note 'ready'
} catch {
    Note "failed: $_"
}

$welcome = @"
uze $((Get-Content "$stage\version" -Raw).Trim()) from your checkout, with Git and the playground marketplace.

  uze doctor
  uze setup claude-code            (or codex, antigravity)
  uze install -m playground@playground
  uze workspace

Progress and errors of this preparation: C:\playground\prepare.log
"@
$greeting = "Write-Host @'`n$welcome`n'@"
$terminal = Get-Command wt.exe -ErrorAction SilentlyContinue
if ($terminal) {
    Start-Process $terminal.Source -ArgumentList '-d', $project, 'powershell.exe', '-NoExit', '-Command', $greeting
} else {
    Start-Process powershell.exe -WorkingDirectory $project -ArgumentList '-NoExit', '-Command', $greeting
}
