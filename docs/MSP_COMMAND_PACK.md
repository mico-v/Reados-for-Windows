# ReadOS virtual-workspace command pack

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This command pack is the portable builtin layer, not a complete POSIX userland.

`native/msp-command-pack` is a ReadOS-owned, deterministic command boundary for the neutral [`msp-backend`](../native/msp-backend) contract. It provides bounded, read-only implementations of:

- `pwd`, `echo`, `cat`, `ls`, and the bounded metadata-only `find` subset (the original virtual-workspace slice)
- the bounded metadata-only `du` subset
- the safe local subset of `printf`, `head`, `tail`, and `wc`
- bounded literal-byte `grep` over virtual files and explicit stdin

The command registry remains immutable, case-sensitive, and atomically composed. Every command declares the `read-only` effect.

The cross-host profile is versioned as [`portable_msp_v1.json`](../native/msp-command-pack/profile/portable_msp_v1.json). It freezes 12 commands for the modular runtime ABI and intentionally excludes the compatibility-only `command`, `env`, `type`, and `which` helpers. The shared executable fixtures are in [`portable_msp_v1_fixtures.json`](../native/msp-command-pack/profile/portable_msp_v1_fixtures.json); Rust host tests execute them against the in-memory backend on Windows and Linux, while Windows release smoke and Android arm64 packaging/instrumentation consume the same profile boundary. Android now has a read-only SAF/ContentResolver projection into the virtual workspace, but arm64 device evidence and writable/product workspace integration remain H3.3/H5 work.

## Safe command subset

`printf` accepts `--help`, `--version`, an optional leading `--`, a format, and format arguments. It renders bytes directly and supports the bounded escape/conversion subset documented by the package tests: string, byte/escape, character, signed/unsigned decimal, octal, hexadecimal, and literal-percent conversions with simple flags, width, and precision. Floating point, dynamic width/precision, locale grouping, shell expansion, and process behavior are not implemented.

`head` and `tail` accept explicit unsigned decimal `-c`/`--bytes=`, `-n`/`--lines=`, quiet/verbose, zero-terminated, and `--`. They operate on virtual files and explicit stdin only. `tail` follow/watch modes and obsolete signed/compact count forms are rejected. File reads use ranges no larger than 32 KiB and the backend limit; line scans are bounded by the command's aggregate scan policy (`MAX_COMMAND_SCAN_BYTES`).

`wc` supports only lines, ASCII-whitespace words, and bytes (`-l`, `-w`, `-c` and long forms), in that stable order. It counts bytes without UTF-8 conversion, handles NUL and invalid UTF-8, uses saturating `u64` counters, and streams virtual files through bounded ranges. Unsupported modes such as character count, maximum line length, debug output, and files-from lists are rejected.

### `du`

`du` reports logical virtual bytes from `WorkspaceEntry::size` metadata only. It accepts `-a`/`--all`, `-s`/`--summarize`, `-b`/`--bytes`, and `--`; omitted operands use the virtual current directory. Totals are printed as decimal bytes, a tab, and a canonical virtual path. Without `-s`, directories are emitted in deterministic postorder and a file operand is emitted; `-a` additionally emits descendant files. `-s` emits only each operand total, and takes precedence over `-a` for output selection.

The command filters any `.msp` component, validates direct-child metadata, and uses checked arithmetic for per-entry totals. Recursion depth, visited entries, aggregate metadata path bytes, output bytes, and cooperative cancellation are bounded. Unsupported formatting, block, inode, filesystem, dereference, timestamp, ownership, physical-usage, and host-path behavior is rejected with fixed diagnostics. The command never calls host disk APIs and does not treat `WorkspaceBackend::usage` capacity fields as per-path usage; a provider-reported missing usage capability is surfaced explicitly.


