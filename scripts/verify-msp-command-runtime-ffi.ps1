[CmdletBinding()]
param(
    [string] $DllPath,

    [switch] $SkipBuild,

    [switch] $SkipContractSmoke
)

$ErrorActionPreference = "Stop"

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [string[]] $ArgumentList = @()
    )

    & $FilePath @ArgumentList
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "External command failed with exit code ${exitCode}: $FilePath $($ArgumentList -join ' ')"
    }
}

function Get-ToolPath {
    param(
        [Parameter(Mandatory)]
        [string[]] $Names
    )

    foreach ($name in $Names) {
        $command = Get-Command $name -ErrorAction SilentlyContinue
        if ($null -ne $command -and $command.Source -and (Test-Path -LiteralPath $command.Source -PathType Leaf)) {
            return $command.Source
        }
    }

    return $null
}

function Read-U16 {
    param(
        [Parameter(Mandatory)]
        [byte[]] $Bytes,

        [Parameter(Mandatory)]
        [int] $Offset
    )

    if ($Offset -lt 0 -or $Offset + 2 -gt $Bytes.Length) {
        throw "PE field is outside the DLL: offset $Offset"
    }

    return [System.BitConverter]::ToUInt16($Bytes, $Offset)
}

function Read-U32 {
    param(
        [Parameter(Mandatory)]
        [byte[]] $Bytes,

        [Parameter(Mandatory)]
        [int] $Offset
    )

    if ($Offset -lt 0 -or $Offset + 4 -gt $Bytes.Length) {
        throw "PE field is outside the DLL: offset $Offset"
    }

    return [System.BitConverter]::ToUInt32($Bytes, $Offset)
}

function Convert-RvaToFileOffset {
    param(
        [Parameter(Mandatory)]
        [uint32] $Rva,

        [Parameter(Mandatory)]
        [object[]] $Sections,

        [Parameter(Mandatory)]
        [byte[]] $Bytes
    )

    foreach ($section in $Sections) {
        $sectionStart = [uint64] $section.VirtualAddress
        $sectionEnd = $sectionStart + [uint64] $section.MappedSize
        if ([uint64] $Rva -ge $sectionStart -and [uint64] $Rva -lt $sectionEnd) {
            $delta = [uint64] $Rva - $sectionStart
            if ($delta -ge [uint64] $section.RawSize) {
                throw "PE RVA 0x$('{0:X8}' -f $Rva) points beyond raw section data."
            }

            $offset = [uint64] $section.RawPointer + $delta
            if ($offset -ge [uint64] $Bytes.Length) {
                throw "PE RVA 0x$('{0:X8}' -f $Rva) maps beyond the DLL."
            }

            return [int] $offset
        }
    }

    throw "PE RVA 0x$('{0:X8}' -f $Rva) does not map to a section."
}

function Read-AsciiStringAtRva {
    param(
        [Parameter(Mandatory)]
        [uint32] $Rva,

        [Parameter(Mandatory)]
        [object[]] $Sections,

        [Parameter(Mandatory)]
        [byte[]] $Bytes
    )

    $offset = Convert-RvaToFileOffset -Rva $Rva -Sections $Sections -Bytes $Bytes
    $end = $offset
    while ($end -lt $Bytes.Length -and $Bytes[$end] -ne 0) {
        $end++
    }

    if ($end -ge $Bytes.Length) {
        throw "PE export name at RVA 0x$('{0:X8}' -f $Rva) is not NUL-terminated."
    }

    return [System.Text.Encoding]::ASCII.GetString($Bytes, $offset, $end - $offset)
}

