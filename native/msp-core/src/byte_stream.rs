//! Bounded byte input/output stream primitives.
//!
//! This module is internal plumbing for the future pipeline slice. It mirrors
//! the upstream Swift `MSPCommandStream.swift` input/output stream contracts
//! (`MSPDataInputStream`, `MSPAsyncBytePipe`, `MSPWorkspaceFileInputStream`)
//! without exposing any ABI, pipeline, or managed surface. All items are
//! `pub(crate)` and stay backend-neutral: no host paths ever appear in errors
//! or messages.
//!
//! The stream items are intentionally not yet consumed by the rest of the
//! crate: the pipeline slice that uses them lands later. Until then they are
//! exercised by this module's tests, so `dead_code` is allowed module-wide.

#![allow(dead_code)]

use crate::workspace_fs::ReadOnlyWorkspaceFileSystem;
use crate::workspace_path::{VirtualPath, WorkspacePathError};
use std::collections::VecDeque;

/// Default chunk size for file-backed readers (mirrors upstream `chunkSize`).
pub(crate) const DEFAULT_STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Default maximum number of buffered chunks in a bounded pipe
/// (mirrors upstream `maxBufferedChunks: 32`).
pub(crate) const DEFAULT_PIPE_MAX_CHUNKS: usize = 32;

/// Default maximum number of buffered bytes in a bounded pipe.
pub(crate) const DEFAULT_PIPE_MAX_BYTES: usize = 1024 * 1024;

/// Stream errors. `WriteToClosed` carries a static "Bad file descriptor"-shaped
/// message and never a host path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamError {
    WriteToClosed(String),
    BufferLimitExceeded,
    BrokenPipe,
    /// Reserved for the pipeline slice. Current readers report EOF as
    /// `Ok(None)` instead of surfacing this error.
    ReadAfterClose,
    Workspace(WorkspacePathError),
}

/// Byte input stream contract (mirrors `MSPCommandInputStream`).
pub(crate) trait MspByteReader {
    /// Reads up to `max_bytes` bytes. `Ok(None)` means end-of-stream.
    fn read(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, StreamError>;

    fn close_read(&mut self) -> Result<(), StreamError>;
}

/// Byte output stream contract (mirrors `MSPCommandOutputStream`).
pub(crate) trait MspByteWriter {
    fn write(&mut self, data: &[u8]) -> Result<(), StreamError>;

    fn close_write(&mut self) -> Result<(), StreamError>;
}

/// In-memory reader over an owned byte buffer (mirrors `MSPDataInputStream`).
pub(crate) struct MspDataReader {
    data: Vec<u8>,
    offset: usize,
    closed: bool,
}

impl MspDataReader {
    pub(crate) fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            offset: 0,
            closed: false,
        }
    }
}

impl MspByteReader for MspDataReader {
    fn read(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, StreamError> {
        if self.closed || self.offset >= self.data.len() {
            return Ok(None);
        }
        let length = max_bytes.max(1);
        let end = self.offset.saturating_add(length).min(self.data.len());
        let chunk = self.data[self.offset..end].to_vec();
        self.offset = end;
        Ok(Some(chunk))
    }

    fn close_read(&mut self) -> Result<(), StreamError> {
        self.closed = true;
        Ok(())
    }
}

/// Synchronous bounded byte pipe implementing both stream contracts.
///
/// Mirrors the intent of `MSPAsyncBytePipe(maxBufferedChunks: 32)`. The pipe is
/// deliberately non-blocking: a synchronous block here would deadlock the
/// single-threaded core, so a full buffer is reported as `BufferLimitExceeded`
/// instead.
pub(crate) struct BoundedBytePipe {
    chunks: VecDeque<Vec<u8>>,
    buffered_bytes: usize,
    max_chunks: usize,
    max_bytes: usize,
    writer_closed: bool,
    reader_closed: bool,
}

impl BoundedBytePipe {
    pub(crate) fn new() -> Self {
        Self::with_limits(DEFAULT_PIPE_MAX_CHUNKS, DEFAULT_PIPE_MAX_BYTES)
    }

    /// Constructs a pipe with explicit caps (used by tests for tiny bounds).
    pub(crate) fn with_limits(max_chunks: usize, max_bytes: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            buffered_bytes: 0,
            max_chunks: max_chunks.max(1),
            max_bytes: max_bytes.max(1),
            writer_closed: false,
            reader_closed: false,
        }
    }
}

