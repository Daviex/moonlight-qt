param(
    [string]$HelperPath = "",
    [int]$TimeoutSeconds = 15
)

$ErrorActionPreference = "Stop"

function Resolve-HelperPath {
    param([string]$RequestedPath)

    if ($RequestedPath) {
        return (Resolve-Path -LiteralPath $RequestedPath).Path
    }

    $repoRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
    $candidates = @(
        (Join-Path $repoRoot "build\tauri-prototype\native\Moonlight.exe"),
        (Join-Path $repoRoot "build\deploy-x64-release\Moonlight.exe"),
        (Join-Path $repoRoot "build\deploy-arm64-release\Moonlight.exe")
    )

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    throw "Unable to find Moonlight.exe. Pass -HelperPath or build the Tauri prototype first."
}

function New-BridgeRequest {
    param(
        [int]$Id,
        [string]$Name,
        [hashtable]$Payload = @{}
    )

    return @{
        id = $Id
        command = @{
            name = $Name
            payload = $Payload
        }
    } | ConvertTo-Json -Compress -Depth 8
}

function Read-BridgeResponse {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$ExpectedId,
        [int]$TimeoutSeconds
    )

    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $remainingMs = [Math]::Max(1, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
        $readTask = $Process.StandardOutput.ReadLineAsync()
        if (-not $readTask.Wait($remainingMs)) {
            break
        }

        $line = $readTask.Result
        if ($null -eq $line) {
            throw "Native helper stdout closed before response $ExpectedId."
        }
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }

        $frame = $line | ConvertFrom-Json
        if ($frame.PSObject.Properties.Name -contains "event") {
            Write-Host "event: $($frame.event.kind) - $($frame.event.message)"
            continue
        }
        if ($frame.id -ne $ExpectedId) {
            Write-Host "skipping response for id $($frame.id)"
            continue
        }

        return $frame
    }

    throw "Timed out waiting for native helper response $ExpectedId."
}

function Send-BridgeRequest {
    param(
        [System.Diagnostics.Process]$Process,
        [string]$Request,
        [int]$ExpectedId,
        [int]$TimeoutSeconds
    )

    Write-Host "request: $Request"
    $Process.StandardInput.WriteLine($Request)
    $Process.StandardInput.Flush()
    return Read-BridgeResponse -Process $Process -ExpectedId $ExpectedId -TimeoutSeconds $TimeoutSeconds
}

$resolvedHelperPath = Resolve-HelperPath -RequestedPath $HelperPath
Write-Host "Using helper: $resolvedHelperPath"

$startInfo = [System.Diagnostics.ProcessStartInfo]::new()
$startInfo.FileName = $resolvedHelperPath
$startInfo.Arguments = "--tauri-bridge-helper"
$startInfo.UseShellExecute = $false
$startInfo.RedirectStandardInput = $true
$startInfo.RedirectStandardOutput = $true
$startInfo.RedirectStandardError = $false
$startInfo.CreateNoWindow = $true

$process = [System.Diagnostics.Process]::new()
$process.StartInfo = $startInfo

try {
    if (-not $process.Start()) {
        throw "Failed to start native helper."
    }

    $listHosts = Send-BridgeRequest `
        -Process $process `
        -Request (New-BridgeRequest -Id 1 -Name "list_hosts") `
        -ExpectedId 1 `
        -TimeoutSeconds $TimeoutSeconds

    if ($listHosts.PSObject.Properties.Name -contains "error") {
        if ($listHosts.error -eq "Bridge command name is required.") {
            throw "list_hosts failed with a protocol mismatch. Rebuild the native helper so it supports the current Tauri name/payload IPC envelope."
        }
        throw "list_hosts failed: $($listHosts.error)"
    }
    if (-not ($listHosts.PSObject.Properties.Name -contains "result") -or $null -eq $listHosts.result) {
        throw "list_hosts did not return a result array."
    }
    Write-Host "list_hosts OK; count=$($listHosts.result.Count)"

    $invalidBitrate = Send-BridgeRequest `
        -Process $process `
        -Request (New-BridgeRequest -Id 2 -Name "default_bitrate" -Payload @{ width = 1; height = 1; fps = 1; yuv444 = $false }) `
        -ExpectedId 2 `
        -TimeoutSeconds $TimeoutSeconds

    if (-not ($invalidBitrate.PSObject.Properties.Name -contains "error")) {
        throw "default_bitrate validation did not return an error."
    }
    Write-Host "validation OK; error=$($invalidBitrate.error)"
    Write-Host "Tauri helper IPC smoke check passed."
}
finally {
    if (-not $process.HasExited) {
        $process.Kill()
    }
    $process.WaitForExit()
}
