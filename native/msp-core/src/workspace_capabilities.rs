use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

/// Read-only operations that a WorkspaceFS backend can serve at one path.
///
/// Capabilities are path-scoped because a composite workspace may route each
/// subtree to a different backend. The type is deliberately dependency-free so
/// the native core does not need a third-party bitflag implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct WorkspaceReadCapabilities(u32);

impl WorkspaceReadCapabilities {
    pub const NONE: Self = Self(0);
    pub const STAT: Self = Self(1 << 0);
    pub const LIST_DIRECTORY: Self = Self(1 << 1);
    pub const READ_FILE_RANGE: Self = Self(1 << 2);
    pub const ALL: Self = Self(Self::STAT.0 | Self::LIST_DIRECTORY.0 | Self::READ_FILE_RANGE.0);

    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }
}

impl Default for WorkspaceReadCapabilities {
    fn default() -> Self {
        Self::NONE
    }
}

impl BitOr for WorkspaceReadCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for WorkspaceReadCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for WorkspaceReadCapabilities {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for WorkspaceReadCapabilities {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

/// Write operations that a writable WorkspaceFS backend can serve at one path.
///
/// Capabilities are path-scoped because a composite workspace may route each
/// subtree to a different backend. The type is deliberately dependency-free so
/// the native core does not need a third-party bitflag implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct WorkspaceWriteCapabilities(u32);

impl WorkspaceWriteCapabilities {
    pub const NONE: Self = Self(0);
    pub const CREATE_FILE: Self = Self(1 << 0);
    pub const WRITE_FILE_RANGE: Self = Self(1 << 1);
    pub const RENAME: Self = Self(1 << 2);
    pub const DELETE: Self = Self(1 << 3);
    pub const ALL: Self =
        Self(Self::CREATE_FILE.0 | Self::WRITE_FILE_RANGE.0 | Self::RENAME.0 | Self::DELETE.0);

    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::ALL.0 == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, required: Self) -> bool {
        self.0 & required.0 == required.0
    }
}

impl Default for WorkspaceWriteCapabilities {
    fn default() -> Self {
        Self::NONE
    }
}

impl BitOr for WorkspaceWriteCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for WorkspaceWriteCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for WorkspaceWriteCapabilities {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for WorkspaceWriteCapabilities {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_bits_are_closed_and_composable() {
        let read = WorkspaceReadCapabilities::STAT | WorkspaceReadCapabilities::READ_FILE_RANGE;

        assert!(read.contains(WorkspaceReadCapabilities::STAT));
        assert!(read.contains(WorkspaceReadCapabilities::READ_FILE_RANGE));
        assert!(!read.contains(WorkspaceReadCapabilities::LIST_DIRECTORY));
        assert_eq!(
            WorkspaceReadCapabilities::from_bits(read.bits()),
            Some(read)
        );
        assert_eq!(WorkspaceReadCapabilities::from_bits(1 << 31), None);
    }

    #[test]
    fn write_capability_bits_are_closed_and_composable() {
        let write = WorkspaceWriteCapabilities::CREATE_FILE | WorkspaceWriteCapabilities::RENAME;

        assert!(write.contains(WorkspaceWriteCapabilities::CREATE_FILE));
        assert!(write.contains(WorkspaceWriteCapabilities::RENAME));
        assert!(!write.contains(WorkspaceWriteCapabilities::DELETE));
        assert!(!write.contains(WorkspaceWriteCapabilities::WRITE_FILE_RANGE));
        assert_eq!(
            WorkspaceWriteCapabilities::from_bits(write.bits()),
            Some(write)
        );
        assert_eq!(WorkspaceWriteCapabilities::from_bits(1 << 31), None);
        assert!(WorkspaceWriteCapabilities::default().is_empty());
        assert!(
            WorkspaceWriteCapabilities::ALL.contains(WorkspaceWriteCapabilities::CREATE_FILE)
                && WorkspaceWriteCapabilities::ALL.contains(WorkspaceWriteCapabilities::DELETE)
        );
    }
}