impl Default for BoundedBytePipe {
    fn default() -> Self {
        Self::new()
    }
}

impl MspByteWriter for BoundedBytePipe {
    fn write(&mut self, data: &[u8]) -> Result<(), StreamError> {
        if data.is_empty() {
            return Ok(());
        }
        if self.reader_closed {
            return Err(StreamError::BrokenPipe);
        }
        if self.writer_closed {
            return Err(StreamError::WriteToClosed(
                "Bad file descriptor".to_string(),
            ));
        }
        if self.chunks.len() >= self.max_chunks
            || self.buffered_bytes.saturating_add(data.len()) > self.max_bytes
        {
            return Err(StreamError::BufferLimitExceeded);
        }
        self.buffered_bytes = self.buffered_bytes.saturating_add(data.len());
        self.chunks.push_back(data.to_vec());
        Ok(())
    }

    fn close_write(&mut self) -> Result<(), StreamError> {
        self.writer_closed = true;
        Ok(())
    }
}

impl MspByteReader for BoundedBytePipe {
    fn read(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, StreamError> {
        if self.reader_closed {
            return Ok(None);
        }
        let length = max_bytes.max(1);
        while let Some(front_len) = self.chunks.front().map(Vec::len) {
            if front_len == 0 {
                self.chunks.pop_front();
                continue;
            }
            if front_len <= length {
                let chunk = self.chunks.pop_front().expect("front chunk present");
                self.buffered_bytes = self.buffered_bytes.saturating_sub(chunk.len());
                return Ok(Some(chunk));
            }
            let head: Vec<u8> = self
                .chunks
                .front_mut()
                .expect("front chunk present")
                .drain(..length)
                .collect();
            self.buffered_bytes = self.buffered_bytes.saturating_sub(head.len());
            return Ok(Some(head));
        }
        Ok(if self.writer_closed {
            None
        } else {
            Some(vec![])
        })
    }

    fn close_read(&mut self) -> Result<(), StreamError> {
        self.reader_closed = true;
        self.chunks.clear();
        self.buffered_bytes = 0;
        Ok(())
    }
}

/// Workspace-file-backed reader (mirrors `MSPWorkspaceFileInputStream`).
///
/// Reads until `read_file_range` returns an empty slice, so no STAT capability
/// dependency is required to detect EOF.
pub(crate) struct MspWorkspaceFileReader<'a> {
    workspace: &'a dyn ReadOnlyWorkspaceFileSystem,
    path: VirtualPath,
    offset: u64,
    chunk_size: usize,
    closed: bool,
}

impl<'a> MspWorkspaceFileReader<'a> {
    pub(crate) fn new(workspace: &'a dyn ReadOnlyWorkspaceFileSystem, path: VirtualPath) -> Self {
        Self::with_chunk_size(workspace, path, DEFAULT_STREAM_CHUNK_SIZE)
    }

    pub(crate) fn with_chunk_size(
        workspace: &'a dyn ReadOnlyWorkspaceFileSystem,
        path: VirtualPath,
        chunk_size: usize,
    ) -> Self {
        Self {
            workspace,
            path,
            offset: 0,
            chunk_size: chunk_size.max(1),
            closed: false,
        }
    }
}

impl MspByteReader for MspWorkspaceFileReader<'_> {
    fn read(&mut self, max_bytes: usize) -> Result<Option<Vec<u8>>, StreamError> {
        if self.closed {
            return Ok(None);
        }
        let requested = max_bytes.max(1).min(self.chunk_size);
        let chunk = self
            .workspace
            .read_file_range(&self.path, self.offset, requested)
            .map_err(StreamError::Workspace)?;
        if chunk.is_empty() {
            return Ok(None);
        }
        let length = u64::try_from(chunk.len()).map_err(|_| workspace_read_io_error(&self.path))?;
        self.offset = self
            .offset
            .checked_add(length)
            .ok_or_else(|| workspace_read_io_error(&self.path))?;
        Ok(Some(chunk))
    }

    fn close_read(&mut self) -> Result<(), StreamError> {
        self.closed = true;
        Ok(())
    }
}