function Get-PeExportNames {
    param(
        [Parameter(Mandatory)]
        [string] $Path
    )

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 512 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        throw "Runtime FFI artifact is not a valid DOS/PE DLL: $Path"
    }

    $peOffset = Read-U32 -Bytes $bytes -Offset 0x3C
    if ([uint64] $peOffset + 24 -gt [uint64] $bytes.Length) {
        throw "The runtime FFI PE header is outside the DLL."
    }
    if ([System.Text.Encoding]::ASCII.GetString($bytes, [int] $peOffset, 4) -cne ("PE" + [char] 0 + [char] 0)) {
        throw "The runtime FFI artifact does not contain a PE signature."
    }

    $fileHeaderOffset = [int] $peOffset + 4
    $machine = Read-U16 -Bytes $bytes -Offset $fileHeaderOffset
    if ($machine -ne 0x8664) {
        throw "The runtime FFI release DLL must target AMD64 (PE machine 0x8664); found 0x$('{0:X4}' -f $machine)."
    }
    $sectionCount = Read-U16 -Bytes $bytes -Offset ($fileHeaderOffset + 2)
    $optionalHeaderSize = Read-U16 -Bytes $bytes -Offset ($fileHeaderOffset + 16)
    $optionalHeaderOffset = $fileHeaderOffset + 20
    $optionalMagic = Read-U16 -Bytes $bytes -Offset $optionalHeaderOffset
    if ($optionalMagic -ne 0x20B) {
        throw "The runtime FFI release DLL must be a PE32+ image."
    }
    if ($optionalHeaderSize -lt 240 -or [uint64] $optionalHeaderOffset + $optionalHeaderSize -gt [uint64] $bytes.Length) {
        throw "The runtime FFI PE optional header is invalid."
    }

    $numberOfDirectories = Read-U32 -Bytes $bytes -Offset ($optionalHeaderOffset + 108)
    if ($numberOfDirectories -lt 1) {
        throw "The runtime FFI PE image has no export data directory."
    }

    $dataDirectoryOffset = $optionalHeaderOffset + 112
    $sectionTableOffset = $optionalHeaderOffset + $optionalHeaderSize
    $sections = @()
    for ($index = 0; $index -lt $sectionCount; $index++) {
        $sectionOffset = $sectionTableOffset + ($index * 40)
        if ([uint64] $sectionOffset + 40 -gt [uint64] $bytes.Length) {
            throw "The runtime FFI PE section table is truncated."
        }

        $virtualSize = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 8)
        $virtualAddress = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 12)
        $rawSize = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 16)
        $rawPointer = Read-U32 -Bytes $bytes -Offset ($sectionOffset + 20)
        if ($rawSize -gt 0 -and ([uint64] $rawPointer + $rawSize -gt [uint64] $bytes.Length)) {
            throw "The runtime FFI PE section points outside the DLL."
        }

        $mappedSize = [Math]::Max([uint32] $virtualSize, [uint32] $rawSize)
        if ($mappedSize -gt 0) {
            $sections += [pscustomobject] @{
                VirtualAddress = $virtualAddress
                MappedSize = $mappedSize
                RawSize = $rawSize
                RawPointer = $rawPointer
            }
        }
    }

    $importDirectoryRva = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 8)
    $importDirectorySize = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 12)
    $forbiddenDynamicCrtNames = @(
        "VCRUNTIME",
        "MSVCP",
        "CONCRT",
        "api-ms-win-crt-",
        "ucrtbase.dll",
        "msvcrt.dll"
    )
    if ($importDirectoryRva -ne 0 -and $importDirectorySize -ne 0) {
        $importOffset = Convert-RvaToFileOffset -Rva $importDirectoryRva -Sections $sections -Bytes $bytes
        $descriptorCount = [Math]::Floor($importDirectorySize / 20)
        $foundTerminator = $false
        for ($index = 0; $index -lt $descriptorCount; $index++) {
            $descriptorOffset = $importOffset + ($index * 20)
            if ([uint64] $descriptorOffset + 20 -gt [uint64] $bytes.Length) {
                throw "The runtime FFI PE import directory is truncated."
            }

            $nameRva = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 12)
            $firstThunk = Read-U32 -Bytes $bytes -Offset ($descriptorOffset + 16)
            if ($nameRva -eq 0 -and $firstThunk -eq 0) {
                $foundTerminator = $true
                break
            }
            if ($nameRva -eq 0) {
                throw "The runtime FFI PE import descriptor has no module name."
            }

            $moduleName = Read-AsciiStringAtRva -Rva $nameRva -Sections $sections -Bytes $bytes
            $dynamicCrt = $forbiddenDynamicCrtNames | Where-Object {
                $moduleName.StartsWith($_, [System.StringComparison]::OrdinalIgnoreCase)
            } | Select-Object -First 1
            if ($dynamicCrt) {
                throw "The runtime FFI DLL is not CRT-self-contained; found dynamic CRT import: $moduleName"
            }
        }

        if (-not $foundTerminator) {
            throw "The runtime FFI PE import directory has no terminator."
        }
    }

    $exportDirectoryRva = Read-U32 -Bytes $bytes -Offset $dataDirectoryOffset
    $exportDirectorySize = Read-U32 -Bytes $bytes -Offset ($dataDirectoryOffset + 4)
    if ($exportDirectoryRva -eq 0 -or $exportDirectorySize -eq 0) {
        throw "The runtime FFI DLL has no PE export directory."
    }

    $exportDirectoryOffset = Convert-RvaToFileOffset -Rva $exportDirectoryRva -Sections $sections -Bytes $bytes
    if ([uint64] $exportDirectoryOffset + 40 -gt [uint64] $bytes.Length) {
        throw "The runtime FFI PE export directory is truncated."
    }

    $functionCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 20)
    $nameCount = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 24)
    $namesRva = Read-U32 -Bytes $bytes -Offset ($exportDirectoryOffset + 32)
    if ($functionCount -ne 13 -or $nameCount -ne 13) {
        throw "The runtime FFI PE export directory contains $nameCount named/$functionCount total exports; expected 13/13."
    }

    $namesOffset = Convert-RvaToFileOffset -Rva $namesRva -Sections $sections -Bytes $bytes
    $names = @()
    for ($index = 0; $index -lt $nameCount; $index++) {
        $nameRva = Read-U32 -Bytes $bytes -Offset ($namesOffset + ($index * 4))
        $names += Read-AsciiStringAtRva -Rva $nameRva -Sections $sections -Bytes $bytes
    }

    return $names
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$abiVerifier = Join-Path $repoRoot "scripts\verify-msp-command-runtime-ffi-abi.ps1"
& $abiVerifier -RepositoryRoot $repoRoot
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "Runtime FFI ABI manifest verification failed."
}
$commandProfileVerifier = Join-Path $repoRoot "scripts\verify-msp-command-profile.ps1"
& $commandProfileVerifier -RepositoryRoot $repoRoot
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
    throw "Runtime FFI portable command profile verification failed."
}
$cargo = Get-ToolPath -Names @("cargo.exe", "cargo")
if (-not $cargo) {
    throw "cargo was not found. Install Rust or add cargo.exe to PATH."
}

