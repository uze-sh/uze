# uze: the official installer for Windows.
#
#   irm https://uze.sh/i | iex
#
# Downloads the prebuilt `uze.exe` for this machine from GitHub Releases,
# verifies the release's signature over its checksums (with the OpenSSH
# client Windows ships) and the archive's SHA-256 checksum, installs it
# into the user's programs directory and puts that directory on the user's
# Path. Windows PowerShell
# 5.1 or PowerShell 7; Windows 10 22H2 or Windows 11, x64 and ARM64.
#
# Uninstall (the binary, its Path entry, the harness launchers, and the
# workspace server), keeping ~\.uze unless -Purge is given:
#
#   & ([scriptblock]::Create((irm https://uze.sh/i))) -Uninstall [-Purge]
#
# Environment overrides:
#   UZE_VERSION   Pin a release (e.g. 1.0.0-beta.1); default: latest.
#   UZE_BASE_URL  Alternate download root (mirror or local test fixture).
#   UZE_BIN_DIR   Installation directory (default:
#                 %LOCALAPPDATA%\Programs\uze\bin).
#   UZE_HOME      Where uze keeps its state (default: ~\.uze).
#   NO_COLOR      Any value forces the plain, escape-free transcript.

param(
    [switch]$Uninstall,
    [switch]$Purge
)

