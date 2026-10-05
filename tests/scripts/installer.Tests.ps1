# install.ps1 run as a person runs it, in a process of its own, against a
# release served from a directory: what it leaves on the machine is what is
# asserted, never what it says. Pester 5, under Windows PowerShell 5.1 and
# PowerShell 7 alike (the `installer-windows` job runs both).

BeforeAll {
    $script:installer = Join-Path $PSScriptRoot '..\..\install.ps1' | Resolve-Path
    $script:version = '9.9.9'
    $script:shell = (Get-Process -Id $PID).Path
    $script:root = Join-Path ([IO.Path]::GetTempPath()) ("uze-installer-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $root | Out-Null

    # A stand-in uze.exe that answers `--version` and nothing else. Built by
    # Windows PowerShell, whose Add-Type writes a console program.
    $stand = Join-Path $root 'stand-in'
    New-Item -ItemType Directory -Path $stand | Out-Null
    $source = @"
public static class Program {
    public static int Main(string[] args) {
        if (args.Length > 0 && args[0] == "--version") { System.Console.WriteLine("uze $version"); }
        return 0;
    }
}
"@
    $built = Join-Path $stand 'uze.exe'
    $code = Join-Path $stand 'uze.cs'
    Set-Content -LiteralPath $code $source -Encoding Ascii
    & "$env:SystemRoot\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -Command `
        "Add-Type -Path '$code' -OutputAssembly '$built' -OutputType ConsoleApplication"
    if (-not (Test-Path $built)) { throw 'the stand-in uze.exe was not built' }

    $arch = if ((Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
    $script:archive = "uze-$arch-windows.zip"
    $script:release = Join-Path $root 'release'

    function script:Publish-Release([string]$Checksum) {
        $download = Join-Path $release "download\v$version"
        Remove-Item -Recurse -Force $release -ErrorAction SilentlyContinue
        New-Item -ItemType Directory -Path $download | Out-Null
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $zip = Join-Path $download $archive
        $archiveFile = [IO.Compression.ZipFile]::Open($zip, 'Create')
        try { [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($archiveFile, $built, 'uze.exe') }
        finally { $archiveFile.Dispose() }
        if (-not $Checksum) { $Checksum = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower() }
        Set-Content -LiteralPath (Join-Path $download 'SHASUMS256.txt') "$Checksum  $archive" -Encoding Ascii
    }

    # The user's Path is the machine's, so every test leaves it as it was.
    $script:savedPath = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue(
        'Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)

    function script:Invoke-Installer([string[]]$Arguments = @(), [hashtable]$Environment = @{}) {
        $variables = @{
            UZE_BASE_URL        = ([Uri]$release).AbsoluteUri
            UZE_VERSION         = $version
            UZE_BIN_DIR         = $script:bin
            UZE_HOME            = $script:home_
            NO_COLOR            = '1'
        }
        foreach ($key in $Environment.Keys) { $variables[$key] = $Environment[$key] }
        $saved = @{}
        foreach ($key in $variables.Keys) {
            $saved[$key] = [Environment]::GetEnvironmentVariable($key)
            [Environment]::SetEnvironmentVariable($key, $variables[$key])
        }
        try {
            # What the installer writes on stderr is its answer, read below;
            # under a caller's `Stop` (CI runs steps so) it would throw here.
            $ErrorActionPreference = 'Continue'
            $output = & $shell -NoProfile -ExecutionPolicy Bypass -File $installer @Arguments 2>&1
            [pscustomobject]@{ Code = $LASTEXITCODE; Output = ($output | Out-String) }
        } finally {
            foreach ($key in $saved.Keys) { [Environment]::SetEnvironmentVariable($key, $saved[$key]) }
        }
    }

    function script:Get-UserPathValue {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
        try {
            [pscustomobject]@{
                Value = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
                Kind  = $key.GetValueKind('Path')
            }
        } finally { $key.Close() }
    }
}

AfterAll {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try { $key.SetValue('Path', $savedPath, [Microsoft.Win32.RegistryValueKind]::ExpandString) } finally { $key.Close() }
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}

Describe 'install.ps1' {
    BeforeEach {
        $script:bin = Join-Path $root ("bin-" + [Guid]::NewGuid().ToString('N'))
        $script:home_ = Join-Path $root ("home-" + [Guid]::NewGuid().ToString('N'))
        Publish-Release
    }

    It 'installs the pinned release, records it, and puts it on Path' {
        $run = Invoke-Installer
        $run.Code | Should -Be 0 -Because $run.Output
        Join-Path $bin 'uze.exe' | Should -Exist
        $receipt = Join-Path $home_ 'state\install.json'
        $receipt | Should -Exist
        $bytes = [IO.File]::ReadAllBytes($receipt)
        ($bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB) | Should -BeFalse -Because 'the updater reads the receipt without a byte-order mark'
        (Get-Content -Raw $receipt | ConvertFrom-Json).version | Should -Be $version
        $path = Get-UserPathValue
        ($path.Value -split ';') | Should -Contain $bin
        $path.Kind | Should -Be ([Microsoft.Win32.RegistryValueKind]::ExpandString)
    }

    It 'refuses an archive that does not match its checksum and installs nothing' {
        Publish-Release -Checksum ('0' * 64)
        $run = Invoke-Installer
        $run.Code | Should -Not -Be 0
        $run.Output | Should -Match 'checksum mismatch'
        Join-Path $bin 'uze.exe' | Should -Not -Exist
    }

    It 'refuses a machine without Git, and says how to get it' {
        $withoutGit = (($env:Path -split ';') | Where-Object {
                $_ -and -not (Test-Path (Join-Path $_ 'git.exe'))
            }) -join ';'
        $run = Invoke-Installer -Environment @{ Path = $withoutGit }
        $run.Code | Should -Not -Be 0
        $run.Output | Should -Match 'needs Git'
        Join-Path $bin 'uze.exe' | Should -Not -Exist
    }

    It 'uninstalls the binary and its Path entry, and keeps the state' {
        (Invoke-Installer).Code | Should -Be 0
        $run = Invoke-Installer -Arguments @('-Uninstall')
        $run.Code | Should -Be 0 -Because $run.Output
        Join-Path $bin 'uze.exe' | Should -Not -Exist
        ((Get-UserPathValue).Value -split ';') | Should -Not -Contain $bin
        $home_ | Should -Exist
    }
}
