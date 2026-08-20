# MSP reference source and external verification

The repository may be developed with the local MSP reference archives under
`MSP/`. The directory is intentionally ignored by Git and must never be copied
into a ReadOS package.

## Which archive to use

- `msp-windows-rust-source-preview-20260818.zip` is the Windows Rust preview. It
  contains `Implementations/Windows/Cargo.toml`, the Windows crates, bindings,
  fixtures, specifications, and Windows build/release scripts.
- `MSP-main (1).zip` is a broader MSP repository snapshot, but its
  `Implementations/Windows` directory is only a placeholder. It is useful for
  cross-reference and Swift/specification history, not as the Windows Rust
  workspace.

The preview archive currently has SHA-256:

```text
3e5775db6d5d02754df8b64c2689e29af3dc12137805bb87623c33e72091a922
```

The main snapshot currently has SHA-256:

```text
bfc77efa148872fcbeb41e7d1b980ceaade009df0e0cc4532fb3fd6e2f6342cf
```

These digests identify the local archives only. They are not an upstream Git
commit and must not be used as a full source-revision claim.

## Prepare the ignored reference tree

From the repository root:

```powershell
python scripts/prepare-msp-reference.py `
  --archive MSP\msp-windows-rust-source-preview-20260818.zip `
  --destination MSP `
  --sha256 3e5775db6d5d02754df8b64c2689e29af3dc12137805bb87623c33e72091a922
```

The script verifies the SHA-256, rejects unsafe ZIP member paths, requires the
Windows Cargo manifest, and extracts only below the ignored `MSP/` directory.

Then run:

```powershell
cargo test --workspace --locked --manifest-path MSP\Implementations\Windows\Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass `
  -File scripts\verify-msp-capability-manifest.ps1
```

The capability manifest may still report `blocked`: the preview package does
not contain every historical parity/release runner named by the evidence
contract. A successful validator run means the blocked state is internally
consistent; it does not mean full upstream parity has passed.

## GitHub Actions

`.github/workflows/windows-ci.yml` now provides:

- locked native and managed verification on Windows;
- public FFI release build with the declared static-CRT and `/Brepro` flags;
- public FFI PE/export/hash verification;
- .NET FFI consumer tests against the explicit DLL path;
- Node binding tests and CMake/C++ consumer tests on `windows-latest`;
- portable Rust contract tests on Ubuntu;
- reporting-only multi-architecture and Debian PTY availability checks;
- an opt-in `workflow_dispatch` reference job.

The reference job requires both `reference_url` and `reference_sha256`. It
never executes an unverified download. After downloading and checking the
archive, it prepares the ignored reference tree and runs the recovered Windows
workspace tests. Normal push and pull-request jobs do not depend on a local
ignored archive and therefore do not silently claim reference parity.

CMake, Visual Studio, Windows SDK, .NET SDK, Rust targets, and NuGet/Cargo/npm
caches are build-environment dependencies. Native DLLs and build directories
remain generated artifacts rather than checked-in project dependencies.
