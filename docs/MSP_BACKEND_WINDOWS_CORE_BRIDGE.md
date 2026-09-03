# Windows core bridge

Architecture authority: [MSP_HYBRID_ARCHITECTURE.md](MSP_HYBRID_ARCHITECTURE.md). This bridge is a migration boundary, not the target portable core.

`msp-backend-windows-core-bridge` is retained as a migration oracle only. It keeps the old
retained-root Windows workspace implementation behind an explicit feature and translates it into
a ReadOS-owned read-only DTO boundary. The supported H3.1 implementation is now
`msp-backend-windows`, which owns the neutral `WorkspaceBackend` contract without a legacy
dependency; this bridge is not product routing and should not receive new features.

See the crate README for the security and portability contract.
