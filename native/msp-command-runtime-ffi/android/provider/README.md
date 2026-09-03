# Android tool-provider profile

This directory contains the H4.3 provider manifest and provenance only. The
current slice does not redistribute Toybox or BusyBox. A production app-owned
Toybox bundle must be supplied by the Android packaging owner, verified with a
SHA-256 digest, and shipped with the exact license and NOTICE evidence before
`AppOwnedBundle` can be enabled.

`SystemToyboxOracle` is instrumentation-only evidence. `TermuxOptional` is an
explicitly unsupported source and never falls back to a shell session.
