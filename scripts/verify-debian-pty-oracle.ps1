[CmdletBinding()]
param(
    [string] $EvidencePath,
    [switch] $Json,
    [switch] $RequireAvailable
)

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($scriptRoot)) {
    $scriptRoot = (Get-Location).Path
}
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($EvidencePath)) {
    $EvidencePath = Join-Path $repoRoot "conformance\msp-upstream\debian-pty-oracle-evidence.json"
}
$evidenceFullPath = [System.IO.Path]::GetFullPath($EvidencePath)

function Add-Unique {
    param(
        [Parameter(Mandatory)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[string]] $List,

        [Parameter(Mandatory)]
        [string] $Value
    )

    if (-not [string]::IsNullOrWhiteSpace($Value) -and -not $List.Contains($Value)) {
        $List.Add($Value)
    }
}

function Assert-RelativePath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,

        [Parameter(Mandatory)]
        [string] $Field
    )

    if ([string]::IsNullOrWhiteSpace($RelativePath)) {
        throw "Evidence field is empty: $Field"
    }
    if ([System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "Evidence path is not repository-relative: $Field=$RelativePath"
    }
}

function Resolve-RepoPath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath
    )

    Assert-RelativePath -RelativePath $RelativePath -Field "path"
    return Join-Path $repoRoot ($RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar))
}

function Test-RepoPath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath
    )

    return Test-Path -LiteralPath (Resolve-RepoPath -RelativePath $RelativePath)
}

function Get-CommandName {
    param(
        [Parameter(Mandatory)]
        [string] $Command
    )

    $trimmed = $Command.Trim()
    if ($trimmed -match '^([^\s]+)') {
        return [string] $Matches[1]
    }
    return $trimmed
}

function Test-CommandAvailable {
    param(
        [Parameter(Mandatory)]
        [string] $Command
    )

    $name = Get-CommandName -Command $Command
    if ([string]::IsNullOrWhiteSpace($name)) {
        return $false
    }
    return $null -ne (Get-Command -Name $name -ErrorAction SilentlyContinue)
}

if (-not (Test-Path -LiteralPath $evidenceFullPath -PathType Leaf)) {
    throw "Debian PTY oracle evidence record was not found: $evidenceFullPath"
}

$evidence = Get-Content -LiteralPath $evidenceFullPath -Raw | ConvertFrom-Json
$missingPaths = New-Object 'System.Collections.Generic.List[string]'
$missingCommands = New-Object 'System.Collections.Generic.List[string]'
$notes = New-Object 'System.Collections.Generic.List[string]'

if ([int] $evidence.schema_version -ne 1) {
    throw "Unsupported Debian PTY oracle evidence schema: $($evidence.schema_version)"
}
if ([string] $evidence.evidence_kind -ne "msp-debian12-pty-oracle-availability") {
    throw "Unexpected Debian PTY oracle evidence kind: $($evidence.evidence_kind)"
}

$fixture = $evidence.fixture
$fixturePath = [string] $fixture.path
$fixtureFullPath = Resolve-RepoPath -RelativePath $fixturePath
$fixtureObserved = [ordered]@{
    path = $fixturePath
    status = "missing"
    case_count = $null
    artifact_kind = $null
    sha256 = $null
}

if (-not (Test-Path -LiteralPath $fixtureFullPath -PathType Leaf)) {
    Add-Unique -List $missingPaths -Value $fixturePath
}
else {
    try {
        $fixtureJson = Get-Content -LiteralPath $fixtureFullPath -Raw | ConvertFrom-Json
        $fixtureObserved.status = "available"
        $fixtureObserved.artifact_kind = [string] $fixtureJson.artifact_kind
        $fixtureObserved.case_count = @($fixtureJson.cases).Count
        $fixtureObserved.sha256 = (Get-FileHash -LiteralPath $fixtureFullPath -Algorithm SHA256).Hash.ToLowerInvariant()

        if ($fixtureObserved.artifact_kind -ne [string] $fixture.artifact_kind) {
            $notes.Add("Fixture artifact_kind does not match the evidence record.")
        }
        if ([int] $fixtureObserved.case_count -ne [int] $fixture.expected_case_count) {
            $notes.Add("Fixture case count does not match the evidence record.")
        }
        if ([string] $fixtureObserved.sha256 -ne ([string] $fixture.sha256).ToLowerInvariant()) {
            $notes.Add("Fixture SHA-256 does not match the evidence record.")
        }
        $caseIndex = 0
        foreach ($case in @($fixtureJson.cases)) {
            $caseIndex++
            if ($null -eq $case.expected -or
                [string]::IsNullOrWhiteSpace([string] $case.expected.stream_b64)) {
                $notes.Add("Fixture case $caseIndex has no expected.stream_b64 field.")
            }
        }
    }
    catch {
        Add-Unique -List $missingPaths -Value $fixturePath
        $notes.Add("Fixture could not be parsed as JSON: $($_.Exception.Message)")
    }
}

$requiredRunnerPaths = @($evidence.runner.required_paths)
$runnerObserved = New-Object 'System.Collections.Generic.List[object]'
foreach ($path in $requiredRunnerPaths) {
    $pathText = [string] $path
    $exists = Test-RepoPath -RelativePath $pathText
    $runnerObserved.Add([ordered]@{ path = $pathText; exists = $exists })
    if (-not $exists) {
        Add-Unique -List $missingPaths -Value $pathText
    }
}

