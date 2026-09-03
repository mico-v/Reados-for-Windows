# `msp-process-backend`

ReadOS-owned platform process behavior for verified external runtime providers.

The public launch object is backend-only: it accepts a host executable and
working directory only after a host/provider has resolved a verified bundle
identity. It never accepts a shell command line or consults `PATH`. Windows
assigns the child to a Job Object with kill-on-close; Linux starts a dedicated
process group and kills the group on cancellation, timeout, or drop. Both
platforms clear the inherited environment, enforce bounded stdout/stderr,
redact the bound host paths from returned bytes, and fail closed on unsupported
targets.

Policy, approval, product audit, provider selection, and virtual path parsing
remain above this crate. This crate owns only platform process lifecycle.
