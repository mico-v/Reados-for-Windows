# Continuous Development Goal Prompt

```text
Continue ReadOS toward the hybrid MSP architecture defined in PRODUCT_GOAL.md, DEVELOPMENT_PLAN.md, docs/MSP_HYBRID_ARCHITECTURE.md, and docs/DEVELOPMENT_TRACKER.md. Inspect the current worktree, preserve user changes, and implement the highest-priority unblocked tracker item as one complete vertical slice. Keep policy/approval/audit in the host, portable command semantics in the modular Rust runtime, and filesystem/process/PTY behavior in platform backends. Prefer small virtual-workspace builtins; integrate complex tools through verified runtime providers; never expose arbitrary shell or host paths. Add focused tests and target-platform evidence, run proportionate verification, update the tracker with exact results and limitations, and continue until one real acceptance gap is closed.
```