$reportPath = [string] $evidence.runner.report_path
if (-not (Test-RepoPath -RelativePath $reportPath)) {
    Add-Unique -List $missingPaths -Value $reportPath
}

$hostProbeCommands = @($evidence.execution_host.availability_checks)
$hostObserved = New-Object 'System.Collections.Generic.List[object]'
foreach ($command in $hostProbeCommands) {
    $commandText = [string] $command
    $available = Test-CommandAvailable -Command $commandText
    $hostObserved.Add([ordered]@{ command = $commandText; executable_available = $available })
    if (-not $available -and $commandText -notlike "wsl.exe -d Debian*") {
        Add-Unique -List $missingCommands -Value $commandText
    }
}

$wslCommand = Get-Command -Name "wsl.exe" -ErrorAction SilentlyContinue
$debianHostProven = $false
if ($null -ne $wslCommand) {
    $wslList = (& $wslCommand.Source --list --quiet 2>$null | Out-String) -replace "`0", ""
    if ($wslList -match '(?im)^\s*debian(?:\s|$)') {
        $osRelease = (& $wslCommand.Source -d Debian -- cat /etc/os-release 2>$null | Out-String) -replace "`0", ""
        if ($osRelease -match '(?im)^\s*VERSION_ID\s*=\s*["'']?12["'']?\s*$' -and
            $osRelease -match '(?im)^\s*ID\s*=\s*["'']?debian["'']?\s*$') {
            $debianHostProven = $true
        }
        else {
            Add-Unique -List $missingCommands -Value "wsl.exe -d Debian -- cat /etc/os-release"
        }
    }
    else {
        Add-Unique -List $missingCommands -Value "wsl.exe -d Debian -- cat /etc/os-release"
    }
}
else {
    Add-Unique -List $missingCommands -Value "wsl.exe -d Debian -- cat /etc/os-release"
}

$swiftAvailable = Test-CommandAvailable -Command "swift --version"
$containerRuntimeAvailable = (Test-CommandAvailable -Command "docker --version") -or
    (Test-CommandAvailable -Command "podman --version")
$customRunnerConfigured = -not [string]::IsNullOrWhiteSpace($env:MSP_DEBIAN12_PTY_ORACLE_RUNNER)
if ($customRunnerConfigured -and -not (Test-CommandAvailable -Command $env:MSP_DEBIAN12_PTY_ORACLE_RUNNER)) {
    Add-Unique -List $missingCommands -Value $env:MSP_DEBIAN12_PTY_ORACLE_RUNNER
}
if (-not $customRunnerConfigured -and -not $containerRuntimeAvailable -and -not $debianHostProven) {
    Add-Unique -List $missingCommands -Value "MSP_DEBIAN12_PTY_ORACLE_RUNNER=<custom-linux-runner>"
}

$declaredStatus = [string] $evidence.status
$allowedStatuses = @("available", "blocked")
if ($allowedStatuses -notcontains $declaredStatus) {
    throw "Unexpected evidence status: $declaredStatus"
}

$status = "available"
if ($missingPaths.Count -gt 0 -or $missingCommands.Count -gt 0 -or $notes.Count -gt 0 -or
    -not $swiftAvailable -or (-not $containerRuntimeAvailable -and -not $debianHostProven -and -not $customRunnerConfigured)) {
    $status = "blocked"
}
if ($declaredStatus -eq "blocked" -and $status -eq "available") {
    $notes.Add("The record declares blocked, so availability cannot be promoted without an updated evidence record.")
    $status = "blocked"
}

$result = [ordered]@{
    schema_version = 1
    evidence_kind = [string] $evidence.evidence_kind
    status = $status
    declared_status = $declaredStatus
    evidence_path = $evidenceFullPath
    fixture = $fixtureObserved
    runner = [ordered]@{
        required_paths = @($requiredRunnerPaths)
        observed_paths = $runnerObserved.ToArray()
        report_path = $reportPath
    }
    execution_host = [ordered]@{
        required = [bool] $evidence.execution_host.required
        debian12_host_proven = $debianHostProven
        custom_runner_configured = $customRunnerConfigured
        observed_commands = $hostObserved.ToArray()
    }
    missing_paths = $missingPaths.ToArray()
    missing_commands = $missingCommands.ToArray()
    notes = $notes.ToArray()
    expected_output_policy = [string] $evidence.expected_output_policy
}

if ($Json) {
    $result | ConvertTo-Json -Depth 10
}
else {
    Write-Output "Debian PTY oracle evidence status: $status"
    Write-Output "Fixture: $($fixtureObserved.status); cases=$($fixtureObserved.case_count); sha256=$($fixtureObserved.sha256)"
    if ($missingPaths.Count -gt 0) {
        Write-Output "Missing paths:"
        foreach ($path in $missingPaths) {
            Write-Output "  $path"
        }
    }
    if ($missingCommands.Count -gt 0) {
        Write-Output "Missing commands or host probes:"
        foreach ($command in $missingCommands) {
            Write-Output "  $command"
        }
    }
    foreach ($note in $notes) {
        Write-Output "Note: $note"
    }
}

if ($RequireAvailable -and $status -ne "available") {
    exit 1
}
exit 0