fn workspace_read_io_error(path: &VirtualPath) -> StreamError {
    StreamError::Workspace(WorkspacePathError::Io {
        path: path.to_string(),
        operation: "read".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_fs::{WorkspaceDirectoryEntry, WorkspaceFileInfo, WorkspaceFileType};
    use crate::workspace_path::WorkspacePathPolicy;
    use std::collections::BTreeMap;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[cfg(windows)]
    use std::path::PathBuf;
    #[cfg(windows)]
    use std::time::{SystemTime, UNIX_EPOCH};

    /// In-memory `ReadOnlyWorkspaceFileSystem` mock with clamped range reads.
    struct TestWorkspace {
        policy: WorkspacePathPolicy,
        files: BTreeMap<VirtualPath, Vec<u8>>,
    }

    impl TestWorkspace {
        fn new(files: BTreeMap<VirtualPath, Vec<u8>>) -> Self {
            Self {
                policy: WorkspacePathPolicy::new(Vec::<String>::new()),
                files,
            }
        }
    }

    impl ReadOnlyWorkspaceFileSystem for TestWorkspace {
        fn policy(&self) -> &WorkspacePathPolicy {
            &self.policy
        }

        fn stat(&self, path: &VirtualPath) -> Result<WorkspaceFileInfo, WorkspacePathError> {
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            Ok(WorkspaceFileInfo {
                virtual_path: path.clone(),
                file_type: WorkspaceFileType::RegularFile,
                size: Some(data.len() as u64),
                modification_time_unix_ms: Some(1_700_000_000_000),
                file_identity: None,
            })
        }

        fn list_directory(
            &self,
            path: &VirtualPath,
        ) -> Result<Vec<WorkspaceDirectoryEntry>, WorkspacePathError> {
            if self.files.contains_key(path) {
                Err(WorkspacePathError::NotDirectory(path.to_string()))
            } else {
                Err(WorkspacePathError::NotFound(path.to_string()))
            }
        }

        fn read_file_range(
            &self,
            path: &VirtualPath,
            offset: u64,
            length: usize,
        ) -> Result<Vec<u8>, WorkspacePathError> {
            let data = self
                .files
                .get(path)
                .ok_or_else(|| WorkspacePathError::NotFound(path.to_string()))?;
            let start = usize::try_from(offset).unwrap_or(usize::MAX);
            if start >= data.len() {
                return Ok(Vec::new());
            }
            let end = start.saturating_add(length).min(data.len());
            Ok(data[start..end].to_vec())
        }
    }

    #[test]
    fn data_reader_round_trip_and_eof() {
        let mut reader = MspDataReader::new(b"abcdef".to_vec());
        assert_eq!(reader.read(1024).unwrap().unwrap(), b"abcdef".to_vec());
        assert_eq!(reader.read(1024).unwrap(), None);
        assert_eq!(reader.read(1).unwrap(), None);
    }

    #[test]
    fn data_reader_chunk_boundaries_and_zero_clamp() {
        let mut reader = MspDataReader::new(b"abcdefgh".to_vec());
        assert_eq!(reader.read(3).unwrap().unwrap(), b"abc".to_vec());
        assert_eq!(reader.read(3).unwrap().unwrap(), b"def".to_vec());
        assert_eq!(reader.read(3).unwrap().unwrap(), b"gh".to_vec());
        assert_eq!(reader.read(3).unwrap(), None);

        // max_bytes == 0 clamps to a single byte.
        let mut reader = MspDataReader::new(b"xy".to_vec());
        assert_eq!(reader.read(0).unwrap().unwrap(), b"x".to_vec());
    }

    #[test]
    fn data_reader_read_after_close_returns_none_and_close_is_idempotent() {
        let mut reader = MspDataReader::new(b"abc".to_vec());
        reader.close_read().unwrap();
        assert_eq!(reader.read(10).unwrap(), None);
        reader.close_read().unwrap();
        assert_eq!(reader.read(10).unwrap(), None);
    }

    #[test]
    fn pipe_round_trip() {
        let mut pipe = BoundedBytePipe::new();
        pipe.write(b"hello ").unwrap();
        pipe.write(b"world").unwrap();
        pipe.close_write().unwrap();
        let mut collected = Vec::new();
        while let Some(chunk) = pipe.read(64).unwrap() {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, b"hello world".to_vec());
    }

    #[test]
    fn pipe_read_splits_at_max_bytes_and_retains_tail() {
        let mut pipe = BoundedBytePipe::new();
        pipe.write(b"0123456789").unwrap();
        assert_eq!(pipe.read(4).unwrap().unwrap(), b"0123".to_vec());
        assert_eq!(pipe.read(4).unwrap().unwrap(), b"4567".to_vec());
        assert_eq!(pipe.read(4).unwrap().unwrap(), b"89".to_vec());
        // Buffer empty but writer open: "empty now, not EOF".
        assert_eq!(pipe.read(4).unwrap().unwrap(), Vec::<u8>::new());
        pipe.close_write().unwrap();
        assert_eq!(pipe.read(4).unwrap(), None);
    }

    #[test]
    fn pipe_overflow_is_buffer_limit_exceeded() {
        let mut chunk_capped = BoundedBytePipe::with_limits(2, 1024);
        chunk_capped.write(b"a").unwrap();
        chunk_capped.write(b"b").unwrap();
        assert_eq!(
            chunk_capped.write(b"c"),
            Err(StreamError::BufferLimitExceeded)
        );

        let mut byte_capped = BoundedBytePipe::with_limits(10, 4);
        byte_capped.write(b"ab").unwrap();
        byte_capped.write(b"cd").unwrap();
        assert_eq!(
            byte_capped.write(b"e"),
            Err(StreamError::BufferLimitExceeded)
        );
    }

    #[test]
    fn pipe_write_after_close_and_reader_closed_errors() {
        let mut pipe = BoundedBytePipe::new();
        pipe.close_write().unwrap();
        assert_eq!(
            pipe.write(b"late"),
            Err(StreamError::WriteToClosed(
                "Bad file descriptor".to_string()
            ))
        );

        let mut pipe = BoundedBytePipe::new();
        pipe.close_read().unwrap();
        assert_eq!(
            pipe.write(b"after-reader-closed"),
            Err(StreamError::BrokenPipe)
        );
    }

    #[test]
    fn pipe_empty_buffer_open_read_is_some_empty_and_close_write_is_true_eof() {
        let mut pipe = BoundedBytePipe::new();
        // No data and writer still open: empty now, not EOF.
        assert_eq!(pipe.read(16).unwrap().unwrap(), Vec::<u8>::new());
        assert_eq!(pipe.read(16).unwrap().unwrap(), Vec::<u8>::new());
        pipe.close_write().unwrap();
        assert_eq!(pipe.read(16).unwrap(), None);
    }

    #[test]
    fn pipe_close_idempotency_and_empty_writes_are_noop() {
        let mut pipe = BoundedBytePipe::with_limits(2, 16);
        pipe.close_read().unwrap();
        pipe.close_read().unwrap();
        assert!(pipe.reader_closed);
        assert_eq!(pipe.write(b"x"), Err(StreamError::BrokenPipe));

        let mut pipe = BoundedBytePipe::with_limits(2, 16);
        pipe.close_write().unwrap();
        pipe.close_write().unwrap();
        assert!(pipe.writer_closed);

        let mut pipe = BoundedBytePipe::with_limits(2, 16);
        pipe.write(&[]).unwrap();
        pipe.write(&[]).unwrap();
        assert_eq!(pipe.buffered_bytes, 0);
        assert!(pipe.chunks.is_empty());

        // Empty writes stay no-ops even after both sides are closed.
        let mut pipe = BoundedBytePipe::with_limits(2, 16);
        pipe.close_write().unwrap();
        pipe.close_read().unwrap();
        assert_eq!(pipe.write(&[]), Ok(()));
    }

    #[test]
    fn pipe_tiny_bounds_reject_excess() {
        let mut pipe = BoundedBytePipe::with_limits(1, 1);
        assert_eq!(pipe.write(b"a"), Ok(()));
        assert_eq!(pipe.write(b"b"), Err(StreamError::BufferLimitExceeded));

        // A single oversized chunk is rejected by the byte cap.
        let mut pipe = BoundedBytePipe::with_limits(4, 3);
        assert_eq!(pipe.write(b"abcd"), Err(StreamError::BufferLimitExceeded));
    }

    #[test]
    fn closed_or_tiny_pipe_never_panics_on_malformed_use() {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut pipe = BoundedBytePipe::with_limits(1, 1);
            let _ = pipe.read(0);
            let _ = pipe.read(usize::MAX);
            let _ = pipe.write(b"x");
            let _ = pipe.write(b"y");
            let _ = pipe.write(&[]);
            let _ = pipe.close_read();
            let _ = pipe.close_read();
            let _ = pipe.read(usize::MAX);
            let _ = pipe.write(b"z");
            let _ = pipe.close_write();
            let _ = pipe.close_write();
        }));
        assert!(outcome.is_ok());

        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let mut pipe = BoundedBytePipe::new();
            let _ = pipe.close_write();
            let _ = pipe.write(b"late");
            let _ = pipe.read(1);
        }));
        assert!(outcome.is_ok());
    }

    #[test]
    fn workspace_reader_small_file() {
        let path = VirtualPath::resolve("/small.txt", "/").unwrap();
        let workspace = TestWorkspace::new(BTreeMap::from([(path.clone(), b"hello".to_vec())]));
        let mut reader = MspWorkspaceFileReader::new(&workspace, path);
        assert_eq!(reader.read(2).unwrap().unwrap(), b"he".to_vec());
        assert_eq!(reader.read(100).unwrap().unwrap(), b"llo".to_vec());
        assert_eq!(reader.read(100).unwrap(), None);
    }

    #[test]
    fn workspace_reader_large_multi_chunk_file_and_offset_tracking() {
        let path = VirtualPath::resolve("/big.bin", "/").unwrap();
        let content: Vec<u8> = (0..200u16).map(|i| (i % 251) as u8).collect();
        let workspace = TestWorkspace::new(BTreeMap::from([(path.clone(), content.clone())]));

        // chunk_size 64 drives reads into 64-byte slices.
        let mut reader = MspWorkspaceFileReader::with_chunk_size(&workspace, path.clone(), 64);
        let mut collected = Vec::new();
        while let Some(chunk) = reader.read(1024).unwrap() {
            assert!(chunk.len() <= 64);
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, content);

        // Offset tracking: a 30-byte read advances the file offset by 30.
        let mut reader = MspWorkspaceFileReader::with_chunk_size(&workspace, path, 64);
        assert_eq!(reader.read(30).unwrap().unwrap(), content[..30]);
        assert_eq!(reader.read(1024).unwrap().unwrap(), content[30..94]);
        assert_eq!(reader.offset, 94);
    }

    #[test]
    fn workspace_reader_eof_and_read_after_close() {
        let path = VirtualPath::resolve("/small.txt", "/").unwrap();
        let workspace = TestWorkspace::new(BTreeMap::from([(path.clone(), b"hello".to_vec())]));
        let mut reader = MspWorkspaceFileReader::new(&workspace, path);
        assert_eq!(reader.read(100).unwrap().unwrap(), b"hello".to_vec());
        assert_eq!(reader.read(100).unwrap(), None);
        reader.close_read().unwrap();
        assert_eq!(reader.read(100).unwrap(), None);
        reader.close_read().unwrap();
    }

    #[cfg(windows)]
    struct TemporaryDirectory(PathBuf);

    #[cfg(windows)]
    impl TemporaryDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("msp-core-{label}-{nonce}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    #[cfg(windows)]
    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(windows)]
    #[test]
    fn workspace_reader_streams_a_real_windows_local_workspace_file() {
        use crate::WindowsLocalReadOnlyWorkspace;

        let root = TemporaryDirectory::new("byte-stream-workspace");
        let content: Vec<u8> = (0..150u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(root.0.join("stream.bin"), &content).unwrap();
        let workspace = WindowsLocalReadOnlyWorkspace::open(&root.0).unwrap();
        let path = workspace.resolve("/stream.bin", "/").unwrap();

        let mut reader = MspWorkspaceFileReader::with_chunk_size(&workspace, path.clone(), 64);
        let mut collected = Vec::new();
        while let Some(chunk) = reader.read(1024).unwrap() {
            collected.extend_from_slice(&chunk);
        }
        assert_eq!(collected, content);
        assert_eq!(reader.read(16).unwrap(), None);
    }
}
