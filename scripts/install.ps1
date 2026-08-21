param(
    [ValidateSet('debug', 'release')]
    [string]$Configuration = 'release',
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
$targetRoot = Join-Path $repoRoot 'target'
$destinationDirectory = Join-Path $env:USERPROFILE '.local\bin'
$destination = Join-Path $destinationDirectory 'git-explain.exe'

$candidates = @(
    (Join-Path $targetRoot "$Configuration\git-explain.exe")
)
$candidates += Get-ChildItem -LiteralPath $targetRoot -Directory -ErrorAction SilentlyContinue |
    ForEach-Object { Join-Path $_.FullName "$Configuration\git-explain.exe" }

$source = $candidates |
    Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } |
    Select-Object -First 1
if (-not $source) {
    throw "Built git-explain binary was not found for configuration '$Configuration'. Run the matching build task first."
}

New-Item -ItemType Directory -Force -Path $destinationDirectory | Out-Null

$running = @(Get-Process -Name 'git-explain' -ErrorAction SilentlyContinue |
    Where-Object {
        try { $_.Path -eq $destination } catch { $false }
    })
$restartDaemon = $running.Count -gt 0

if ($restartDaemon) {
    Write-Host "Stopping the running git-explain daemon before installation..."
    try {
        & $destination daemon stop | Out-Host
    } catch {
        Write-Verbose "Graceful daemon stop failed: $($_.Exception.Message)"
    }

    $deadline = (Get-Date).AddSeconds(5)
    do {
        Start-Sleep -Milliseconds 200
        $running = @(Get-Process -Name 'git-explain' -ErrorAction SilentlyContinue |
            Where-Object {
                try { $_.Path -eq $destination } catch { $false }
            })
    } while ($running.Count -gt 0 -and (Get-Date) -lt $deadline)

    if ($running.Count -gt 0) {
        if (-not $Force) {
            throw "git-explain.exe is still busy. Retry with: task install FORCE=true"
        }
        Write-Host "Force-stopping the busy git-explain daemon..."
        $running | Stop-Process -Force
        Start-Sleep -Milliseconds 300
    }
}

Copy-Item -LiteralPath $source -Destination $destination -Force
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
$installedHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
if ($sourceHash -ne $installedHash) {
    throw "Installed binary hash does not match the built binary."
}

Write-Host "Installed $source"
Write-Host "        to $destination"
Write-Host "SHA256: $installedHash"

if ($restartDaemon) {
    Write-Host "Restarting the git-explain daemon..."
    Start-Process -FilePath $destination -ArgumentList @('daemon', 'start') -WindowStyle Hidden | Out-Null
    Write-Host "Daemon restart requested."
}
