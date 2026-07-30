[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $PackageRoot,

    [string] $ArtifactsRoot,

    [string] $SmokeRoot,

    [ValidateRange(5, 300)]
    [int] $TimeoutSeconds = 45
)

$ErrorActionPreference = "Stop"

function Get-FullPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    return [System.IO.Path]::GetFullPath($Path)
}

function Assert-UnderDirectory {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $Directory
    )

    $fullPath = Get-FullPath -Path $Path
    $fullDirectory = (Get-FullPath -Path $Directory).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)
    $directoryPrefix = $fullDirectory + [System.IO.Path]::DirectorySeparatorChar
    if (-not $fullPath.StartsWith($directoryPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to use a package smoke path outside artifacts: $fullPath"
    }

    return $fullPath
}

function Remove-SmokeDirectorySafe {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    $safePath = Assert-UnderDirectory -Path $Path -Directory $ArtifactsRoot
    if (Test-Path -LiteralPath $safePath) {
        Remove-Item -LiteralPath $safePath -Recurse -Force
    }
}

function Test-FixedNtfsPath {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    try {
        $fullPath = Get-FullPath -Path $Path
        if ($fullPath.StartsWith(
                "\\\\",
                [System.StringComparison]::Ordinal)) {
            return $false
        }

        $driveRoot = [System.IO.Path]::GetPathRoot($fullPath)
        if ([string]::IsNullOrWhiteSpace($driveRoot)) {
            return $false
        }

        $drive = [System.IO.DriveInfo]::new($driveRoot)
        return $drive.IsReady -and
            [int]$drive.DriveType -eq 3 -and
            [string]::Equals(
                $drive.DriveFormat,
                "NTFS",
                [System.StringComparison]::OrdinalIgnoreCase)
    }
    catch {
        return $false
    }
}