$requiredRustFlags = "-C target-feature=+crt-static -C link-arg=/Brepro"
$buildFlagsVerified = $false

if ([string]::IsNullOrWhiteSpace($DllPath)) {
    $DllPath = Join-Path $repoRoot "target\release\msp_command_runtime_ffi.dll"
}
$DllPath = [System.IO.Path]::GetFullPath($DllPath)

if (-not $SkipBuild) {
    Push-Location $repoRoot
    $previousRustFlags = $env:RUSTFLAGS
    $previousCargoIncremental = $env:CARGO_INCREMENTAL
    try {
        # Keep this release gate self-contained. Do not require a caller to know
        # or preserve the CRT/reproducibility flags used by the full verifier.
        $env:CARGO_INCREMENTAL = "0"
        if ([string]::IsNullOrWhiteSpace($previousRustFlags)) {
            $env:RUSTFLAGS = $requiredRustFlags
        }
        else {
            $env:RUSTFLAGS = "$previousRustFlags $requiredRustFlags"
        }
        Invoke-CheckedCommand -FilePath $cargo -ArgumentList @(
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            "native/msp-command-runtime-ffi/Cargo.toml"
        )
        $buildFlagsVerified = $true
    }
    finally {
        if ($null -eq $previousRustFlags) {
            Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
        }
        else {
            $env:RUSTFLAGS = $previousRustFlags
        }
        if ($null -eq $previousCargoIncremental) {
            Remove-Item Env:CARGO_INCREMENTAL -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_INCREMENTAL = $previousCargoIncremental
        }
        Pop-Location
    }
}
else {
    Write-Warning "-SkipBuild verifies a prebuilt artifact only; this invocation does not establish $requiredRustFlags. It will not claim a release build pass."
}

if (-not (Test-Path -LiteralPath $DllPath -PathType Leaf)) {
    throw "Runtime FFI release DLL was not found: $DllPath"
}

