# Tests

The active test projects are:

```text
tests/ReadOS.Msp.Tests
tests/ReadOS.App.Tests
```

They currently cover MSP parser/runtime behavior, audit records, structured diagnostics, streaming command events, transcript progress state, durable session records, artifact provenance manifests, app virtual workspace projections, approval-gated host commands, and chat command persistence. New MSP work should add tests close to the layer being changed:

- parser tests for quoting, argument shape, and invalid command text;
- runtime tests for success, failure, unknown commands, dry runs, and exit codes;
- diagnostic tests for stable codes, targets, and recovery hints on failure paths;
- event tests for started, policy, progress, completed, and canceled streaming states;
- workspace tests for virtual path normalization, listing, file reads, and ReadOS projections such as `/sessions` and `/transcripts`;
- session/transcript tests for grouping, diagnostics, recovery hints, progress/cancel state, and stale record cleanup surfaced in the workbench;
- policy tests for mutating command allow/confirm/deny behavior and approval previews;
- artifact tests when commands create durable outputs, including returned artifact metadata, automatic source provenance, and `/artifacts/*.manifest.json` projections.

Future app-level tests should expand into workflow state transitions, richer service-host diagnostics, command transcript persistence, and approval flows before UI automation is added.
