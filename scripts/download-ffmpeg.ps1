# Download a verified Capto FFmpeg sidecar from its GitHub Release.
#
# The release asset is selected from the Rust target triple, then checked against
# the SHA256SUMS asset before it is copied into Tauri's externalBin layout.
# Nothing is downloaded or executed at runtime by Capto itself.
#
# Usage:
#   .\scripts\download-ffmpeg.ps1
#   .\scripts\download-ffmpeg.ps1 -TargetTriple aarch64-pc-windows-msvc
#   .\scripts\download-ffmpeg.ps1 -Tag v1.0.0-n9.0
#
# Pin defaults live in `.github/capto-ffmpeg.env` (overridable via env / params).

param(
    [string]$TargetTriple = "",
    [string]$Tag = "",
    [string]$Repository = "",
    [switch]$VerifyAttestation
)

$ErrorActionPreference = "Stop"

function Read-PinnedEnv {
    $path = Join-Path (Split-Path -Parent $PSScriptRoot) ".github\capto-ffmpeg.env"
    $map = @{}
    if (Test-Path -LiteralPath $path) {
        Get-Content -LiteralPath $path | ForEach-Object {
            $line = $_.Trim()
            if (-not $line -or $line.StartsWith("#")) { return }
            $parts = $line -split "=", 2
            if ($parts.Count -eq 2) {
                $map[$parts[0].Trim()] = $parts[1].Trim()
            }
        }
    }
    return $map
}

$pinned = Read-PinnedEnv
if (-not $Repository) {
    $Repository = if ($env:CAPTO_FFMPEG_REPO) { $env:CAPTO_FFMPEG_REPO } elseif ($pinned["CAPTO_FFMPEG_REPO"]) { $pinned["CAPTO_FFMPEG_REPO"] } else { "ciryanna/capto-ffmpeg" }
}
if (-not $Tag) {
    $Tag = if ($env:CAPTO_FFMPEG_TAG) { $env:CAPTO_FFMPEG_TAG } elseif ($pinned["CAPTO_FFMPEG_TAG"]) { $pinned["CAPTO_FFMPEG_TAG"] } else { "v1.0.0-n9.0" }
}

if (-not $TargetTriple) {
    $TargetTriple = (& rustc --print host-tuple 2>$null | Select-Object -First 1).Trim()
}
if (-not $TargetTriple) {
    throw "Could not determine a Rust target triple. Pass -TargetTriple explicitly."
}

$asset = switch ($TargetTriple) {
    "x86_64-pc-windows-msvc" { "ffmpeg-windows-x86_64.exe"; break }
    "aarch64-pc-windows-msvc" { "ffmpeg-windows-aarch64.exe"; break }
    default { throw "No Capto FFmpeg Release asset is defined for target: $TargetTriple" }
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$binDir = Join-Path $repoRoot "apps\desktop\src-tauri\binaries"
$releaseBase = "https://github.com/$Repository/releases/download/$Tag"
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("capto-ffmpeg-" + [Guid]::NewGuid())
$downloaded = Join-Path $tempDir $asset

New-Item -ItemType Directory -Force -Path $tempDir | Out-Null
try {
    $sumsResponse = Invoke-WebRequest -UseBasicParsing -Uri "$releaseBase/SHA256SUMS"
    $sums = if ($sumsResponse.Content -is [byte[]]) {
        [System.Text.Encoding]::UTF8.GetString($sumsResponse.Content)
    } else {
        [string]$sumsResponse.Content
    }
    $line = $sums -split "\r?\n" | Where-Object {
        $_ -match ("^([0-9A-Fa-f]{64})\s+\*?" + [regex]::Escape($asset) + "$")
    } | Select-Object -First 1
    if (-not $line) {
        throw "SHA256SUMS does not contain an entry for $asset"
    }
    $expectedHash = ([regex]::Match($line, "^[0-9A-Fa-f]{64}").Value).ToUpperInvariant()

    Invoke-WebRequest -UseBasicParsing -Uri "$releaseBase/$asset" -OutFile $downloaded
    $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $downloaded).Hash.ToUpperInvariant()
    if ($actualHash -ne $expectedHash) {
        throw "SHA-256 mismatch for $asset. Expected $expectedHash, got $actualHash."
    }

    if ($VerifyAttestation -or $env:CAPTO_FFMPEG_VERIFY_ATTESTATION -eq "1") {
        if (Get-Command gh -ErrorAction SilentlyContinue) {
            # Prefer --repo (gh attestation API). Fall back to -R for older gh.
            $verifyArgs = @("attestation", "verify", $downloaded, "--repo", $Repository)
            Write-Host "Running: gh $($verifyArgs -join ' ')"
            & gh @verifyArgs
            if ($LASTEXITCODE -ne 0) {
                throw "Attestation verification failed for $asset (exit $LASTEXITCODE). Ensure GH_TOKEN can read attestations on $Repository."
            }
        } else {
            Write-Warning "gh not found; skipping attestation verify (SHA-256 still checked)."
        }
    }

    New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    $targetPath = Join-Path $binDir "ffmpeg-$TargetTriple.exe"
    Copy-Item -LiteralPath $downloaded -Destination $targetPath -Force

    # `ffmpeg.exe` is the development-side name. It is safe only when the
    # downloaded target matches the machine running the dev app.
    $hostTriple = (& rustc --print host-tuple 2>$null | Select-Object -First 1).Trim()
    if ($hostTriple -eq $TargetTriple) {
        Copy-Item -LiteralPath $downloaded -Destination (Join-Path $binDir "ffmpeg.exe") -Force
    }

    $meta = @{
        repository = $Repository
        tag        = $Tag
        asset      = $asset
        sha256     = $actualHash
    } | ConvertTo-Json -Compress
    Set-Content -LiteralPath (Join-Path $binDir "capto-ffmpeg.json") -Value $meta -Encoding utf8

    Write-Host "Release: $Repository@$Tag"
    Write-Host "Asset  : $asset"
    Write-Host "SHA256 : $actualHash"
    Write-Host "Copied : $targetPath"
    if ($hostTriple -eq $TargetTriple) {
        Write-Host "Copied : $(Join-Path $binDir 'ffmpeg.exe')"
    }
    Write-Host "Meta   : $(Join-Path $binDir 'capto-ffmpeg.json')"
}
finally {
    Remove-Item -LiteralPath $tempDir -Recurse -Force -ErrorAction SilentlyContinue
}
