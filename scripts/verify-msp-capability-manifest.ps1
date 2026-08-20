[CmdletBinding()]
param(
    [string] $ManifestPath
)

$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($scriptRoot)) {
    $scriptRoot = (Get-Location).Path
}
$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot ".."))
if ([string]::IsNullOrWhiteSpace($ManifestPath)) {
    $ManifestPath = Join-Path $repoRoot "conformance\msp-upstream\windows-capability-manifest.json"
}
$manifestFullPath = [System.IO.Path]::GetFullPath($ManifestPath)
if (-not (Test-Path -LiteralPath $manifestFullPath -PathType Leaf)) {
    throw "Capability manifest was not found: $manifestFullPath"
}

function Assert-RelativeManifestPath {
    param(
        [Parameter(Mandatory)]
        [string] $RelativePath,

        [Parameter(Mandatory)]
        [bool] $ShouldExist
    )

    if ([string]::IsNullOrWhiteSpace($RelativePath)) {
        throw "Manifest contains an empty path."
    }

    if ([System.IO.Path]::IsPathRooted($RelativePath) -or
        $RelativePath -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "Manifest path is not repository-relative: $RelativePath"
    }

    $normalized = $RelativePath.Replace('/', [System.IO.Path]::DirectorySeparatorChar)
    $candidate = Join-Path $repoRoot $normalized
    $hasWildcard = $RelativePath.IndexOfAny([char[]] '*?[') -ge 0
    if ($hasWildcard) {
        $exists = @(Get-ChildItem -Path $candidate -Force -ErrorAction SilentlyContinue).Count -gt 0
    }
    else {
        $exists = Test-Path -LiteralPath $candidate
    }

    if ($ShouldExist -and -not $exists) {
        throw "Manifest claims a path exists, but it is absent: $RelativePath"
    }

    if (-not $ShouldExist -and $exists) {
        throw "Manifest claims a path is missing, but it exists: $RelativePath"
    }
}

function Assert-NonEmptyString {
    param(
        [Parameter(Mandatory)]
        [string] $Value,

        [Parameter(Mandatory)]
        [string] $Field
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "Manifest field is empty: $Field"
    }
}

$json = Get-Content -LiteralPath $manifestFullPath -Raw | ConvertFrom-Json
Assert-NonEmptyString -Value $json.manifest_kind -Field "manifest_kind"
Assert-NonEmptyString -Value $json.profile -Field "profile"

$allowedStatuses = @("conformant", "partial", "deferred", "blocked", "not_applicable")
$allowedEvidenceStates = @("available", "blocked", "not_applicable")
if ($allowedStatuses -notcontains [string] $json.status) {
    throw "Unknown manifest status: $($json.status)"
}
if ([string] $json.status -ne [string] $json.validation.current_expected_result) {
    throw "Manifest status does not match validation.current_expected_result."
}

if ([string] $json.provenance.reference_root -ne "MSP/") {
    throw "The manifest must identify MSP/ as its reference root."
}
if (-not [bool] $json.provenance.raw_reference_must_not_be_tracked_or_packaged) {
    throw "The manifest must prohibit raw MSP tracking and packaging."
}
foreach ($licensePath in @($json.provenance.license_paths)) {
    Assert-RelativeManifestPath -RelativePath ([string] $licensePath) -ShouldExist $true
}
Assert-RelativeManifestPath -RelativePath ([string] $json.provenance.reados_derivative_provenance) -ShouldExist $true

