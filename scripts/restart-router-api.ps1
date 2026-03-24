[CmdletBinding()]
param(
    [string]$HealthUrl = "http://127.0.0.1:8080/healthz",
    [int]$HealthTimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

function Write-Step {
    param([string]$Message)
    Write-Host "==> $Message" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "PASS $Message" -ForegroundColor Green
}

function Resolve-RepoRoot {
    return Split-Path -Parent $PSScriptRoot
}

function Get-BackendExecutablePath {
    param([string]$RepoRoot)
    return Join-Path $RepoRoot "target\debug\router-api.exe"
}

function Get-BackendProcesses {
    param([string]$ExecutablePath)

    $normalized = [System.IO.Path]::GetFullPath($ExecutablePath)
    return Get-CimInstance Win32_Process |
        Where-Object {
            $_.Name -eq "router-api.exe" -and
            $_.ExecutablePath -and
            ([System.IO.Path]::GetFullPath($_.ExecutablePath) -eq $normalized)
        }
}

function Invoke-Build {
    param([string]$RepoRoot)

    $rtk = Get-Command rtk -ErrorAction SilentlyContinue
    if ($rtk) {
        & $rtk.Source cargo build -p router-api
        return
    }

    cargo build -p router-api
}

function Wait-ForHealth {
    param(
        [string]$Url,
        [int]$TimeoutSeconds
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    do {
        try {
            $response = Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 5
            if ([int]$response.StatusCode -eq 200) {
                return
            }
        } catch {
        }

        Start-Sleep -Seconds 1
    } while ((Get-Date) -lt $deadline)

    throw "Backend health check timed out: $Url"
}

$repoRoot = Resolve-RepoRoot
$backendExe = Get-BackendExecutablePath -RepoRoot $repoRoot
$logDir = Join-Path $repoRoot ".codex-local"
$stdoutLog = Join-Path $logDir "router-api.stdout.log"
$stderrLog = Join-Path $logDir "router-api.stderr.log"
$pidFile = Join-Path $logDir "router-api.pid"

if (-not (Test-Path $backendExe)) {
    Write-Step "Backend executable not found yet; a build will generate it."
}

Write-Step "Stopping existing router-api process"
$existing = @(Get-BackendProcesses -ExecutablePath $backendExe)
foreach ($process in $existing) {
    Stop-Process -Id $process.ProcessId -Force
}
Write-Success "Stopped $($existing.Count) existing process(es)"

Write-Step "Building router-api"
Push-Location $repoRoot
try {
    Invoke-Build -RepoRoot $repoRoot
} finally {
    Pop-Location
}
Write-Success "Build completed"

New-Item -ItemType Directory -Force -Path $logDir | Out-Null

Write-Step "Starting router-api"
$process = Start-Process `
    -FilePath $backendExe `
    -WorkingDirectory $repoRoot `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru

$process.Id | Set-Content -Path $pidFile

Write-Step "Waiting for health check"
Wait-ForHealth -Url $HealthUrl -TimeoutSeconds $HealthTimeoutSeconds

Write-Success "router-api restarted successfully"
Write-Host "PID: $($process.Id)"
Write-Host "Health: $HealthUrl"
Write-Host "Stdout: $stdoutLog"
Write-Host "Stderr: $stderrLog"