`grep` is intentionally a literal byte/line search rather than a regular-expression command. Its only options are the exact short forms `-n`, `-i`, `-v`, `-c`, `-l`, `-q`, and one `-r`. The first non-option is the pattern, limited to `MAX_GREP_PATTERN_BYTES`; remaining operands are virtual paths or `-` for the caller-provided stdin. Long options, clusters, `--`, repeated `-r`, regex syntax, and other option forms are rejected with exit code 2.

Matching uses a KMP failure table, so each input line is scanned in linear time in its bytes plus the bounded pattern. `-i` folds only ASCII A–Z; all other bytes are compared literally. NUL, invalid UTF-8, and patterns or lines spanning 32 KiB backend ranges are retained without conversion. `-n` prefixes selected lines, `-c` emits selected-line counts, `-l` emits one matching path, and `-q` suppresses output and stops after a match. `-l` takes precedence over `-c`; `-q` takes precedence over both. File labels are included when more than one input is selected.

`-r` enumerates only backend directory metadata, sorts entries by canonical virtual path, and descends depth-first. `.msp` path components are skipped. Entry, recursion-depth, per-read, output, line-retention, and aggregate scan bounds are enforced. A match exits 0, no match or any input/limit error exits 1, and only invalid options or patterns exit 2. Diagnostics are fixed and never include rejected operands, patterns, backend text, or host values.

### `find`

`find` is a metadata-only virtual-workspace traversal. It accepts one virtual start path (or the virtual cwd when omitted), followed by implicit-AND `-name PATTERN`, `-iname PATTERN`, and `-type f`/`-type d` predicates. Name matching is a bounded literal/glob matcher: only `*` and `?` have special meaning, and arbitrary regular expressions are never compiled. Results are canonical virtual paths, one per line, including the start entry when it matches, in deterministic depth-first canonical-path order.

The command uses only provider `stat`/`list` metadata and never reads file bytes or follows symlink, reparse, or special entries; platform providers that cannot represent a safe file/directory entry omit it. `.msp` components are excluded. Recursion depth, metadata entry count, pattern bytes, aggregate metadata scan bytes, output bytes, and cooperative cancellation checks are bounded. `-exec`, `-ok`, `-delete`, `-print`, and other actions/options, host-shaped paths, malformed expressions, and invalid patterns fail with fixed path-free diagnostics. The effect is always read-only.


The pack receives a caller-provided `VirtualPath`, explicit argument values, optional byte stdin, and a `dyn WorkspaceBackend`. It never obtains a host current directory, reads process or environment state, opens a host path, starts a process, invokes a shell, or calls the managed product route. All paths are resolved lexically in the virtual namespace. Host-shaped syntax, traversal, internal `.msp` paths, invalid arguments, and configured byte limits fail closed with fixed diagnostics that do not echo rejected host values, raw operands, numeric arguments, or backend display text.

Command output is binary-safe `Vec<u8>`. `cat`, `head`, `tail`, and `wc` preserve NUL and non-UTF-8 file/stdin bytes. `echo` retains explicit empty arguments. Output is bounded independently on stdout and stderr by the invocation limits. Zero-count selections do not read or open a file. Repeated `-` operands use the one explicit stdin cursor according to each command's documented semantics; no host stream is reopened or replayed.

## Provenance and licensing

This is authored ReadOS code informed by public MSP specifications and conformance descriptions. No raw MSP implementation is copied into this package and no MSP crate or source path is a dependency. See the package [`NOTICE`](../native/msp-command-pack/NOTICE) and [`LICENSE-APACHE-2.0`](../native/msp-command-pack/LICENSE-APACHE-2.0).

## Validation

Focused validation uses:

```text
cargo fmt --all -- --check
cargo test -p msp-command-pack --locked
cargo clippy -p msp-command-pack --all-targets --locked -- -D warnings
cargo package --workspace --list --allow-dirty --locked
pwsh -NoProfile -File scripts/verify-msp-crate-packages.ps1
```

The capability manifest records this as a partial local command-profile slice; it does not claim full upstream MSP command parity.