$expectedExports = @(
    "msp_command_runtime_ffi_abi_version",
    "msp_command_runtime_ffi_version_data",
    "msp_command_runtime_ffi_runtime_create",
    "msp_command_runtime_ffi_runtime_free",
    "msp_command_runtime_ffi_workspace_create",
    "msp_command_runtime_ffi_workspace_free",
    "msp_command_runtime_ffi_workspace_put_file",
    "msp_command_runtime_ffi_execute_json",
    "msp_command_runtime_ffi_result_exit_code",
    "msp_command_runtime_ffi_result_stdout_data",
    "msp_command_runtime_ffi_result_stderr_data",
    "msp_command_runtime_ffi_result_diagnostic_data",
    "msp_command_runtime_ffi_result_free"
)
$definitionPath = Join-Path $repoRoot "native\msp-command-runtime-ffi\exports\msp_command_runtime_ffi.def"
if (-not (Test-Path -LiteralPath $definitionPath -PathType Leaf)) {
    throw "Runtime FFI export definition was not found: $definitionPath"
}
$definitionExports = @()
$readingExports = $false
foreach ($line in Get-Content -LiteralPath $definitionPath) {
    $trimmed = $line.Trim()
    if ($trimmed -eq "EXPORTS") {
        $readingExports = $true
        continue
    }
    if (-not $readingExports -or [string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith(";")) {
        continue
    }
    if ($trimmed -notmatch '^([A-Za-z_][A-Za-z0-9_]*)$') {
        throw "Malformed runtime FFI export definition line: $line"
    }
    $definitionExports += $Matches[1]
}
if (-not $readingExports -or $definitionExports.Count -ne $expectedExports.Count -or
    @($definitionExports | Sort-Object -Unique).Count -ne $definitionExports.Count -or
    @($definitionExports | Where-Object { $expectedExports -notcontains $_ }).Count -gt 0 -or
    @($expectedExports | Where-Object { $definitionExports -notcontains $_ }).Count -gt 0) {
    throw "Runtime FFI export definition must contain exactly the expected 13 exports."
}
$actualExports = @(Get-PeExportNames -Path $DllPath)
$missingExports = @($expectedExports | Where-Object { $actualExports -notcontains $_ })
$unexpectedExports = @($actualExports | Where-Object { $expectedExports -notcontains $_ })
if ($missingExports.Count -gt 0 -or $unexpectedExports.Count -gt 0 -or $actualExports.Count -ne $expectedExports.Count) {
    throw "Runtime FFI exports drifted. Missing: $($missingExports -join ', '); unexpected: $($unexpectedExports -join ', '); count: $($actualExports.Count)."
}

if (-not $SkipContractSmoke) {
    $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("reados-runtime-ffi-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
try {
    $includeRoot = Join-Path $repoRoot "native\msp-command-runtime-ffi\include"
    $cContract = Join-Path $repoRoot "native\msp-command-runtime-ffi\tests\c_contract.c"
    $cObject = Join-Path $tempRoot "c_contract.obj"
    $cCompiler = Get-ToolPath -Names @("cl.exe", "clang-cl.exe", "gcc.exe", "clang.exe")
    if (-not $cCompiler) {
        throw "No C compiler was found for the C11 header smoke."
    }

    $cCompilerName = [System.IO.Path]::GetFileName($cCompiler).ToLowerInvariant()
    if ($cCompilerName -eq "cl.exe" -or $cCompilerName -eq "clang-cl.exe") {
        Invoke-CheckedCommand -FilePath $cCompiler -ArgumentList @(
            "/nologo",
            "/std:c11",
            "/W4",
            "/WX",
            "/I$includeRoot",
            "/c",
            $cContract,
            "/Fo$cObject"
        )
    }
    else {
        Invoke-CheckedCommand -FilePath $cCompiler -ArgumentList @(
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-I$includeRoot",
            "-c",
            $cContract,
            "-o",
            $cObject
        )
    }

    $dllCopy = Join-Path $tempRoot "msp_command_runtime_ffi.dll"
    Copy-Item -LiteralPath $DllPath -Destination $dllCopy -Force
    $importLibrary = [System.IO.Path]::ChangeExtension($DllPath, ".dll.lib")
    if (-not (Test-Path -LiteralPath $importLibrary -PathType Leaf)) {
        throw "Runtime FFI import library was not found beside the release DLL: $importLibrary"
    }

    $cSmokeSource = Join-Path $tempRoot "runtime_ffi_smoke.c"
    @'
#include "msp_command_runtime_ffi.h"

#include <assert.h>
#include <string.h>

int main(void) {
    assert(msp_command_runtime_ffi_abi_version() == 1);
    MspCommandRuntimeFfiRuntime *runtime = msp_command_runtime_ffi_runtime_create();
    MspCommandRuntimeFfiWorkspace *workspace = msp_command_runtime_ffi_workspace_create();
    assert(runtime != NULL && workspace != NULL);

    const uint8_t request[] =
        "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/c11\"}";
    MspCommandRuntimeFfiResult *result = msp_command_runtime_ffi_execute_json(
        runtime, workspace, request, sizeof(request) - 1);
    assert(result != NULL);
    assert(msp_command_runtime_ffi_result_exit_code(result) == 0);

    size_t stdout_length = 0;
    const uint8_t *stdout_bytes =
        msp_command_runtime_ffi_result_stdout_data(result, &stdout_length);
    assert(stdout_length == 5 && memcmp(stdout_bytes, "/c11\n", 5) == 0);

    msp_command_runtime_ffi_result_free(result);
    msp_command_runtime_ffi_workspace_free(workspace);
    msp_command_runtime_ffi_runtime_free(runtime);
    return 0;
}
'@ | Set-Content -LiteralPath $cSmokeSource -Encoding UTF8

    $cSmokeExe = Join-Path $tempRoot "runtime_ffi_smoke_c.exe"
    if ($cCompilerName -eq "cl.exe" -or $cCompilerName -eq "clang-cl.exe") {
        Invoke-CheckedCommand -FilePath $cCompiler -ArgumentList @(
            "/nologo",
            "/std:c11",
            "/W4",
            "/WX",
            "/I$includeRoot",
            $cSmokeSource,
            $importLibrary,
            "/Fe:$cSmokeExe"
        )
    }
    else {
        Invoke-CheckedCommand -FilePath $cCompiler -ArgumentList @(
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-I$includeRoot",
            $cSmokeSource,
            $importLibrary,
            "-o",
            $cSmokeExe
        )
    }

    Invoke-CheckedCommand -FilePath $cSmokeExe

    $cppSource = Join-Path $tempRoot "runtime_ffi_smoke.cpp"
    @'
#include "msp_command_runtime_ffi.h"

#include <cassert>
#include <cstdint>
#include <cstring>
#include <windows.h>

template <typename T>
T bind_export(HMODULE module, const char *name) {
    FARPROC raw = GetProcAddress(module, name);
    assert(raw != nullptr);
    T value{};
    static_assert(sizeof(value) == sizeof(raw));
    std::memcpy(&value, &raw, sizeof(value));
    return value;
}

int main(int argc, char **argv) {
    assert(argc == 2);
    HMODULE module = LoadLibraryA(argv[1]);
    assert(module != nullptr);

    auto abi_version = bind_export<decltype(&msp_command_runtime_ffi_abi_version)>(module, "msp_command_runtime_ffi_abi_version");
    auto version_data = bind_export<decltype(&msp_command_runtime_ffi_version_data)>(module, "msp_command_runtime_ffi_version_data");
    auto runtime_create = bind_export<decltype(&msp_command_runtime_ffi_runtime_create)>(module, "msp_command_runtime_ffi_runtime_create");
    auto runtime_free = bind_export<decltype(&msp_command_runtime_ffi_runtime_free)>(module, "msp_command_runtime_ffi_runtime_free");
    auto workspace_create = bind_export<decltype(&msp_command_runtime_ffi_workspace_create)>(module, "msp_command_runtime_ffi_workspace_create");
    auto workspace_free = bind_export<decltype(&msp_command_runtime_ffi_workspace_free)>(module, "msp_command_runtime_ffi_workspace_free");
    auto workspace_put_file = bind_export<decltype(&msp_command_runtime_ffi_workspace_put_file)>(module, "msp_command_runtime_ffi_workspace_put_file");
    auto execute_json = bind_export<decltype(&msp_command_runtime_ffi_execute_json)>(module, "msp_command_runtime_ffi_execute_json");
    auto result_exit_code = bind_export<decltype(&msp_command_runtime_ffi_result_exit_code)>(module, "msp_command_runtime_ffi_result_exit_code");
    auto result_stdout_data = bind_export<decltype(&msp_command_runtime_ffi_result_stdout_data)>(module, "msp_command_runtime_ffi_result_stdout_data");
    auto result_stderr_data = bind_export<decltype(&msp_command_runtime_ffi_result_stderr_data)>(module, "msp_command_runtime_ffi_result_stderr_data");
    auto result_diagnostic_data = bind_export<decltype(&msp_command_runtime_ffi_result_diagnostic_data)>(module, "msp_command_runtime_ffi_result_diagnostic_data");
    auto result_free = bind_export<decltype(&msp_command_runtime_ffi_result_free)>(module, "msp_command_runtime_ffi_result_free");

    assert(abi_version() == 1);
    size_t version_length = 0;
    const uint8_t *version = version_data(&version_length);
    assert(version_length == 5 && std::memcmp(version, "0.1.0", 5) == 0);

    MspCommandRuntimeFfiRuntime *runtime = runtime_create();
    MspCommandRuntimeFfiWorkspace *workspace = workspace_create();
    assert(runtime != nullptr && workspace != nullptr);

    const uint8_t file_bytes[] = {0, 255};
    const char file_path[] = "/workspace/input";
    assert(workspace_put_file(
        workspace,
        reinterpret_cast<const uint8_t *>(file_path),
        sizeof(file_path) - 1,
        file_bytes,
        sizeof(file_bytes)) == 0);

    const uint8_t request[] =
        "{\"version\":1,\"command\":\"cat\",\"cwd\":\"/workspace\",\"stdinBase64\":\"AP8=\"}";
    MspCommandRuntimeFfiResult *result = execute_json(
        runtime,
        workspace,
        request,
        sizeof(request) - 1);
    assert(result != nullptr && result_exit_code(result) == 0);

    size_t stdout_length = 0;
    const uint8_t *stdout_bytes = result_stdout_data(result, &stdout_length);
    assert(stdout_length == 2 && stdout_bytes[0] == 0 && stdout_bytes[1] == 255);

    size_t stderr_length = 0;
    const uint8_t *stderr_bytes = result_stderr_data(result, &stderr_length);
    assert(stderr_length == 0 && stderr_bytes != nullptr);

    size_t diagnostic_length = 0;
    const uint8_t *diagnostic = result_diagnostic_data(result, &diagnostic_length);
    assert(diagnostic_length == 17 && std::memcmp(diagnostic, "{\"code\":\"msp.ok\"}", 17) == 0);

    result_free(result);

    const uint8_t malformed[] = "not json";
    MspCommandRuntimeFfiResult *malformed_result = execute_json(
        runtime, workspace, malformed, sizeof(malformed) - 1);
    assert(malformed_result != nullptr && result_exit_code(malformed_result) == 2);
    result_free(malformed_result);

    const uint8_t independent_request[] =
        "{\"version\":1,\"command\":\"pwd\",\"cwd\":\"/independent\"}";
    MspCommandRuntimeFfiResult *independent_result = execute_json(
        runtime,
        workspace,
        independent_request,
        sizeof(independent_request) - 1);
    assert(independent_result != nullptr && result_exit_code(independent_result) == 0);
    workspace_free(workspace);
    runtime_free(runtime);
    size_t independent_length = 0;
    const uint8_t *independent_stdout = result_stdout_data(
        independent_result, &independent_length);
    assert(independent_length == 13 && std::memcmp(independent_stdout, "/independent\n", 13) == 0);
    result_free(independent_result);
    FreeLibrary(module);
    return 0;
}
'@ | Set-Content -LiteralPath $cppSource -Encoding UTF8

    $dllCopy = Join-Path $tempRoot "msp_command_runtime_ffi.dll"
    Copy-Item -LiteralPath $DllPath -Destination $dllCopy -Force
    $cppCompiler = Get-ToolPath -Names @("cl.exe", "clang-cl.exe", "g++.exe", "clang++.exe")
    if (-not $cppCompiler) {
        throw "No C++ compiler was found for the runtime FFI smoke."
    }

    $cppExe = Join-Path $tempRoot "runtime_ffi_smoke.exe"
    $cppCompilerName = [System.IO.Path]::GetFileName($cppCompiler).ToLowerInvariant()
    if ($cppCompilerName -eq "cl.exe" -or $cppCompilerName -eq "clang-cl.exe") {
        Invoke-CheckedCommand -FilePath $cppCompiler -ArgumentList @(
            "/nologo",
            "/std:c++17",
            "/EHsc",
            "/W4",
            "/WX",
            "/wd4191",
            "/I$includeRoot",
            $cppSource,
            "/Fe:$cppExe"
        )
    }
    else {
        Invoke-CheckedCommand -FilePath $cppCompiler -ArgumentList @(
            "-std=c++17",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-I$includeRoot",
            $cppSource,
            "-o",
            $cppExe
        )
    }

    Invoke-CheckedCommand -FilePath $cppExe -ArgumentList @($dllCopy)
}
finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}
}

if ($buildFlagsVerified) {
    Write-Host "ReadOS command runtime FFI release verification passed."
    Write-Host "  Build flags: $requiredRustFlags"
}
else {
    Write-Host "ReadOS command runtime FFI prebuilt artifact verification passed; no release build claim was made."
}
Write-Host "  Release DLL: $DllPath"
Write-Host "  Exports: 13"
Write-Host "  C11 header smoke: passed"
Write-Host "  C++ runtime smoke: passed"
