package com.reados.msp.commandruntime

import kotlin.test.Test
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class MspAndroidWorkspaceProjectionTest {
    @Test
    fun projectsBinaryDocumentIntoTheNativeVirtualWorkspace() {
        val source = FakeWorkspaceSource(
            "/workspace/doc.bin" to byteArrayOf(0, 0xff.toByte(), 1, 0),
        )
        val bridge = RecordingBridge()
        val workspace = MspCommandRuntimeFfiWorkspaceHandle(bridge, 7L)
        val projection = MspAndroidWorkspaceProjection(source, workspace)

        assertIs<MspAndroidWorkspaceResult.Success<Unit>>(
            projection.projectFile("/workspace/doc.bin"),
        )
        assertContentEquals(
            byteArrayOf(0, 0xff.toByte(), 1, 0),
            bridge.files["/workspace/doc.bin"],
        )
        projection.close()
        workspace.close()
        assertEquals(1, source.closeCount)
    }

    @Test
    fun rejectsHostShapedOrOutsidePathsWithoutCallingTheProvider() {
        val source = FakeWorkspaceSource()
        val bridge = RecordingBridge()
        val workspace = MspCommandRuntimeFfiWorkspaceHandle(bridge, 9L)
        val projection = MspAndroidWorkspaceProjection(source, workspace)

        val result = projection.projectFile("C:\\secret\\document.pdf")
        val failure = assertIs<MspAndroidWorkspaceResult.Failure>(result)
        assertEquals(MspAndroidWorkspaceError.OutsideMount, failure.error)
        assertEquals(0, source.readCount)
        assertTrue(!failure.error.toString().contains("secret"))
        projection.close()
        workspace.close()
    }

    @Test
    fun cancellationAndCloseAreObservableAndDoNotTouchNativeWorkspace() {
        val source = FakeWorkspaceSource("/workspace/doc.txt" to byteArrayOf(1))
        val bridge = RecordingBridge()
        val workspace = MspCommandRuntimeFfiWorkspaceHandle(bridge, 11L)
        val token = MspAndroidCancellationToken()
        token.cancel()
        val projection = MspAndroidWorkspaceProjection(source, workspace, token)

        val cancelled = assertIs<MspAndroidWorkspaceResult.Failure>(
            projection.projectFile("/workspace/doc.txt"),
        )
        assertEquals(MspAndroidWorkspaceError.Cancelled, cancelled.error)
        assertTrue(bridge.files.isEmpty())

        projection.close()
        val closed = assertIs<MspAndroidWorkspaceResult.Failure>(
            projection.projectFile("/workspace/doc.txt"),
        )
        assertEquals(MspAndroidWorkspaceError.Closed, closed.error)
        workspace.close()
    }

    private class FakeWorkspaceSource(vararg entries: Pair<String, ByteArray>) :
        MspAndroidWorkspaceSource {
        private val files = entries.associate { it.first to it.second.clone() }
        var readCount = 0
            private set
        var closeCount = 0
            private set

        override fun readFile(
            path: MspVirtualPath,
            cancellation: MspAndroidCancellation,
        ): MspAndroidWorkspaceResult<ByteArray> {
            readCount++
            if (cancellation.isCancelled()) {
                return MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.Cancelled)
            }
            return files[path.value]?.let { MspAndroidWorkspaceResult.Success(it.clone()) }
                ?: MspAndroidWorkspaceResult.Failure(MspAndroidWorkspaceError.NotFound)
        }

        override fun close() {
            closeCount++
        }
    }

    private class RecordingBridge : MspCommandRuntimeFfiBridge {
        val files = linkedMapOf<String, ByteArray>()

        override fun abiVersion(): Int = MspCommandRuntimeFfiAbi.ABI

        override fun versionData(): MspFfiByteBuffer =
            MspFfiByteBuffer(MspCommandRuntimeFfiAbi.VERSION.toByteArray())

        override fun runtimeCreate(): Long = 1L
        override fun runtimeFree(runtime: Long) = Unit
        override fun workspaceCreate(): Long = 1L
        override fun workspaceFree(workspace: Long) = Unit

        override fun workspacePutFile(
            workspace: Long,
            path: ByteArray?,
            pathLength: Int,
            data: ByteArray?,
            dataLength: Int,
        ): Int {
            val pathText = path!!.copyOf(pathLength).toString(Charsets.UTF_8)
            files[pathText] = data!!.copyOf(dataLength)
            return MspCommandRuntimeFfiAbi.STATUS_OK
        }

        override fun executeJson(
            runtime: Long,
            workspace: Long,
            request: ByteArray?,
            requestLength: Int,
        ): Long = 1L

        override fun resultExitCode(result: Long): Int = 0
        override fun resultStdoutData(result: Long): MspFfiByteBuffer = MspFfiByteBuffer(ByteArray(0))
        override fun resultStderrData(result: Long): MspFfiByteBuffer = MspFfiByteBuffer(ByteArray(0))
        override fun resultDiagnosticData(result: Long): MspFfiByteBuffer = MspFfiByteBuffer(ByteArray(0))
        override fun resultFree(result: Long) = Unit
    }
}