$sourceInventory = $json.source_inventory
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.source_package_readme) -ShouldExist $true
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.windows_workspace_manifest) -ShouldExist $true
$workspaceText = Get-Content -LiteralPath (Join-Path $repoRoot ([string] $sourceInventory.windows_workspace_manifest)) -Raw
$workspaceMemberMatches = [regex]::Matches($workspaceText, '(?m)^\s*"([^"]+)"\s*,?\s*$')
$actualWorkspaceMembers = @(
    $workspaceMemberMatches | ForEach-Object {
        ("MSP/Implementations/Windows/" + [string] $_.Groups[1].Value).Replace('\\', '/')
    }
)
$manifestWorkspaceMembers = @(
    @($sourceInventory.workspace_members) | ForEach-Object {
        ([string] $_.path).Replace('\\', '/')
    }
)
if ($actualWorkspaceMembers.Count -eq 0) {
    throw "The Windows Cargo workspace has no parseable member entries."
}
$missingWorkspaceMembers = @(
    $actualWorkspaceMembers | Where-Object { $manifestWorkspaceMembers -notcontains $_ }
)
$unexpectedWorkspaceMembers = @(
    $manifestWorkspaceMembers | Where-Object { $actualWorkspaceMembers -notcontains $_ }
)
if ($missingWorkspaceMembers.Count -gt 0 -or $unexpectedWorkspaceMembers.Count -gt 0) {
    throw "Manifest workspace members do not match MSP/Implementations/Windows/Cargo.toml. Missing: $($missingWorkspaceMembers -join ', '); unexpected: $($unexpectedWorkspaceMembers -join ', ')"
}
if ([int] $sourceInventory.workspace_member_count -ne @($sourceInventory.workspace_members).Count) {
    throw "The declared workspace member count does not match the inventory."
}
if ([int] $sourceInventory.workspace_crate_count + [int] $sourceInventory.workspace_example_count -ne [int] $sourceInventory.workspace_member_count) {
    throw "The workspace crate/example counts do not add up."
}

$rowIds = @(
    @($json.components) | ForEach-Object { [string] $_.id }
    @($json.capabilities) | ForEach-Object { [string] $_.id }
)
foreach ($member in @($sourceInventory.workspace_members)) {
    Assert-NonEmptyString -Value ([string] $member.name) -Field "source_inventory.workspace_members.name"
    Assert-NonEmptyString -Value ([string] $member.represented_by) -Field "source_inventory.workspace_members.represented_by"
    Assert-RelativeManifestPath -RelativePath ([string] $member.path) -ShouldExist $true
    Assert-RelativeManifestPath -RelativePath (Join-Path ([string] $member.path) "Cargo.toml") -ShouldExist $true
    if ($rowIds -notcontains [string] $member.represented_by) {
        throw "Workspace member has no component mapping: $($member.name) -> $($member.represented_by)"
    }
}
foreach ($nonMember in @($sourceInventory.non_workspace_directories)) {
    Assert-RelativeManifestPath -RelativePath ([string] $nonMember.path) -ShouldExist $true
    Assert-RelativeManifestPath -RelativePath (Join-Path ([string] $nonMember.path) "Cargo.toml") -ShouldExist $false
}

$swiftSurface = $sourceInventory.swift_source_surface
foreach ($path in @($swiftSurface.present_paths)) {
    Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $true
}
foreach ($path in @($swiftSurface.excluded_paths)) {
    Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $false
}
Assert-RelativeManifestPath -RelativePath ([string] $swiftSurface.exclusion_evidence) -ShouldExist $true

$requiredBlockedDrift = @(
    "swift-source-package-omission",
    "inventory-runner-and-report-omission",
    "release-runner-dependency-omission",
    "windows-implementation-document-drift",
    "reference-revision-metadata-omission",
    "windows-conformance-test-placeholders",
    "windows-package-document-claims"
)
$driftById = @{}
foreach ($drift in @($json.source_drift)) {
    $driftById[[string] $drift.id] = $drift
    if ([string] $drift.status -ne "blocked") {
        throw "Source/document drift must remain blocked until its evidence is verified: $($drift.id)"
    }
    foreach ($path in @($drift.observed_paths)) {
        if ($null -eq $path) { continue }
        Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $true
    }
    foreach ($path in @($drift.missing_paths)) {
        if ($null -eq $path) { continue }
        Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $false
    }
    Assert-NonEmptyString -Value ([string] $drift.evidence_command) -Field "source_drift.evidence_command"
}
foreach ($requiredId in $requiredBlockedDrift) {
    if (-not $driftById.ContainsKey($requiredId)) {
        throw "Required source/document drift record is missing: $requiredId"
    }
}

