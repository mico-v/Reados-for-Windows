# Tests

The active test project is:

```text
tests/ReadOS.Msp.Tests
```

It currently covers MSP parser/runtime behavior and audit records. New MSP work should add tests close to the layer being changed:

- parser tests for quoting, argument shape, and invalid command text;
- runtime tests for success, failure, unknown commands, dry runs, and exit codes;
- workspace tests for virtual path normalization, listing, and file reads;
- policy tests for mutating command allow/confirm/deny behavior;
- artifact tests when commands create durable outputs.

Future app-level tests should focus on ViewModel state transitions, service-host sessions, command transcript persistence, and approval flows before UI automation is added.
