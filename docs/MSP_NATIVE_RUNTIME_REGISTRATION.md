# ReadOS Native Runtime Capability Records

`ReadOS.Msp.Hosting.Native` now defines an evidence-only registration contract for
three future runtime families: Python, Node, and Git. This contract is a
pre-launch gate, not a launcher, process API, command registration, provider
integration, or UI surface.

## Required evidence

A `MspNativeRuntimeRegistrationRequest` can produce a `Verified`
`MspNativeRuntimeCapabilityRecord` only when all of the following are true:

- the runtime family is Python, Node, or Git;
- an expected bundle identity is present, including a 64-character SHA-256
  manifest digest;
- a bundle identity is present and marked `Verified` by the bundle verifier;
- the bundle runtime family, bundle identifier, and manifest digest match the
  expected identity;
- the bundle RID matches the explicitly supplied host target RID;
- the bundle PE machine matches the explicitly supplied host target PE machine;
- scratch behavior is explicitly `Disabled` or
  `DedicatedVirtualWorkspace`;
- executable resolution is explicitly `BundleOnly`; and
- network access is explicitly `Disabled`.

There is no default-to-safe interpretation for an omitted policy. Missing
bundle evidence, an unverified bundle, identity drift, RID/PE drift, an
unspecified scratch mode, host PATH lookup, or unspecified/allowed network
access produces a `Blocked` record with a stable reason code.

## Truthful blocked state

`Blocked` is the default capability state and has `IsUsable == false`. It is
never represented as a registered or available runtime. `StatusCode` is
`msp.native.runtime.blocked`, and `BlockReasonCode` plus `BlockMessage` explain
why the evidence did not pass. Records contain bundle identity metadata only;
they contain no host bundle path, executable path, PATH value, URL, credential,
or process handle.

The evaluator is pure. It does not load a bundle, hash files, resolve the host
PATH, open a network connection, start a process, or mutate the current MSP
command registry. A later launcher must consume a `Verified` record only after
performing its own launch-time revalidation and policy checks.

## Runtime matrix

| Runtime | Contract status | Required evidence |
| --- | --- | --- |
| Python | capability record supported; launch deferred | verified identity, matching RID/PE, explicit scratch mode, bundle-only resolution, network disabled |
| Node | capability record supported; launch deferred | verified identity, matching RID/PE, explicit scratch mode, bundle-only resolution, network disabled |
| Git | capability record supported; launch deferred | verified identity, matching RID/PE, explicit scratch mode, bundle-only resolution, network disabled |

This is intentionally not a claim that any of these runtimes is installed,
packaged, registered in the command registry, or executable on the host.

## Tests

`MspNativeRuntimeCapabilityContractsTests` covers:

- verified Python, Node, and Git records;
- missing bundles;
- bundle identifier and manifest-digest mismatches;
- unverified bundles;
- RID and PE mismatches;
- missing scratch policy;
- non-bundle-only path resolution; and
- network policy that is not explicitly disabled.

The focused command is:

```powershell
dotnet test .\tests\ReadOS.Msp.Hosting.Tests\ReadOS.Msp.Hosting.Tests.csproj --filter FullyQualifiedName~MspNativeRuntimeCapabilityContractsTests
```
