# Tests

The active test projects are:

```text
tests/ReadOS.Msp.Tests
tests/ReadOS.App.Tests
```

They currently cover MSP parser/runtime behavior, audit records, and app virtual workspace projections. New MSP work should add tests close to the layer being changed:

- parser tests for quoting, argument shape, and invalid command text;
- runtime tests for success, failure, unknown commands, dry runs, and exit codes;
- workspace tests for virtual path normalization, listing, file reads, and ReadOS projections such as `/transcripts`;
- policy tests for mutating command allow/confirm/deny behavior;
- artifact tests when commands create durable outputs.

Future app-level tests should expand into ViewModel state transitions, service-host sessions, command transcript persistence, and approval flows before UI automation is added.
