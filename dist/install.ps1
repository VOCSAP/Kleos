# Kleos installer for Windows -- downloads the correct binaries.
#
# Usage (PowerShell):
#   irm https://raw.githubusercontent.com/Ghost-Frame/Kleos/main/dist/install.ps1 | iex
#
# Options (via environment variables):
#   $env:KLEOS_VERSION   -- version to install (default: latest)
#   $env:KLEOS_INSTALL   -- installation directory (default: ~\.kleos\bin)
#   $env:KLEOS_PROFILE   -- "server" (default), "agent-host", or "full"
#   $env:KLEOS_BINARIES  -- comma-separated list (overrides KLEOS_PROFILE if set)

$ErrorActionPreference = "Stop"

$Repo = "Ghost-Frame/Kleos"
$Version = if ($env:KLEOS_VERSION) { $env:KLEOS_VERSION } else { "" }
$InstallDir = if ($env:KLEOS_INSTALL) { $env:KLEOS_INSTALL } else { Join-Path $HOME ".kleos\bin" }
$Profile = if ($env:KLEOS_PROFILE) { $env:KLEOS_PROFILE } else { "server" }
$Suffix = "windows-x64"

# Resolve binary list from profile if not set explicitly
if ($env:KLEOS_BINARIES) {
    $BinList = $env:KLEOS_BINARIES -split ","
} else {
    # NOTE: kr/kw/ke (kleos-fs) and kleos-credd are LINUX-ONLY -- they depend on
    # unix-only crates and are not cross-compiled for windows-gnu (see
    # .woodpecker.yml build-windows-x64). Do not list them here: with
    # $ErrorActionPreference = "Stop", a 404 on a missing binary aborts the
    # whole install. Windows profiles ship only the binaries the release
    # actually produces for windows-x64.
    switch ($Profile) {
        { $_ -in "agent-host", "agent" } {
            $BinList = @("kleos-cli", "kleos-sh", "agent-forge", "eidolon-supervisor", "cred")
        }
        "full" {
            $BinList = @("kleos-server", "kleos-cli", "kleos-sidecar", "cred", "kleos-mcp", "kleos-sh", "agent-forge", "eidolon-supervisor")
        }
        default {
            $BinList = @("kleos-server", "kleos-cli", "kleos-mcp")
        }
    }
}

# ─── Resolve latest version ─────────────────────────────────────────────────

if (-not $Version) {
    Write-Host "Fetching latest release..."
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        $Version = $release.tag_name -replace "^v", ""
    }
    catch {
        Write-Error "Could not determine latest version. Set `$env:KLEOS_VERSION manually."
        exit 1
    }
}

# ─── SHA-256 verification helpers ────────────────────────────────────────────

function Get-ExpectedHash {
    param([string]$Filename, [hashtable]$Manifest)
    if ($Manifest.ContainsKey($Filename)) { return $Manifest[$Filename] }
    if ($Manifest.ContainsKey("*$Filename")) { return $Manifest["*$Filename"] }
    return $null
}

function Test-BinaryHash {
    param([string]$Path, [string]$Expected)
    $actual = (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $Expected) {
        Remove-Item -Force $Path
        Write-Host "FAILED (sha256 mismatch)"
        Write-Host "    expected: $Expected" -ForegroundColor Red
        Write-Host "    actual:   $actual" -ForegroundColor Red
        return $false
    }
    return $true
}

# ─── Install ─────────────────────────────────────────────────────────────────

Write-Host "Installing Kleos v$Version ($Suffix)"
Write-Host "  Binaries: $($BinList -join ', ')"
Write-Host "  Target:   $InstallDir"
Write-Host ""

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$BaseUrl = "https://github.com/$Repo/releases/download/v$Version"
$Failed = @()

# Fetch SHA256SUMS once. Graceful if missing (older releases may not have it).
$ShaManifest = @{}
$HasManifest = $false
try {
    $raw = (Invoke-WebRequest -Uri "$BaseUrl/SHA256SUMS" -UseBasicParsing).Content
    foreach ($line in ($raw -split "`n")) {
        $line = $line.Trim()
        if ($line -and $line -match '^([0-9a-f]{64})\s+(.+)$') {
            $ShaManifest[$Matches[2]] = $Matches[1]
        }
    }
    if ($ShaManifest.Count -gt 0) {
        $HasManifest = $true
        Write-Host "  manifest: SHA256SUMS fetched; integrity will be verified"
    }
}
catch {
    Write-Host "  warn: SHA256SUMS not found for this release; integrity NOT verified" -ForegroundColor Yellow
}
Write-Host ""

foreach ($bin in $BinList) {
    $fname = "$bin-$Suffix.exe"
    $url = "$BaseUrl/$fname"
    $dest = Join-Path $InstallDir "$bin.exe"

    Write-Host -NoNewline ("  {0,-20}" -f $bin)
    try {
        Invoke-WebRequest -Uri $url -OutFile $dest -UseBasicParsing
    }
    catch {
        Write-Host "FAILED (download)"
        $Failed += $bin
        continue
    }

    $expected = Get-ExpectedHash -Filename $fname -Manifest $ShaManifest
    if ($HasManifest -and -not $expected) {
        Write-Host "FAILED (no checksum entry)"
        Remove-Item -Force $dest
        $Failed += $bin
        continue
    }
    if ($expected) {
        if (-not (Test-BinaryHash -Path $dest -Expected $expected)) {
            $Failed += $bin
            continue
        }
        Write-Host "OK (verified)"
    } else {
        Write-Host "OK (unverified)"
    }
}

Write-Host ""

if ($Failed.Count -gt 0) {
    Write-Warning "Failed to install: $($Failed -join ', ')"
    Write-Warning "Check https://github.com/$Repo/releases/tag/v$Version"
}

# Agent-host profile: drop sample supervisor config if none exists
if ($Profile -in "agent-host", "agent", "full") {
    $SupervisorDir = Join-Path $HOME ".config\eidolon"
    $SupervisorConfig = Join-Path $SupervisorDir "supervisor.json"
    if (-not (Test-Path $SupervisorConfig)) {
        New-Item -ItemType Directory -Force -Path $SupervisorDir | Out-Null
        @'
[
  {"id":"no-force-push","check_type":"rule_match","pattern":"git\\s+push\\s+.*--force","severity":"critical","cooldown_secs":300,"message":"Force push detected"},
  {"id":"no-reboot","check_type":"rule_match","pattern":"reboot|shutdown|systemctl\\s+(reboot|poweroff)","severity":"critical","cooldown_secs":600,"message":"Reboot or shutdown command detected"},
  {"id":"retry-loop","check_type":"retry_loop","pattern":"","severity":"warning","cooldown_secs":120,"message":"Agent stuck in retry loop (3+ identical failing commands)"}
]
'@ | Set-Content $SupervisorConfig
        Write-Host "  Dropped sample config: $SupervisorConfig"
    }
    Write-Host ""
}

# Check if install dir is on PATH
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$InstallDir*") {
    Write-Host "Add to PATH (run once):"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `"$InstallDir;`$env:Path`", 'User')"
    Write-Host ""
}

$VerifyCmd = if ($Profile -eq "server") { "kleos-server --version" } else { "kleos-cli --version" }
Write-Host "Done. Verify with: $VerifyCmd"
