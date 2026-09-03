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

function Fail([string] $Message) {
    throw "MSP portable command profile verification failed: $Message"
}

function Assert-Set([object[]] $Actual, [object[]] $Expected, [string] $Label) {
    $actualSorted = @($Actual | ForEach-Object { [string] $_ } | Sort-Object -Unique)
    $expectedSorted = @($Expected | ForEach-Object { [string] $_ } | Sort-Object -Unique)
    $missing = @($expectedSorted | Where-Object { $actualSorted -notcontains $_ })
    $unexpected = @($actualSorted | Where-Object { $expectedSorted -notcontains $_ })
    if ($missing.Count -gt 0 -or $unexpected.Count -gt 0 -or $actualSorted.Count -ne $expectedSorted.Count) {
        Fail "$Label drifted. Missing: $($missing -join ', '); unexpected: $($unexpected -join ', ')."
    }
}

$profilePath = Join-Path $RepositoryRoot "native\msp-command-pack\profile\portable_msp_v1.json"
$fixturePath = Join-Path $RepositoryRoot "native\msp-command-pack\profile\portable_msp_v1_fixtures.json"
$rustPath = Join-Path $RepositoryRoot "native\msp-command-pack\src\lib.rs"
$ffiPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\src\lib.rs"
$cargoPath = Join-Path $RepositoryRoot "native\msp-command-pack\Cargo.toml"
$androidTestPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\android\src\androidTest\kotlin\com\reados\msp\commandruntime\MspPortableProfileInstrumentationTest.kt"
$androidGradlePath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\android\msp-command-runtime-ffi\build.gradle.kts"
$androidCmakePath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\android\msp-command-runtime-ffi\src\main\cpp\CMakeLists.txt"
$androidWorkflowPath = Join-Path $RepositoryRoot ".github\workflows\portable-rust-ci.yml"
$ffiBuildPath = Join-Path $RepositoryRoot "native\msp-command-runtime-ffi\build.rs"

foreach ($path in @($profilePath, $fixturePath, $rustPath, $ffiPath, $cargoPath, $androidTestPath, $androidGradlePath, $androidCmakePath, $androidWorkflowPath, $ffiBuildPath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        Fail "required profile input is missing: $path"
    }
}

$profile = Get-Content -Raw -LiteralPath $profilePath | ConvertFrom-Json
$fixtures = Get-Content -Raw -LiteralPath $fixturePath | ConvertFrom-Json
$rust = Get-Content -Raw -LiteralPath $rustPath
$ffi = Get-Content -Raw -LiteralPath $ffiPath
$cargo = Get-Content -Raw -LiteralPath $cargoPath
$androidTest = Get-Content -Raw -LiteralPath $androidTestPath
$androidGradle = Get-Content -Raw -LiteralPath $androidGradlePath
$androidCmake = Get-Content -Raw -LiteralPath $androidCmakePath
$androidWorkflow = Get-Content -Raw -LiteralPath $androidWorkflowPath
$ffiBuild = Get-Content -Raw -LiteralPath $ffiBuildPath

if ([string] $profile.profile -ne "reados-portable-msp-v1" -or
    [string] $fixtures.profile -ne [string] $profile.profile) {
    Fail "profile and fixture version names do not match."
}

$commands = @($profile.commands)
if ($commands.Count -ne 12) {
    Fail "portable v1 must contain exactly 12 commands; found $($commands.Count)."
}
$commandNames = @($commands | ForEach-Object { $_.name })
if (@($commandNames | Sort-Object -Unique).Count -ne $commandNames.Count) {
    Fail "portable profile contains duplicate command names."
}
foreach ($command in $commands) {
    foreach ($property in @("name", "options", "effects", "binary", "limits", "deviations")) {
        if ($null -eq $command.$property) {
            Fail "command '$($command.name)' is missing profile property '$property'."
        }
    }
}