function Get-NativeSmokeTemporaryParent {
    $temporaryBase = Get-FullPath -Path ([System.IO.Path]::GetTempPath())
    if (-not (Test-FixedNtfsPath -Path $temporaryBase)) {
        throw "The current-user temporary directory is not on a fixed local NTFS volume."
    }

    $temporaryParent = Join-Path $temporaryBase "ReadOS.PackageSmoke.Native"
    New-Item -ItemType Directory -Force -Path $temporaryParent | Out-Null
    if (-not (Test-FixedNtfsPath -Path $temporaryParent)) {
        throw "The native package smoke temporary parent is not on a fixed local NTFS volume."
    }
    if (((Get-Item -LiteralPath $temporaryParent -Force).Attributes -band
            [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The native package smoke temporary parent must not be a reparse point."
    }

    return Get-FullPath -Path $temporaryParent
}

function Remove-NativeSmokeDirectorySafe {
    param(
        [Parameter(Mandatory)]
        [string] $Path,

        [Parameter(Mandatory)]
        [string] $TemporaryParent
    )

    $fullPath = Get-FullPath -Path $Path
    $fullTemporaryParent = (Get-FullPath -Path $TemporaryParent).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar,
        [System.IO.Path]::AltDirectorySeparatorChar)
    $temporaryPrefix = $fullTemporaryParent + [System.IO.Path]::DirectorySeparatorChar
    if ($fullPath -eq $fullTemporaryParent -or
        -not $fullPath.StartsWith(
            $temporaryPrefix,
            [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove a native package smoke directory outside its dedicated temporary parent."
    }

    if (-not (Test-FixedNtfsPath -Path $fullPath)) {
        throw "Refusing to remove a native package smoke directory outside a fixed local NTFS volume."
    }

    if (Test-Path -LiteralPath $fullPath) {
        Remove-Item -LiteralPath $fullPath -Recurse -Force
    }
}

function Get-SmokeDiagnostics {
    param(
        [Parameter(Mandatory)]
        [string] $LogPath
    )

    if (-not (Test-Path -LiteralPath $LogPath -PathType Leaf)) {
        return "No application diagnostic log was written."
    }

    try {
        return Get-Content -LiteralPath $LogPath -Raw
    }
    catch {
        return "The application diagnostic log could not be read: $($_.Exception.Message)"
    }
}

$repoRoot = Get-FullPath -Path (Join-Path $PSScriptRoot "..")
if ([string]::IsNullOrWhiteSpace($ArtifactsRoot)) {
    $ArtifactsRoot = Join-Path $repoRoot "artifacts"
}

$ArtifactsRoot = Get-FullPath -Path $ArtifactsRoot
$PackageRoot = Assert-UnderDirectory -Path $PackageRoot -Directory $ArtifactsRoot
if (-not (Test-Path -LiteralPath $PackageRoot -PathType Container)) {
    throw "Windows package directory was not found: $PackageRoot"
}

if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
    $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
    $SmokeRoot = Join-Path $ArtifactsRoot "smoke\windows-package-$stamp-$PID"
}

$SmokeRoot = Assert-UnderDirectory -Path $SmokeRoot -Directory $ArtifactsRoot
$executablePath = Join-Path $PackageRoot "ReadOS.App.exe"
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "Packaged ReadOS executable was not found: $executablePath"
}

Remove-SmokeDirectorySafe -Path $SmokeRoot
New-Item -ItemType Directory -Force -Path $SmokeRoot | Out-Null

$markerPath = Join-Path $SmokeRoot "reados-package-smoke.success.json"
$logPath = Join-Path $SmokeRoot "reados-package-smoke.log.json"
$nativeTemporaryParent = Get-NativeSmokeTemporaryParent
$nativeSmokeRunRoot = Join-Path `
    $nativeTemporaryParent `
    ("run-{0}-{1}" -f $PID, [Guid]::NewGuid().ToString("N"))
$nativeWorkspaceRoot = Join-Path $nativeSmokeRunRoot "workspace"
$quotedSmokeRoot = '"' + $SmokeRoot + '"'
$quotedNativeWorkspaceRoot = '"' + $nativeWorkspaceRoot + '"'
$process = $null
$timedOut = $false
$exitCode = $null

try {
    New-Item -ItemType Directory -Path $nativeWorkspaceRoot | Out-Null
    if (-not (Test-FixedNtfsPath -Path $nativeWorkspaceRoot)) {
        throw "The native package smoke workspace is not on a fixed local NTFS volume."
    }
    if (((Get-Item -LiteralPath $nativeSmokeRunRoot -Force).Attributes -band
            [System.IO.FileAttributes]::ReparsePoint) -ne 0 -or
        ((Get-Item -LiteralPath $nativeWorkspaceRoot -Force).Attributes -band
            [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "The native package smoke workspace must not contain a reparse-point boundary."
    }

    Write-Host "Running hidden Windows package startup smoke..."
    Write-Host "  Package: $PackageRoot"
    Write-Host "  Isolated state: $SmokeRoot"
    Write-Host "  Native fixture: fixed local NTFS temporary workspace"

    $process = Start-Process `
        -FilePath $executablePath `
        -ArgumentList @(
            "--package-smoke",
            $quotedSmokeRoot,
            "--package-smoke-native-workspace",
            $quotedNativeWorkspaceRoot) `
        -WorkingDirectory $PackageRoot `
        -WindowStyle Hidden `
        -PassThru

    if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
        $timedOut = $true
        try {
            $process.Kill($true)
        }
        catch {
            Write-Warning "Failed to terminate timed-out package smoke process $($process.Id): $($_.Exception.Message)"
        }

        [void]$process.WaitForExit(5000)
    }

    if (-not $timedOut) {
        $exitCode = $process.ExitCode
    }
}
finally {
    try {
        if ($null -ne $process) {
            try {
                if (-not $process.HasExited) {
                    $process.Kill($true)
                    [void]$process.WaitForExit(5000)
                }
            }
            finally {
                $process.Dispose()
            }
        }
    }
    finally {
        Remove-NativeSmokeDirectorySafe `
            -Path $nativeSmokeRunRoot `
            -TemporaryParent $nativeTemporaryParent
    }
}

if ($timedOut) {
    $diagnostics = Get-SmokeDiagnostics -LogPath $logPath
    throw "Windows package smoke timed out after $TimeoutSeconds seconds.`nDiagnostics: $logPath`n$diagnostics"
}

if ($exitCode -ne 0) {
    $diagnostics = Get-SmokeDiagnostics -LogPath $logPath
    throw "Windows package smoke failed with exit code $exitCode.`nDiagnostics: $logPath`n$diagnostics"
}

if (-not (Test-Path -LiteralPath $markerPath -PathType Leaf)) {
    $diagnostics = Get-SmokeDiagnostics -LogPath $logPath
    throw "Windows package smoke exited successfully without its success marker.`nDiagnostics: $logPath`n$diagnostics"
}

try {
    $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
}
catch {
    throw "Windows package smoke marker is not valid JSON: $markerPath`n$($_.Exception.Message)"
}

if ($marker.schemaVersion -ne 1 -or $marker.status -ne "ready") {
    throw "Windows package smoke marker reported an unexpected result: $(Get-Content -LiteralPath $markerPath -Raw)"
}

try {
    $logJson = Get-Content -LiteralPath $logPath -Raw
    $log = $logJson | ConvertFrom-Json
}
catch {
    throw "Windows package smoke diagnostic log is not valid JSON: $logPath`n$($_.Exception.Message)"
}

if ($logJson.Contains(
        "providerApiKey",
        [System.StringComparison]::OrdinalIgnoreCase) -or
    $logJson.Contains(
        $nativeWorkspaceRoot,
        [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Windows package smoke diagnostic log exposed a credential field or native temporary path: $logPath"
}

if ($log.schemaVersion -ne 1 -or $log.status -ne "ready") {
    throw "Windows package smoke diagnostic log reported an unexpected result: $(Get-Content -LiteralPath $logPath -Raw)"
}
try {
    if ($log.nativeAbiCapabilities -notmatch '^0x[0-9A-Fa-f]{16}$') {
        throw "invalid capability format"
    }
    $nativeAbiCapabilities = [Convert]::ToUInt64(
        $log.nativeAbiCapabilities.Substring(2),
        16)
}
catch {
    throw "Packaged Hosting reported invalid native MSP ABI capabilities: $($log.nativeAbiCapabilities)"
}

if ($log.nativeAbiMode -ne "LengthDelimitedV2" -or
    $log.nativeAbiMajor -ne 2 -or
    $log.nativeAbiMinor -ne 0 -or
    $log.nativeAbiContractId -ne "0x324D534F44414552" -or
    ($nativeAbiCapabilities -band [UInt64]0xF) -ne [UInt64]0xF) {
    throw "Packaged Hosting did not report the required native MSP ABI v2 handshake: $(Get-Content -LiteralPath $logPath -Raw)"
}

Write-Host "Windows package startup smoke passed."
Write-Host "  Marker:      $markerPath"
Write-Host "  Diagnostics: $logPath"
Write-Host "  Native ABI:  $($log.nativeAbiMode) $($log.nativeAbiMajor).$($log.nativeAbiMinor)"
