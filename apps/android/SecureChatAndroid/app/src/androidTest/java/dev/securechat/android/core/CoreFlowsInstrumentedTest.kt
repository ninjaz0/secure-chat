package dev.securechat.android.core

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import java.util.UUID
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class CoreFlowsInstrumentedTest {
    @Test
    fun nativeCoreRunsOneToOneAndTemporaryChatFlowsOnDevice() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.noBackupFilesDir, "instrumented-${UUID.randomUUID()}")
        val aliceDir = File(root, "alice").absolutePath
        val bobDir = File(root, "bob").absolutePath
        val relayUrl = "http://127.0.0.1:8787"

        try {
            checkedJson(SecureChatNative.bootstrapJson(aliceDir, "Alice", relayUrl))
            checkedJson(SecureChatNative.bootstrapJson(bobDir, "Bob", relayUrl))

            val aliceInvite = checkedJson(SecureChatNative.inviteJson(aliceDir))
                .getString("invite_uri")
            val bobSnapshot = checkedJson(
                SecureChatNative.addContactJson(bobDir, "Alice", aliceInvite),
            )
            val aliceContactId = bobSnapshot
                .getJSONArray("contacts")
                .getJSONObject(0)
                .getString("id")

            checkedJson(SecureChatNative.sendMessageJson(bobDir, aliceContactId, "hello from emulator"))
            val aliceReport = checkedJson(SecureChatNative.receiveJson(aliceDir))
            val aliceSnapshot = aliceReport.getJSONObject("snapshot")
            assertEquals(1, aliceReport.getInt("received_count"))
            assertEquals(1, aliceSnapshot.getJSONArray("contacts").length())
            assertEquals(0, aliceSnapshot.getJSONArray("temporary_connections").length())
            assertEquals("hello from emulator", aliceSnapshot.getJSONArray("messages").getJSONObject(0).getString("body"))

            val bobContactId = aliceSnapshot
                .getJSONArray("contacts")
                .getJSONObject(0)
                .getString("id")
            checkedJson(SecureChatNative.sendMessageJson(aliceDir, bobContactId, "reply from invite owner"))
            val bobReport = checkedJson(SecureChatNative.receiveJson(bobDir))
            assertEquals(1, bobReport.getInt("received_count"))
            assertTrue(
                jsonArrayContainsBody(
                    bobReport.getJSONObject("snapshot").getJSONArray("messages"),
                    "reply from invite owner",
                ),
            )

            val temporaryInvite = checkedJson(SecureChatNative.temporaryInviteJson(aliceDir))
                .getString("invite_uri")
            val temporaryStart = checkedJson(
                SecureChatNative.startTemporaryConnectionJson(bobDir, temporaryInvite),
            )
            checkedJson(
                SecureChatNative.sendTemporaryMessageJson(
                    bobDir,
                    temporaryStart.getString("connection_id"),
                    "temporary hello from emulator",
                ),
            )
            val temporaryReport = checkedJson(SecureChatNative.receiveJson(aliceDir))
            val temporarySnapshot = temporaryReport.getJSONObject("snapshot")
            assertEquals(1, temporaryReport.getInt("received_count"))
            assertEquals(1, temporarySnapshot.getJSONArray("contacts").length())
            assertEquals(1, temporarySnapshot.getJSONArray("temporary_connections").length())
            assertTrue(
                jsonArrayContainsBody(
                    temporarySnapshot.getJSONArray("temporary_messages"),
                    "temporary hello from emulator",
                ),
            )
        } finally {
            root.deleteRecursively()
        }
    }

    private fun checkedJson(payload: String): JSONObject {
        val json = JSONObject(payload)
        if (json.has("ok") && !json.getBoolean("ok")) {
            fail(json.optString("error", "native core returned ok=false"))
        }
        return json
    }

    private fun jsonArrayContainsBody(array: org.json.JSONArray, body: String): Boolean {
        for (index in 0 until array.length()) {
            if (array.getJSONObject(index).optString("body") == body) {
                return true
            }
        }
        return false
    }
}