$rustBlock = [regex]::Match($rust, '(?ms)pub const PORTABLE_MSP_V1_COMMANDS: &\[&str\] = &\[(?<body>.*?)\];')
if (-not $rustBlock.Success) {
    Fail "Rust portable profile command constant is missing."
}
$rustNames = @(
    [regex]::Matches($rustBlock.Groups["body"].Value, '"([a-z][a-z0-9._-]*)"') |
        ForEach-Object { $_.Groups[1].Value }
)
Assert-Set $rustNames $commandNames "Rust portable profile command names"

if ($ffi -notmatch 'Registry::with_portable_msp_v1\(\)') {
    Fail "the modular runtime FFI does not consume the frozen portable profile."
}
if ($cargo -notmatch '"profile/\*\*"') {
    Fail "the command-pack Cargo package does not include the profile manifest and fixtures."
}
foreach ($requiredText in @(
    @{ Text = $androidTest; Pattern = 'MspCommandRuntimeFfiClient\.withNative\(\)'; Label = 'Android instrumentation native client' },
    @{ Text = $androidTest; Pattern = 'portable_msp_v1_fixtures\.json'; Label = 'Android instrumentation canonical fixture asset' },
    @{ Text = $androidGradle; Pattern = 'sourceSets\["androidTest"\]\.assets\.srcDir'; Label = 'Android instrumentation fixture asset wiring' },
    @{ Text = $androidGradle; Pattern = 'androidTestImplementation\("androidx\.test:runner:'; Label = 'Android instrumentation runner dependency' },
    @{ Text = $androidGradle; Pattern = 'providers\.gradleProperty\("androidAbi"\)'; Label = 'Android test ABI override' },
    @{ Text = $androidCmake; Pattern = 'IMPORTED_SONAME "libmsp_command_runtime_ffi\.so"'; Label = 'Android Rust SONAME linkage' },
    @{ Text = $ffiBuild; Pattern = 'rustc-link-arg-cdylib=-Wl,-soname,libmsp_command_runtime_ffi\.so'; Label = 'Android Rust artifact SONAME' },
    @{ Text = $androidWorkflow; Pattern = 'compileDebugAndroidTestKotlin'; Label = 'CI Android instrumentation compile gate' }
)) {
    if ($requiredText.Text -notmatch $requiredText.Pattern) {
        Fail "$($requiredText.Label) is missing."
    }
}

$fixtureCases = @($fixtures.cases)
$caseIds = @($fixtureCases | ForEach-Object { $_.id })
if (@($caseIds | Sort-Object -Unique).Count -ne $caseIds.Count) {
    Fail "portable profile fixtures contain duplicate ids."
}
$excludedNames = @($profile.excluded_from_profile | ForEach-Object { $_.name })
$coveredPortableCommands = @(
    $fixtureCases |
        Where-Object { $commandNames -contains $_.command } |
        ForEach-Object { $_.command } |
        Sort-Object -Unique
)
Assert-Set $coveredPortableCommands $commandNames "portable profile fixture command coverage"
$unexpectedFixtureCommands = @(
    $fixtureCases |
        Where-Object {
            ($commandNames -notcontains $_.command) -and
            ($excludedNames -notcontains $_.command)
        } |
        ForEach-Object { $_.command } |
        Sort-Object -Unique
)
if ($unexpectedFixtureCommands.Count -gt 0) {
    Fail "portable profile fixtures contain commands outside the profile or explicit exclusions: $($unexpectedFixtureCommands -join ', ')."
}
if (@($fixtureCases | Where-Object { $_.expected.exitCode -eq 2 }).Count -lt 4) {
    Fail "portable profile fixtures need at least four deterministic unsupported-option cases."
}

$fixtureJson = Get-Content -Raw -LiteralPath $fixturePath
if ($fixtureJson -match 'C:\\\\|/Users/|/home/|PATH=') {
    Fail "portable profile fixtures contain a host path or environment disclosure token."
}

Write-Host "MSP portable command profile verified: reados-portable-msp-v1, $($commands.Count) commands, $($fixtureCases.Count) shared fixtures, deterministic unsupported-option coverage present."
