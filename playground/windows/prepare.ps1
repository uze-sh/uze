# Runs inside Windows Sandbox at logon (see up.sh). Windows Sandbox signs in
# as an administrator with UAC off, so everything it starts is elevated;
# Codex, for one, refuses to run elevated, and an administrator hides what an
# ordinary account would meet. So by default the world is an ordinary user's,
# `person`; `UZE_PLAYGROUND_USER=admin` keeps the administrator. Either way it
# opens in Windows Terminal. Progress goes to C:\playground\prepare.log, which
# the host sees.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$stage = 'C:\playground'
$log = "$stage\prepare.log"
Set-Content $log "started $(Get-Date -Format o)" -Encoding UTF8
function Note([string]$text) { Add-Content $log $text -Encoding UTF8 }
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Expand-Zip([string]$archive, [string]$into) {
    Remove-Item -Recurse -Force $into -ErrorAction SilentlyContinue
    [IO.Compression.ZipFile]::ExtractToDirectory($archive, $into)
}
function Latest-Asset([string]$repository, [string]$pattern) {
    $release = Invoke-RestMethod "https://api.github.com/repos/$repository/releases/latest" -UseBasicParsing
    $release.assets | Where-Object { $_.name -match $pattern } | Select-Object -First 1
}
$asAdministrator = (Get-Content "$stage\user" -Raw).Trim() -eq 'admin'
$password = 'Playground-Person-1!'
$credential = $null
$script:terminal = $null

try {
    # For every account: under Program Files, on the machine's Path.
    Note 'git: MinGit'
    $mingit = Latest-Asset 'git-for-windows/git' '^MinGit-[\d.]+-64-bit\.zip$'
    Invoke-WebRequest $mingit.browser_download_url -OutFile "$env:TEMP\mingit.zip" -UseBasicParsing
    Expand-Zip "$env:TEMP\mingit.zip" "$env:ProgramFiles\MinGit"
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    [Environment]::SetEnvironmentVariable('Path', "$env:ProgramFiles\MinGit\cmd;$machinePath", 'Machine')
    $env:Path = "$env:ProgramFiles\MinGit\cmd;$env:Path"

    # Its unpackaged build, which runs for any account: the packaged one is
    # installed per account, and the Sandbox has no Store to fetch its
    # frameworks from.
    Note 'terminal: Windows Terminal'
    try {
        $kit = Latest-Asset 'microsoft/terminal' '^Microsoft\.WindowsTerminal_[\d.]+_x64\.zip$'
        Invoke-WebRequest $kit.browser_download_url -OutFile "$env:TEMP\terminal.zip" -UseBasicParsing
        Expand-Zip "$env:TEMP\terminal.zip" "$env:ProgramFiles\WindowsTerminal"
        $script:terminal = (Get-ChildItem "$env:ProgramFiles\WindowsTerminal" -Recurse -Filter wt.exe | Select-Object -First 1).FullName
        Note "terminal: $script:terminal"
    } catch {
        Note "terminal: not installed ($_); a PowerShell window is opened instead"
    }

    if ($asAdministrator) {
        & powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$stage\setup-user.ps1" -Stage $stage -Log $log
        $home_ = $env:USERPROFILE
    } else {
        Note 'user: person, an ordinary account'
        $secure = ConvertTo-SecureString $password -AsPlainText -Force
        New-LocalUser -Name person -Password $secure -PasswordNeverExpires | Out-Null
        $credential = New-Object System.Management.Automation.PSCredential("$env:COMPUTERNAME\person", $secure)
        # What the user's part reads, copied where every account may: the
        # mapped folder is the host's, whose permissions are left alone.
        $shared = "$env:PUBLIC\uze-playground"
        Copy-Item $stage $shared -Recurse -Force
        $userLog = "$env:PUBLIC\uze-playground-user.log"
        Set-Content $userLog '' -Encoding UTF8
        icacls $userLog /grant '*S-1-1-0:(M)' /Q | Out-Null
        $setup = Start-Process powershell.exe -Credential $credential -LoadUserProfile -Wait -PassThru `
            -WorkingDirectory $env:PUBLIC -WindowStyle Hidden `
            -ArgumentList '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$shared\setup-user.ps1", '-Stage', $shared, '-Log', $userLog
        Get-Content $userLog | ForEach-Object { Note $_ }
        if ($setup.ExitCode -ne 0) { throw "the user's part failed (exit $($setup.ExitCode))" }
        $home_ = 'C:\Users\person'
    }
    Note 'ready'
} catch {
    Note "failed: $_"
    $home_ = if ($asAdministrator) { $env:USERPROFILE } else { 'C:\Users\person' }
}

# Lent sessions leave the stage, which is the host's own folder, and the
# copy every account could read, however the preparation went.
Remove-Item "$stage\sessions", "$env:PUBLIC\uze-playground" -Recurse -Force -ErrorAction SilentlyContinue

$project = "$home_\projects\demo"
$who = if ($asAdministrator) { 'the Sandbox administrator' } else { 'person, an ordinary account' }
$welcome = @"
uze $((Get-Content "$stage\version" -Raw).Trim()) from your checkout, as $who, with Git and the
playground marketplace.

  uze doctor
  uze setup claude-code            (or codex, opencode, antigravity)
  uze install -m playground@playground
  uze workspace

Progress and errors of this preparation: C:\playground\prepare.log
"@
$greeting = "Write-Host @'`n$welcome`n'@"
$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($greeting))
$shell = 'powershell.exe', '-NoExit', '-EncodedCommand', $encoded
# Windows Terminal is a window of its own; a console started directly would
# share this script's, which the Sandbox's logon command never shows, so the
# fallback goes through `start`, which always opens one.
$program, $arguments = if ($script:terminal) {
    $script:terminal, (@('-d', $project) + $shell)
} else {
    'cmd.exe', (@('/c', 'start', '"uze playground"') + $shell)
}
$account = if ($credential) { @{ Credential = $credential; LoadUserProfile = $true } } else { @{} }
Start-Process $program -WorkingDirectory $project -ArgumentList $arguments @account
