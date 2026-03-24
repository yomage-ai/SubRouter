[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:8080",
    [string]$AdminToken = $env:SUBROUTER_ADMIN_TOKEN,
    [switch]$ExerciseProxy,
    [switch]$RequireProxyReady,
    [string]$ProxyPath = "/v1/responses",
    [string]$Model = "gpt-5"
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

function Write-WarningMessage {
    param([string]$Message)
    Write-Host "WARN $Message" -ForegroundColor Yellow
}

function Fail-Smoke {
    param([string]$Message)
    throw "Smoke test failed: $Message"
}

function Parse-JsonContent {
    param([string]$Content)

    if ([string]::IsNullOrWhiteSpace($Content)) {
        return $null
    }

    try {
        return $Content | ConvertFrom-Json
    } catch {
        return $Content
    }
}

function Invoke-SmokeRequest {
    param(
        [string]$Method,
        [string]$Uri,
        [hashtable]$Headers = @{},
        $Body = $null,
        [Microsoft.PowerShell.Commands.WebRequestSession]$WebSession
    )

    $params = @{
        Method           = $Method
        Uri              = $Uri
        Headers          = $Headers
        UseBasicParsing  = $true
    }

    if ($null -ne $Body) {
        $params.Body = if ($Body -is [string]) { $Body } else { $Body | ConvertTo-Json -Depth 10 }
        $params.ContentType = "application/json"
    }

    if ($WebSession) {
        $params.WebSession = $WebSession
    }

    try {
        $response = Invoke-WebRequest @params
        return [pscustomobject]@{
            StatusCode = [int]$response.StatusCode
            Content    = $response.Content
            Json       = Parse-JsonContent -Content $response.Content
        }
    } catch {
        $httpResponse = $_.Exception.Response
        if (-not $httpResponse) {
            throw
        }

        $reader = New-Object System.IO.StreamReader($httpResponse.GetResponseStream())
        $content = $reader.ReadToEnd()
        return [pscustomobject]@{
            StatusCode = [int]$httpResponse.StatusCode
            Content    = $content
            Json       = Parse-JsonContent -Content $content
        }
    }
}

$base = $BaseUrl.TrimEnd("/")

Write-Step "Checking healthz"
$health = Invoke-SmokeRequest -Method "GET" -Uri "$base/healthz"
if ($health.StatusCode -ne 200 -or $health.Json.status -ne "ok") {
    Fail-Smoke "healthz returned unexpected payload: $($health.Content)"
}
Write-Success "healthz is healthy"

if ([string]::IsNullOrWhiteSpace($AdminToken)) {
    Fail-Smoke "Admin token is missing. Pass -AdminToken or set SUBROUTER_ADMIN_TOKEN."
}

$adminHeaders = @{ Authorization = "Bearer $AdminToken" }

Write-Step "Checking bearer admin API"
$dashboard = Invoke-SmokeRequest -Method "GET" -Uri "$base/api/admin/dashboard" -Headers $adminHeaders
if ($dashboard.StatusCode -ne 200) {
    Fail-Smoke "Bearer dashboard failed: HTTP $($dashboard.StatusCode) $($dashboard.Content)"
}
Write-Success "Bearer admin API works"

Write-Step "Checking admin session login"
$session = New-Object Microsoft.PowerShell.Commands.WebRequestSession
$login = Invoke-SmokeRequest -Method "POST" -Uri "$base/api/admin/session/login" -Body @{ token = $AdminToken } -WebSession $session
if ($login.StatusCode -ne 204) {
    Fail-Smoke "Admin login failed: HTTP $($login.StatusCode) $($login.Content)"
}

$me = Invoke-SmokeRequest -Method "GET" -Uri "$base/api/admin/session/me" -WebSession $session
if ($me.StatusCode -ne 200 -or -not $me.Json.authenticated) {
    Fail-Smoke "session/me did not return authenticated=true"
}
Write-Success "Admin session works"

Write-Step "Checking accounts list"
$accounts = Invoke-SmokeRequest -Method "GET" -Uri "$base/api/admin/accounts" -WebSession $session
if ($accounts.StatusCode -ne 200) {
    Fail-Smoke "Accounts API failed: HTTP $($accounts.StatusCode) $($accounts.Content)"
}

$accountCount = if ($accounts.Json) { @($accounts.Json).Count } else { 0 }
Write-Success "Accounts API works, current account count: $accountCount"

if (-not $ExerciseProxy) {
    Write-Host "Smoke test completed. Proxy request skipped. Add -ExerciseProxy to validate /v1." -ForegroundColor Cyan
    exit 0
}

if ($accountCount -eq 0) {
    $message = "No account is configured, proxy request skipped."
    if ($RequireProxyReady) {
        Fail-Smoke $message
    }

    Write-WarningMessage $message
    exit 0
}

Write-Step "Checking proxy endpoint $ProxyPath"
$proxyBody = @{
    model = $Model
    instructions = "SubRouter smoke test"
    input = @(
        @{
            type = "message"
            role = "user"
            content = @(
                @{
                    type = "input_text"
                    text = "SubRouter smoke test"
                }
            )
        }
    )
    max_output_tokens = 16
}

if ($ProxyPath -eq "/v1/responses") {
    $proxyBody.store = $false
    $proxyBody.stream = $true
}

$proxyResponse = Invoke-SmokeRequest -Method "POST" -Uri "$base$ProxyPath" -Body $proxyBody

if ($proxyResponse.StatusCode -lt 200 -or $proxyResponse.StatusCode -ge 300) {
    Fail-Smoke "Proxy request failed: HTTP $($proxyResponse.StatusCode) $($proxyResponse.Content)"
}

Write-Success "Proxy endpoint returned HTTP $($proxyResponse.StatusCode)"
