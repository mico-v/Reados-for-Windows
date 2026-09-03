# Android tool-provider experiment decision

Status: H4.3 experiment slice complete; production adoption remains deferred.

## Decision

ReadOS will prototype an app-owned Toybox provider before considering BusyBox
or Termux. The provider is a fixed-profile Android platform backend, not a
portable command-pack feature. The model selects a registered profile and a
virtual `/workspace/...` path; it never selects an executable, host path,
shell string, Termux session, or arbitrary argv.

The first profiles are intentionally small:

- `pwd` — print the projected workspace cwd;
- `cat` — read one projected virtual file;
- `ls` — list one projected virtual directory.

These are an experiment in external-provider lifecycle and are not a claim
that Android needs a second implementation of the portable builtin pack.
Product routing stays managed until a real workflow demonstrates value over
the in-process virtual commands.

## Comparison

| Option | Size and coverage | License/update | Sandbox/storage | Decision |
| --- | --- | --- | --- | --- |
| App-owned Toybox | Small multi-call binary; Android-oriented utilities and stable argv entrypoint | 0BSD; ReadOS pins and updates the exact artifact | App-private executable and temporary projected workspace; no shared-storage path | Selected prototype |
| App-owned BusyBox | Broad POSIX utility coverage, but larger surface and GPL obligations | GPL-2.0-only; requires source/notice and update process | Same possible app-private sandbox, but more commands increase review surface | Keep as later comparison, not bundled now |
| Optional Termux | Broadest user-installed tool ecosystem, version/configuration variability | Package-specific licenses and user-controlled updates | Requires explicit installation, IPC/storage permissions, and lifecycle coordination | Not a dependency; rejected by this slice |

Toybox is the smallest reviewed starting point because it offers a platform
native multi-call executable while keeping one artifact identity and one
license family. BusyBox may be preferable for a later workflow that needs
specific GNU/POSIX utilities, but that is not evidence to broaden this slice.

## Contract and lifecycle

`MspAndroidToolProvider` is in the Android platform layer. It binds an
explicit, digest-verified `toybox` executable and an app-private workspace
root. It constructs fixed argv for the three profiles, clears the inherited
environment, sets `LANG=C`/`LC_ALL=C`, applies output/time limits, terminates
the process on cancellation or expiry, and redacts the bound executable and
workspace root from returned bytes.

The request contains only a profile, virtual cwd, and optional virtual path.
`MspAndroidWorkspaceProjection` or a future projected-directory adapter owns
copying authorized SAF content into the temporary workspace. URI authorities,
document IDs, and Android filesystem paths remain Kotlin/platform data. The
provider does not own policy, approval, audit, credentials, or product session
state.

`SystemToyboxOracle` exists only for instrumentation evidence on a target
device. `/system/bin/toybox` is not redistributed and does not make every
Android build a supported provider. `TermuxOptional` is an explicit rejected
source in this experiment; there is no shell fallback.

## Evidence and limitations

JVM Android library tests cover fixed argv, virtual path containment, digest
drift, Termux rejection, and path-free failures. On 2026-08-26, an API 35
x86_64 emulator (`reados-api35-x86_64(AVD) - 15`) passed all 7 connected
instrumentation tests, including the app-owned asset fail-closed path, the
system Toybox oracle, projected workspace reads, cancellation, and disclosure
checks. Existing SAF/FFI projection evidence remains part of that emulator
run.

The experiment does not yet bundle a Toybox executable, does not grant arbitrary process access,
does not support BusyBox or Termux execution, and does not route product
commands. Before production adoption, add a reviewed app-owned artifact,
license/NOTICE package evidence, device-run lifecycle tests, projected
directory semantics, and managed Host policy/approval/audit integration.
