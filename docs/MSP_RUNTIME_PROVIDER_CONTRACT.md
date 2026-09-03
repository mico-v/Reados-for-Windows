# Verified runtime provider contract

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md).
This document defines the host-neutral H4.1 contract. It is a registration and
launch-plan boundary, not a process launcher.

## Ownership

`ReadOS.Msp.Hosting.Native.MspVerifiedRuntimeProviderContracts` owns the
reusable contract and pure validation. `MspVerifiedRuntimeProviderCatalog`
owns the Host-side immutable registration/selection boundary. They sit above
platform process backends:

```text
host policy / approval / product audit
        │
        ▼
provider registration + launch-plan validation  (this contract)
        │
        ▼
Windows / Linux / Android process and workspace backends
```

The contract does not load a bundle, inspect a filesystem, resolve `PATH`,
open a network connection, start a process, or append an audit record. A
backend may consume a ready plan only after launch-time identity
revalidation. Product policy and approval remain the caller's responsibility.

## Required provider declaration

A provider registration combines verified, platform-neutral bundle evidence
with:

- immutable provider id and one or more explicitly named command profiles;
- bundle-relative entry point and verifier-produced executable SHA-256;
- projected/dedicated virtual workspace mode;
- bounded arguments, environment, output, time, memory, child-process, and
  workspace-file limits;
- mandatory cancellation and process-tree cleanup guarantees; and
- license and NOTICE identifiers for package provenance.

The registration evaluator is fail-closed. A missing or unverified bundle,
identity/RID/architecture mismatch, shell entry point, unsafe relative entry point,
missing executable digest, missing lifecycle guarantee, invalid limit, or
missing provenance produces a blocked result with a stable reason code. A
blocked result never carries a usable provider contract.

## Launch request and plan

The model-facing launch request contains only:

- registered profile id;
- virtual cwd;
- an argv list; and
- bounded caller-supplied environment values.

There is deliberately no shell string, host cwd, executable path, `PATH`,
network URL, process handle, or host workspace root. `PATH`, `PATHEXT`,
`COMSPEC`, `SHELL`, `LD_LIBRARY_PATH`, and `DYLD_LIBRARY_PATH` are rejected.
Obvious DOS/UNC/file-URL host paths and control/NUL values are rejected. The
ready plan carries the bundle-relative entry point and immutable identity
metadata for a platform backend to resolve inside the already verified bundle.

The platform backend must revalidate both the bundle identity and executable
digest immediately before launch. Any verification, runtime, bundle id,
manifest digest, RID, architecture, or executable digest change yields
`msp.provider.plan.identity_changed` and no launch.

## Profile rule

Profiles are the allowlist boundary. The model selects a profile id; it does
not select an executable. An entry point whose leaf is `sh`, `bash`, `cmd`,
PowerShell, or another shell wrapper is rejected. Future profiles can use the
`VirtualWorkspaceOnly` argument policy for commands whose arguments are
themselves virtual paths. A profile may also declare an immutable argument
prefix; every launch must carry that prefix exactly, with only the bounded
suffix subject to the profile's argument policy. No profile may fall back to
arbitrary shell text.

## Current scope and non-claims

H4.1 remains the host-neutral registration and launch-plan contract. H4.2 now
adds a separately reviewed `msp-process-backend` plus the
`msp-git-provider` fixed-profile implementation. The provider supports only
`status`, `files`, `head`, and bounded `log`; it accepts a virtual
`/workspace/...` cwd and provider-owned argv/configuration, never arbitrary Git
argv, shell text, host paths, hooks, credentials, or network operations.

The process backend owns executable digest revalidation, virtual-cwd
confinement, environment clearing, bounded output, timeout/cancellation, and
Windows Job Object or Unix process-group cleanup. Policy, approval, provider
selection, product audit, and session projection remain in Hosting/App. Git is
currently host-bound: the target Host supplies the Git executable and digest;
these crates do not redistribute a Git executable and are not yet product
command routing.

Windows and Linux provider/backend tests are available, including configuration
and hook isolation, virtual repository escape, cancellation, timeout, output
sanitization, and real Git invocation. Android H4.3 now has a separate
platform-layer Toybox experiment with fixed `pwd`/`cat`/`ls` profiles, digest
binding, projected workspace confinement, bounded process lifecycle, and
explicit Termux rejection. It is not yet a supported bundled provider:
`AppOwnedBundle` awaits a ReadOS-distributed Toybox artifact, and the
`SystemToyboxOracle` path is instrumentation-only evidence. Arbitrary
external-process support and unrestricted Termux remain forbidden. Android's
Toybox profiles use a fixed argv prefix (`pwd`, `cat`, or `ls -1`) so a
multi-call executable cannot be repurposed as a shell. The Host-side
`MspAndroidToyboxProviderContract` can register only verified Android bundle
evidence; the Kotlin `MspAndroidToolProviderFactory` owns app-asset extraction
and executable verification, and returns unavailable when the app does not
ship Toybox.

Focused verification:

```powershell
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj --no-restore --filter FullyQualifiedName~MspVerifiedRuntimeProviderContractTests
cargo test -p msp-process-backend -p msp-git-provider --locked
cargo clippy -p msp-process-backend -p msp-git-provider --all-targets --locked -- -D warnings
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-msp-crate-packages.ps1
```

The provider/backend commands were run on Windows and again in the local WSL
checkout on 2026-08-26. The WSL run is non-Windows target evidence, not hosted
CI evidence.
