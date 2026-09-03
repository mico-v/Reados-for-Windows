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

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
if ($null -eq $cargoCommand) {
    throw "cargo was not found. Install Rust or add cargo.exe to PATH."
}

$expectations = @(
    @{
        Manifest = "native\msp-backend-linux\Cargo.toml"
        Files = @("README.md", "NOTICE", "LICENSE-APACHE-2.0", "src/lib.rs")
    },
    @{
        Manifest = "native\msp-process-backend\Cargo.toml"
        Files = @("README.md", "NOTICE", "LICENSE-APACHE-2.0", "src/lib.rs")
    },
    @{
        Manifest = "native\msp-git-provider\Cargo.toml"
        Files = @(
            "README.md",
            "NOTICE",
            "LICENSE-APACHE-2.0",
            "profile/git-provider-v1.json",
            "profile/provenance.json",
            "src/lib.rs"
        )
    },
    @{
        Manifest = "native\msp-command-pack\Cargo.toml"
        Files = @(
            "Cargo.toml",
            "README.md",
            "NOTICE",
            "LICENSE-APACHE-2.0",
            "src/lib.rs",
            "src/grep.rs",
            "src/safe_subset.rs"
        )
    },
    @{
        Manifest = "native\msp-command-runtime\Cargo.toml"
        Files = @("tests/runtime.rs")
    },
    @{
        Manifest = "native\msp-web-host\Cargo.toml"
        Files = @("README.md", "NOTICE", "LICENSE-APACHE-2.0", "src/lib.rs", "src/main.rs")
    },
    @{
        Manifest = "native\msp-kernel\Cargo.toml"
        Files = @("LICENSE-MIT")
    },
    @{
        Manifest = "native\msp-host-adapter\Cargo.toml"
        Files = @("LICENSE-MIT")
    },
    @{
        Manifest = "native\msp-protocol-windows\Cargo.toml"
        Files = @("LICENSE-APACHE-2.0")
    },
    @{
        Manifest = "native\msp-command-runtime-ffi\Cargo.toml"
        Files = @(
            "android/README.md",
            "android/build.gradle.kts",
            "android/gradle/wrapper/gradle-wrapper.jar",
            "android/provider/README.md",
            "android/provider/NOTICE",
            "android/provider/android-tool-provider-v1.json",
            "android/provider/provenance.json",
            "android/src/main/kotlin/com/reados/msp/commandruntime/MspAndroidToolProvider.kt",
            "android/src/test/kotlin/com/reados/msp/commandruntime/MspAndroidToolProviderTest.kt",
            "android/src/androidTest/kotlin/com/reados/msp/commandruntime/MspAndroidToolProviderInstrumentationTest.kt"
        )
    }
)

Push-Location $RepositoryRoot
try {
    foreach ($expectation in $expectations) {
        $manifestPath = [System.IO.Path]::GetFullPath((Join-Path $RepositoryRoot $expectation.Manifest))
        if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
            throw "Cargo manifest was not found: $manifestPath"
        }

        $members = @(& $cargoCommand.Source package --manifest-path $manifestPath --list --allow-dirty --locked 2>&1)
        if ($LASTEXITCODE -ne 0) {
            throw "cargo package --list failed for $($expectation.Manifest)."
        }

        $normalizedMembers = @(
            $members |
                ForEach-Object { ([string] $_).Trim().Replace('\', '/') } |
                Where-Object { $_ -and $_ -notmatch '^(warning:|\s*Compiling\s|\s*Finished\s|\s*Packaging\s|\s*Verifying\s|\s*Updating\s)' }
        )
        $rawMspMembers = @($normalizedMembers | Where-Object { $_ -cmatch '(^|/)MSP(/|$)' })
        if ($rawMspMembers.Count -gt 0) {
            throw "Package $($expectation.Manifest) contains raw MSP path members: $($rawMspMembers -join ', ')"
        }
        foreach ($requiredFile in $expectation.Files) {
            if ($normalizedMembers -notcontains $requiredFile) {
                throw "Package $($expectation.Manifest) omits required member: $requiredFile"
            }
        }
    }
}
finally {
    Pop-Location
}

Write-Host "Focused Rust package manifest tests passed."
