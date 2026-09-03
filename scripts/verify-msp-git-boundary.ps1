[CmdletBinding()]
param(
    [string] $RepositoryRoot
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
}
else {
    $RepositoryRoot = [System.IO.Path]::GetFullPath($RepositoryRoot)
}

if (-not (Test-Path -LiteralPath $RepositoryRoot -PathType Container)) {
    throw "Repository root was not found: $RepositoryRoot"
}

Push-Location $RepositoryRoot
try {
    $trackedMsp = @(git ls-files -- MSP)
    if ($LASTEXITCODE -ne 0) {
        throw "Could not inspect Git tracking state for MSP/."
    }
    if ($trackedMsp.Count -gt 0) {
        throw "Raw MSP paths are tracked by Git: $($trackedMsp -join ', ')"
    }
}
finally {
    Pop-Location
}

Write-Host "Raw MSP Git boundary validation passed: MSP/ is not tracked."