if ([string] $sourceInventory.inventory_runner.status -ne "blocked" -or
    [string] $sourceInventory.release_runner.status -ne "blocked") {
    throw "Missing inventory/release runners must remain blocked."
}
foreach ($path in @($sourceInventory.inventory_runner.missing_paths)) {
    if ($null -eq $path) { continue }
    Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $false
}
foreach ($path in @($sourceInventory.release_runner.missing_paths)) {
    if ($null -eq $path) { continue }
    Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $false
}
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.inventory_runner.runner_path) -ShouldExist $false
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.inventory_runner.report_path) -ShouldExist $false
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.release_runner.wrapper_path) -ShouldExist $true
Assert-RelativeManifestPath -RelativePath ([string] $sourceInventory.release_runner.contract_path) -ShouldExist $true

foreach ($sectionName in @("components", "capabilities")) {
    foreach ($row in @($json.$sectionName)) {
        Assert-NonEmptyString -Value ([string] $row.id) -Field "$sectionName.id"
        Assert-NonEmptyString -Value ([string] $row.owner) -Field "$sectionName.owner"
        if ($allowedStatuses -notcontains [string] $row.status) {
            throw "Unknown $sectionName status for $($row.id): $($row.status)"
        }
        if ($null -eq $row.dependencies) {
            throw "Missing dependencies for $sectionName row: $($row.id)"
        }
        $evidence = $row.evidence
        if ($null -eq $evidence) {
            throw "Missing evidence object for $sectionName row: $($row.id)"
        }
        Assert-NonEmptyString -Value ([string] $evidence.state) -Field "$sectionName.evidence.state"
        if ($allowedEvidenceStates -notcontains [string] $evidence.state) {
            throw "Unknown evidence state for $sectionName row $($row.id): $($evidence.state)"
        }
        $evidenceCommands = @()
        if ($null -ne $evidence.commands) {
            $evidenceCommands = @($evidence.commands)
        }
        if ($evidenceCommands.Count -eq 0) {
            throw "Every $sectionName row needs an evidence command or explicit not-applicable command: $($row.id)"
        }
        foreach ($target in @($row.reados_targets)) {
            if ($null -eq $target) { continue }
            if ([string] $target -like "MSP/*") {
                throw "A ReadOS target may not point into raw MSP/: $($row.id)"
            }
            Assert-RelativeManifestPath -RelativePath ([string] $target) -ShouldExist $true
        }
        foreach ($path in @($evidence.paths)) {
            if ($null -eq $path) { continue }
            Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $true
        }
        $evidenceMissingPaths = @()
        if ($null -ne $evidence.missing_paths) {
            $evidenceMissingPaths = @($evidence.missing_paths)
        }
        foreach ($path in $evidenceMissingPaths) {
            if ($null -eq $path) { continue }
            Assert-RelativeManifestPath -RelativePath ([string] $path) -ShouldExist $false
        }
        if ([string] $evidence.state -eq "available" -and $evidenceMissingPaths.Count -gt 0) {
            throw "Available evidence cannot declare missing paths: $($row.id)"
        }
        if ([string] $evidence.state -eq "blocked" -and $evidenceMissingPaths.Count -eq 0) {
            throw "Blocked evidence must declare at least one missing path: $($row.id)"
        }
    }
}

Push-Location $repoRoot
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

$componentCount = @($json.components).Count
$capabilityCount = @($json.capabilities).Count
Write-Host "MSP capability manifest validation passed."
Write-Host "  Profile: $($json.profile)"
Write-Host "  Status: $($json.status) (blocked external inventory/release evidence is recorded)"
Write-Host "  Components: $componentCount; capabilities: $capabilityCount; workspace members: $($sourceInventory.workspace_member_count)"
