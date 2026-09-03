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

$validatorPath = Join-Path $RepositoryRoot "scripts\verify-msp-git-boundary.ps1"
if (-not (Test-Path -LiteralPath $validatorPath -PathType Leaf)) {
    throw "Git boundary validator was not found: $validatorPath"
}

& $validatorPath -RepositoryRoot $RepositoryRoot
if ($LASTEXITCODE -ne 0) {
    throw "The clean Git boundary validation failed."
}

$fakeRepositoryRoot = Join-Path ([System.IO.Path]::GetTempPath()) "reados-fake-repo-$PID"
New-Item -ItemType Directory -Force -Path $fakeRepositoryRoot | Out-Null
try {
    & git -C $fakeRepositoryRoot init --quiet
    if ($LASTEXITCODE -ne 0) {
        throw "Could not initialize the temporary Git repository."
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $fakeRepositoryRoot "MSP") | Out-Null
    Set-Content -LiteralPath (Join-Path $fakeRepositoryRoot "MSP\README.md") -Value "raw" -NoNewline
    & git -C $fakeRepositoryRoot add -- MSP\README.md
    if ($LASTEXITCODE -ne 0) {
        throw "Could not stage the simulated raw MSP path."
    }

    $failed = $false
    try {
        & $validatorPath -RepositoryRoot $fakeRepositoryRoot
    }
    catch {
        $failed = $true
    }
    if (-not $failed) {
        throw "The Git boundary validator accepted a tracked MSP path."
    }
}
finally {
    if (Test-Path -LiteralPath $fakeRepositoryRoot) {
        Remove-Item -LiteralPath $fakeRepositoryRoot -Recurse -Force
    }
}

$workflowPath = Join-Path $RepositoryRoot ".github\workflows\windows-ci.yml"
$workflow = Get-Content -LiteralPath $workflowPath -Raw
$boundaryStep = [regex]::Match(
    $workflow,
    '(?ms)^      - name: Verify raw MSP Git boundary\r?\n(?<body>.*?)(?=^      - name:)')
if (-not $boundaryStep.Success) {
    throw "The Windows workflow must run the raw MSP Git boundary step."
}
if ($boundaryStep.Groups["body"].Value -match '(?m)^        if:') {
    throw "The raw MSP Git boundary step must be unconditional."
}
if ($boundaryStep.Groups["body"].Value -notmatch 'verify-msp-git-boundary\.ps1') {
    throw "The raw MSP Git boundary step must invoke its dedicated validator."
}

Write-Host "Focused raw MSP Git boundary tests passed."
