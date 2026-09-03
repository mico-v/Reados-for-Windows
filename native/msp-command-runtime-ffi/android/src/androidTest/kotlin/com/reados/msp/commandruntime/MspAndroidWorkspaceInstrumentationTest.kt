package com.reados.msp.commandruntime

import android.net.Uri
import android.provider.DocumentsContract
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlin.test.assertContentEquals
import kotlin.test.assertEquals
import kotlin.test.assertIs
import org.junit.Test
import org.junit.runner.RunWith

/** Exercises the real ContentResolver -> virtual workspace projection on Android. */
@RunWith(AndroidJUnit4::class)
class MspAndroidWorkspaceInstrumentationTest {
    @Test
    fun contentResolverDocumentBytesProjectThroughVirtualPathIntoRust() {
        // The provider is registered by the androidTest APK itself. Resolve it
        // through the instrumentation/test context rather than the target app
        // context so the test-only authority is visible to ContentResolver.
        val context = InstrumentationRegistry.getInstrumentation().context
        val resolver = context.contentResolver
        val treeUri = DocumentsContract.buildTreeDocumentUri(
            MspTestDocumentsProvider.AUTHORITY,
            MspTestDocumentsProvider.ROOT_ID,
        )
        val opened = MspAndroidContentResolverWorkspaceSource.open(resolver, treeUri)
        val source = assertIs<MspAndroidWorkspaceSourceOpenResult.Success>(opened).source
        val client = MspCommandRuntimeFfiClient.withNative()
        client.openSession().use { session ->
            val projection = MspAndroidWorkspaceProjection(source, session.workspaceHandle())
            val projected = projection.projectFile("/workspace/doc.bin")
            check(projected is MspAndroidWorkspaceResult.Success) {
                "ContentResolver projection failed: ${(projected as MspAndroidWorkspaceResult.Failure).error}"
            }
            val result = session.execute(
                MspVirtualRequest(command = "cat '/workspace/doc.bin'", cwd = "/workspace"),
            )
            assertEquals(0, result.exitCode)
            assertContentEquals(byteArrayOf(0, 0xff.toByte(), 1, 0), result.stdout)
            projection.close()
        }
    }

    @Test
    fun cancellationAndDisclosureBoundariesRemainPathFree() {
        val context = InstrumentationRegistry.getInstrumentation().context
        val resolver = context.contentResolver
        val treeUri: Uri = DocumentsContract.buildTreeDocumentUri(
            MspTestDocumentsProvider.AUTHORITY,
            MspTestDocumentsProvider.ROOT_ID,
        )
        val source = assertIs<MspAndroidWorkspaceSourceOpenResult.Success>(
            MspAndroidContentResolverWorkspaceSource.open(resolver, treeUri),
        ).source
        val token = MspAndroidCancellationToken()
        token.cancel()
        val result = source.readFile(
            MspVirtualPathValidator.requireValid("/workspace/doc.bin"),
            token,
        )
        val failure = assertIs<MspAndroidWorkspaceResult.Failure>(result)
        assertEquals(MspAndroidWorkspaceError.Cancelled, failure.error)
        check(!failure.error.toString().contains(MspTestDocumentsProvider.AUTHORITY))
        source.close()
    }
}
