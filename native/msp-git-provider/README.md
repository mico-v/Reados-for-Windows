# `msp-git-provider`

The first H4.2 external runtime provider. It exposes one fixed, read-only
profile catalog on Windows and Linux:

- `status` → `git status --short`;
- `files` → cached tracked files as NUL-delimited bytes;
- `head` → verified `HEAD` object id; and
- `log` → bounded one-line history.

The caller selects a profile and virtual `/workspace` cwd. It cannot provide
Git argv, a shell string, a host cwd, a config path, hooks, a remote, or a
network option. The platform process backend maps the projected repository to
its host root, clears inherited environment/config, verifies the Git executable
digest before launch, and bounds output/time/cancellation. Returned output is
sanitized so the bound executable and workspace host paths cannot cross the
provider boundary.

The provider does not own policy, approval, session state, or product audit.
The host must authorize a launch before calling it and records exactly one
product audit result around the returned bounded result.

The crate carries the provider profile, provenance, Apache-2.0 wrapper license,
and NOTICE. It does not redistribute Git itself: each target Host must bind an
approved Git executable and its verified SHA-256 (Git remains GPL-2.0-only).