# The whole installer is one scriptblock, run on the last line: a download
# cut off midway is a scriptblock that never closes, which PowerShell
# refuses to parse, so nothing of a truncated installer runs at all.
& {
    param([bool]$Uninstall, [bool]$Purge)

    Set-StrictMode -Version 2
    # Every failure is thrown, never `exit`ed: under `iex` an `exit` closes
    # the window the person ran this in, taking the reason with it.
    $ErrorActionPreference = 'Stop'
    # The progress bar slows Invoke-WebRequest down by an order of magnitude
    # on Windows PowerShell 5.1.
    $ProgressPreference = 'SilentlyContinue'

    $DefaultBaseUrl = 'https://github.com/uze-sh/uze/releases'
    # The public key every release's SHASUMS256.txt is signed with: the key
    # line of `release-signing.pub`, which a test holds equal to this one.
    # Empty until the first release key exists, and an empty key installs
    # nothing.
    $ReleaseKey = ''
    $SignatureNamespace = 'uze-release'
    $BuildFloor = 19045

    # --- presentation ---------------------------------------------------------
    # The same rule `src/progress.rs` applies to the CLI: colour only for a
    # real console, and a plain transcript everywhere else. A console not in
    # UTF-8 shows ASCII marks rather than replacement characters.
    $interactive = -not [Console]::IsOutputRedirected -and -not $env:NO_COLOR
    $utf8 = [Console]::OutputEncoding.CodePage -eq 65001
    $esc = [char]27
    function Paint([string]$Code) { if ($interactive) { "$esc[$($Code)m" } else { '' } }
    $Bright = Paint '1;38;2;242;240;234'
    $Muted = Paint '38;2;107;113;118'
    $Heading = Paint '1;38;2;107;113;118'
    $Accent = Paint '38;2;255;255;255'
    $Success = Paint '38;2;143;209;158'
    $Amber = Paint '38;2;224;181;103'
    $Danger = Paint '38;2;224;118;95'
    $Reset = Paint '0'
    $Gutter = if ($utf8) { [string][char]0x2502 } else { '|' }
    $Tick = if ($utf8) { [string][char]0x2713 } else { '+' }
    $Cross = if ($utf8) { [string][char]0x00D7 } else { 'x' }

    function Note([string]$Text) { Write-Host "$Muted$Gutter$Reset $Text" }
    function Ok([string]$Text) { Write-Host "$Success$Tick$Reset $Text" }
    function Warn([string]$Text) { Write-Host "$Amber!$Reset $Text" }
    function Centred([string]$Text) {
        $width = 60
        if ($Text.Length -lt $width) { (' ' * [int](($width - $Text.Length) / 2)) + $Text } else { $Text }
    }

    # One unit of work, settled with a mark once it worked; a failure is
    # thrown with what was being done.
    function Step([string]$Running, [string]$Settled, [scriptblock]$Body) {
        try {
            $result = & $Body
        } catch {
            Write-Host "$Danger$Cross$Reset $Running"
            throw
        }
        if ($result -is [string] -and $result) { $Settled = $result }
        Ok $Settled
    }

    # --- where things go ------------------------------------------------------
    $binDir = if ($env:UZE_BIN_DIR) {
        $env:UZE_BIN_DIR
    } else {
        Join-Path $env:LOCALAPPDATA 'Programs\uze\bin'
    }
    $uzeHome = if ($env:UZE_HOME) { $env:UZE_HOME } else { Join-Path $HOME '.uze' }
    $binary = Join-Path $binDir 'uze.exe'

    # The user's Path as written, `%VARIABLES%` unexpanded: reading the
    # expanded value and writing it back would freeze every variable in it.
    function Get-UserPath {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
        try {
            [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        } finally {
            $key.Close()
        }
    }

    function Save-UserPath([string]$Value) {
        $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        try {
            $key.SetValue('Path', $Value, [Microsoft.Win32.RegistryValueKind]::ExpandString)
        } finally {
            $key.Close()
        }
        # Running programs re-read the environment only when told it changed.
        if (-not ('Uze.Environment' -as [type])) {
            Add-Type -Namespace Uze -Name Environment -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(
    IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam,
    uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
        }
        $result = [UIntPtr]::Zero
        [void][Uze.Environment]::SendMessageTimeout(
            [IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
    }

    # Path entries compare as Windows compares directories: without regard
    # to case or a trailing separator.
    function Test-SameDirectory([string]$A, [string]$B) {
        $A.TrimEnd('\') -ieq $B.TrimEnd('\')
    }

    function Get-PathWithout([string]$Path, [string]$Directory) {
        ($Path -split ';' | Where-Object { $_ -and -not (Test-SameDirectory $_ $Directory) }) -join ';'
    }

    if ($Uninstall) {
        Write-Host "$Bright$(Centred 'UZE')$Reset"
        Write-Host ''
        if (Test-Path -LiteralPath $binary) {
            # The server and every pane it runs end before their binary goes.
            Step 'Stopping the workspace' 'Workspace stopped' {
                # What a native command says on stderr is an error record in
                # Windows PowerShell, and a terminating one under `Stop`.
                $ErrorActionPreference = 'Continue'
                & $binary workspace stop *> $null
            }
            Step "Removing $binary" "Removed $binary" {
                Remove-Item -LiteralPath $binary -Force
                Get-ChildItem -LiteralPath $binDir -Filter 'uze.exe.old-*' -ErrorAction SilentlyContinue |
                    Remove-Item -Force -ErrorAction SilentlyContinue
            }
        }
        $userPath = Get-UserPath
        $kept = Get-PathWithout $userPath $binDir
        if ($kept -ne $userPath) {
            Step 'Removing it from Path' "Removed $binDir from Path" { Save-UserPath $kept }
        }
        $env:Path = Get-PathWithout $env:Path $binDir
        $shims = Join-Path $uzeHome 'shims'
        if (Test-Path -LiteralPath $shims) {
            Step 'Removing the harness launchers' "Removed $shims" {
                Remove-Item -LiteralPath $shims -Recurse -Force
            }
        }
        if ($Purge -and (Test-Path -LiteralPath $uzeHome)) {
            Step "Removing $uzeHome" "Removed $uzeHome" {
                Remove-Item -LiteralPath $uzeHome -Recurse -Force
            }
        } elseif (Test-Path -LiteralPath $uzeHome) {
            Note "$uzeHome is kept: run again with -Purge to remove it"
        }
        return
    }

    # --- the machine ----------------------------------------------------------
    $currentVersion = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    $build = [int](Get-ItemProperty -LiteralPath $currentVersion).CurrentBuildNumber
    if ($build -lt $BuildFloor) {
        throw "Windows build $build is older than uze supports (Windows 10 22H2, build $BuildFloor, or Windows 11)"
    }
    # The machine's own architecture, which an x64 PowerShell running under
    # emulation on ARM64 reports wrongly through its own environment.
    $machine = (Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE
    $arch = switch ($machine) {
        'AMD64' { 'x86_64' }
        'ARM64' { 'aarch64' }
        default { throw "unsupported architecture: $machine (supported: x64, ARM64)" }
    }
    if (-not (Get-Command git.exe -ErrorAction SilentlyContinue)) {
        throw 'uze needs Git: install it first (winget install Git.Git) and open a new terminal'
    }
    if (-not $ReleaseKey) {
        throw 'this installer carries no release signing key, so it cannot verify a release'
    }
    # Windows' own OpenSSH client, by the system directory the kernel
    # names rather than by Path or by an environment variable.
    $sshKeygen = Join-Path ([Environment]::SystemDirectory) 'OpenSSH\ssh-keygen.exe'
    if (-not (Test-Path -LiteralPath $sshKeygen)) {
        throw "uze needs the OpenSSH client to verify a release: $sshKeygen is missing (Settings, Optional features, OpenSSH Client)"
    }
    $archive = "uze-$arch-windows.zip"

    # --- download -------------------------------------------------------------
    # TLS 1.2 is added to what this session allows, never put in its place:
    # replacing the set would turn off TLS 1.3 where it is on.
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    $baseUrl = if ($env:UZE_BASE_URL) { $env:UZE_BASE_URL.TrimEnd('/') } else { $DefaultBaseUrl }
    $releasePath = if ($env:UZE_VERSION) { "download/v$($env:UZE_VERSION)" } else { 'latest/download' }
    $scratch = Join-Path ([IO.Path]::GetTempPath()) ("uze-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $scratch | Out-Null

    try {
        Write-Host "$Bright$(Centred 'UZE')$Reset"
        Write-Host "$Muted$(Centred "$arch-windows")$Reset"
        Write-Host "$Muted$(Centred 'Agents come and go. Your work stays.')$Reset"
        Write-Host ''
        Note "$baseUrl/$releasePath/$archive"

        # A release served from a directory (a mirror on a share, a local
        # test) is copied: PowerShell 7's Invoke-WebRequest refuses `file:`.
        function Get-ReleaseFile([string]$Uri, [string]$Destination) {
            $parsed = [Uri]$Uri
            if ($parsed.IsFile) {
                Copy-Item -LiteralPath $parsed.LocalPath -Destination $Destination
            } else {
                Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
            }
        }

        Step "Downloading $archive" "Downloaded $archive" {
            foreach ($file in @($archive, 'SHASUMS256.txt', 'SHASUMS256.txt.sig')) {
                Get-ReleaseFile "$baseUrl/$releasePath/$file" (Join-Path $scratch $file)
            }
        }

        # The checksums are only as trustworthy as whoever could publish
        # them: the signature is what says the release is uze's. The bytes
        # are handed to ssh-keygen as they are, never through a PowerShell
        # pipeline, which would re-encode them.
        Step 'Verifying the release signature' 'Release signature verified' {
            $signers = Join-Path $scratch 'allowed_signers'
            [IO.File]::WriteAllText($signers,
                "$SignatureNamespace namespaces=`"$SignatureNamespace`" $ReleaseKey`n",
                (New-Object Text.UTF8Encoding $false))
            $start = New-Object Diagnostics.ProcessStartInfo $sshKeygen
            $start.Arguments = "-Y verify -f `"$signers`" -I $SignatureNamespace -n $SignatureNamespace -s `"$(Join-Path $scratch 'SHASUMS256.txt.sig')`""
            $start.UseShellExecute = $false
            $start.RedirectStandardInput = $true
            $start.RedirectStandardOutput = $true
            $start.RedirectStandardError = $true
            $verifier = [Diagnostics.Process]::Start($start)
            $sums = [IO.File]::ReadAllBytes((Join-Path $scratch 'SHASUMS256.txt'))
            $verifier.StandardInput.BaseStream.Write($sums, 0, $sums.Length)
            $verifier.StandardInput.Close()
            $errors = $verifier.StandardError.ReadToEndAsync()
            [void]$verifier.StandardOutput.ReadToEnd()
            $verifier.WaitForExit()
            $reason = $errors.Result.Trim()
            if ($verifier.ExitCode -ne 0) {
                throw "SHASUMS256.txt is not signed by the uze release key ($reason)"
            }
        }

        Step 'Verifying checksum' 'Checksum verified' {
            $line = Get-Content -LiteralPath (Join-Path $scratch 'SHASUMS256.txt') |
                Where-Object { ($_ -split '\s+', 2)[1] -replace '^\*', '' -eq $archive } |
                Select-Object -First 1
            if (-not $line) { throw "no checksum entry for $archive in SHASUMS256.txt" }
            $expected = ($line -split '\s+', 2)[0]
            $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $scratch $archive)).Hash
            if ($actual -ine $expected) {
                throw "checksum mismatch for $archive (expected $expected, got $actual)"
            }
        }

        Step "Installing into $binDir" "Installed $binary" {
            $unpacked = Join-Path $scratch 'unpacked'
            # .NET's own reader rather than Expand-Archive: that cmdlet's
            # module fails to load in a Windows PowerShell started from
            # PowerShell 7, whose module path it inherits.
            Add-Type -AssemblyName System.IO.Compression.FileSystem
            [IO.Compression.ZipFile]::ExtractToDirectory((Join-Path $scratch $archive), $unpacked)
            New-Item -ItemType Directory -Force -Path $binDir | Out-Null
            # A running uze.exe cannot be replaced, but it can be renamed: it
            # steps aside, and the next uze to start sweeps it away.
            if (Test-Path -LiteralPath $binary) {
                Rename-Item -LiteralPath $binary -NewName "uze.exe.old-$PID"
            }
            Copy-Item -LiteralPath (Join-Path $unpacked 'uze.exe') -Destination $binary
        }

        Step 'Verifying the install' 'Verified' {
            $reported = & $binary --version
            if ($LASTEXITCODE -ne 0) { throw "installed binary failed to run: $binary" }
            ([string]$reported).Trim()
        }
        $version = ([string](& $binary --version)).Trim() -replace '^uze ', ''

        # The receipt the updater reads (`src/self_update.rs`): the file this
        # installer placed, and the release it was. UTF-8 without a byte-order
        # mark, which the updater's JSON reader requires.
        try {
            $installed = (Resolve-Path -LiteralPath $binary).ProviderPath
            $receipt = [ordered]@{ binary = $installed; version = $version }
            $state = Join-Path $uzeHome 'state'
            New-Item -ItemType Directory -Force -Path $state | Out-Null
            [IO.File]::WriteAllText((Join-Path $state 'install.json'),
                ($receipt | ConvertTo-Json) + "`n", (New-Object Text.UTF8Encoding $false))
        } catch {
            Warn 'could not record this install; it will not update itself until the next one'
        }

        $userPath = Get-UserPath
        $onPath = $userPath -split ';' | Where-Object { $_ -and (Test-SameDirectory ([Environment]::ExpandEnvironmentVariables($_)) $binDir) }
        if (-not $onPath) {
            Step 'Adding it to Path' "Added $binDir to Path" {
                Save-UserPath ((@($binDir) + ($userPath -split ';' | Where-Object { $_ })) -join ';')
            }
        }
        if (-not ($env:Path -split ';' | Where-Object { $_ -and (Test-SameDirectory $_ $binDir) })) {
            $env:Path = "$binDir;$env:Path"
        }

        # An unsigned binary is one Smart App Control may refuse to run.
        $policy = Get-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' -ErrorAction SilentlyContinue
        if ($policy -and ($policy.PSObject.Properties.Name -contains 'VerifiedAndReputablePolicyState') -and
            $policy.VerifiedAndReputablePolicyState -eq 1) {
            Warn 'Smart App Control is on, and it may block uze.exe, which is not signed yet'
        }

        Write-Host ''
        Write-Host "$($Heading)Next$Reset"
        Write-Host ("  $Accent{0}$Reset{1}" -f 'uze setup', (' ' * 7 + 'Detect and provision your harnesses'))
        Write-Host ("  $Accent{0}$Reset{1}" -f 'uze workspace', (' ' * 3 + 'Open the terminal workspace'))
    } finally {
        Remove-Item -LiteralPath $scratch -Recurse -Force -ErrorAction SilentlyContinue
    }
} $Uninstall.IsPresent $Purge.IsPresent
