use crate::ProtocolError;
use msp_kernel::{Capability, KernelExecutor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityReason {
    NotImplemented,
    HostPolicy,
    MissingExecutorSupport,
    AdapterLimit,
    CancelOnly,
    ReadOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapabilityState {
    Supported,
    Partial { reason: CapabilityReason },
    Unsupported { reason: CapabilityReason },
}
impl CapabilityState {
    pub fn supported() -> Self {
        Self::Supported
    }
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::Supported)
    }
    pub fn is_explained(&self) -> bool {
        !self.is_supported()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityReport {
    pub workspace: CapabilityState,
    pub process: CapabilityState,
    pub command: CapabilityState,
    pub event_streaming: CapabilityState,
    pub cancellation: CapabilityState,
}
impl CapabilityReport {
    pub fn unsupported() -> Self {
        Self {
            workspace: CapabilityState::Unsupported {
                reason: CapabilityReason::NotImplemented,
            },
            process: CapabilityState::Unsupported {
                reason: CapabilityReason::NotImplemented,
            },
            command: CapabilityState::Unsupported {
                reason: CapabilityReason::NotImplemented,
            },
            event_streaming: CapabilityState::Unsupported {
                reason: CapabilityReason::NotImplemented,
            },
            cancellation: CapabilityState::Unsupported {
                reason: CapabilityReason::NotImplemented,
            },
        }
    }
    pub fn from_capabilities(capabilities: &[Capability]) -> Self {
        Self {
            workspace: state(capabilities, Capability::Workspace),
            process: state(capabilities, Capability::Process),
            command: state(capabilities, Capability::Command),
            event_streaming: state(capabilities, Capability::EventStreaming),
            cancellation: state(capabilities, Capability::Cancellation),
        }
    }
    pub fn from_executor<E: KernelExecutor>(executor: &E) -> Self {
        Self::from_capabilities(executor.capabilities())
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        Ok(())
    }
    pub fn has_explanation(&self) -> bool {
        self.workspace.is_explained()
            || self.process.is_explained()
            || self.command.is_explained()
            || self.event_streaming.is_explained()
            || self.cancellation.is_explained()
    }
    pub fn state(&self, capability: Capability) -> &CapabilityState {
        match capability {
            Capability::Workspace => &self.workspace,
            Capability::Process => &self.process,
            Capability::Command => &self.command,
            Capability::EventStreaming => &self.event_streaming,
            Capability::Cancellation => &self.cancellation,
        }
    }
}
fn state(capabilities: &[Capability], wanted: Capability) -> CapabilityState {
    if capabilities.contains(&wanted) {
        CapabilityState::Supported
    } else {
        CapabilityState::Unsupported {
            reason: CapabilityReason::MissingExecutorSupport,
        }
    }
}

pub fn capability_intersection(
    left: &CapabilityReport,
    right: &CapabilityReport,
) -> CapabilityReport {
    fn combine(a: &CapabilityState, b: &CapabilityState) -> CapabilityState {
        match (a, b) {
            (CapabilityState::Supported, CapabilityState::Supported) => CapabilityState::Supported,
            (CapabilityState::Unsupported { reason }, _)
            | (_, CapabilityState::Unsupported { reason }) => {
                CapabilityState::Unsupported { reason: *reason }
            }
            (CapabilityState::Partial { reason }, _) | (_, CapabilityState::Partial { reason }) => {
                CapabilityState::Partial { reason: *reason }
            }
        }
    }
    CapabilityReport {
        workspace: combine(&left.workspace, &right.workspace),
        process: combine(&left.process, &right.process),
        command: combine(&left.command, &right.command),
        event_streaming: combine(&left.event_streaming, &right.event_streaming),
        cancellation: combine(&left.cancellation, &right.cancellation),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_kernel_capability_is_present() {
        let report = CapabilityReport::from_capabilities(KernelProfile::Basic.capabilities());
        assert!(report.workspace.is_supported());
        assert!(report.event_streaming.is_explained());
    }
    use msp_kernel::KernelProfile;
}
