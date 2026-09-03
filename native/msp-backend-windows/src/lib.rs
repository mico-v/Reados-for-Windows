//! Windows retained-handle workspace backend for the neutral MSP contract.
//!
//! The host root is bound once and retained as an owned handle. The platform implementation is
//! isolated in `platform.rs`; non-Windows builds remain fail-closed stubs.

use msp_backend::{
    Capability, CapabilityReport, CapabilityState, CapabilityStatus, PlatformProfile,
};

pub const VIRTUAL_MOUNT: &str = "/workspace";

pub fn capability_report() -> CapabilityReport {
    let statuses = [
        (Capability::WorkspaceRead, CapabilityState::Unsupported),
        (Capability::WorkspaceWrite, CapabilityState::Unsupported),
        (Capability::WorkspaceUsage, CapabilityState::Unsupported),
        (Capability::Process, CapabilityState::Unsupported),
        (Capability::Pty, CapabilityState::Unsupported),
        (Capability::EventStreaming, CapabilityState::Unsupported),
        (Capability::Cancellation, CapabilityState::Unsupported),
    ]
    .into_iter()
    .map(|(capability, state)| CapabilityStatus { capability, state })
    .collect();

    CapabilityReport {
        platform: PlatformProfile::Windows,
        statuses,
    }
}

#[cfg(windows)]
pub(crate) fn supported_capability_report() -> CapabilityReport {
    let supported = [Capability::WorkspaceRead, Capability::WorkspaceWrite];
    let all = [
        Capability::WorkspaceRead,
        Capability::WorkspaceWrite,
        Capability::WorkspaceUsage,
        Capability::Process,
        Capability::Pty,
        Capability::EventStreaming,
        Capability::Cancellation,
    ];
    CapabilityReport {
        platform: PlatformProfile::Windows,
        statuses: all
            .into_iter()
            .map(|capability| CapabilityStatus {
                capability,
                state: if supported.contains(&capability) {
                    CapabilityState::Supported
                } else {
                    CapabilityState::Unsupported
                },
            })
            .collect(),
    }
}

#[cfg(windows)]
mod platform;
#[cfg(windows)]
pub use platform::{WindowsBackendError, WindowsWorkspaceBackend};
#[cfg(windows)]
pub type WindowsReadOnlyBackend = WindowsWorkspaceBackend;
#[cfg(windows)]
pub type WindowsReadOpenError = WindowsBackendError;
#[cfg(windows)]
pub type WindowsReadPolicy = msp_backend::BackendLimits;

#[cfg(not(windows))]
mod unsupported_read;
#[cfg(not(windows))]
pub use unsupported_read::{WindowsBackendError, WindowsWorkspaceBackend};
#[cfg(not(windows))]
pub use unsupported_read::{WindowsReadOnlyBackend, WindowsReadOpenError, WindowsReadPolicy};

mod unsupported_process;
pub use unsupported_process::{
    WindowsProcessAdapter, WindowsProcessError, WindowsProcessPolicy, WindowsProgramRule,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_report_matches_the_real_windows_slice() {
        let report = capability_report();
        assert_eq!(report.platform, PlatformProfile::Windows);
        assert_eq!(
            report.state(Capability::WorkspaceRead),
            CapabilityState::Unsupported
        );
        assert_eq!(
            report.state(Capability::WorkspaceWrite),
            CapabilityState::Unsupported
        );
        assert!(!report.supports(Capability::Process));
        let debug = format!("{report:?}");
        assert!(!debug.contains("\\\\"));
        assert!(!debug.contains(":\\"));
    }
}
