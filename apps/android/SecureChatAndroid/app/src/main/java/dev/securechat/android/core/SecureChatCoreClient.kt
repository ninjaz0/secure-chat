package dev.securechat.android.core

import android.content.Context
import java.io.File
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json

class SecureChatCoreClient(context: Context) {
    private val dataDir: String = File(context.noBackupFilesDir, "SecureChatAndroid")
        .apply { mkdirs() }
        .absolutePath

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
    }

    fun appSnapshot(): AppSnapshot =
        decode(SecureChatNative.appSnapshotJson(dataDir))

    fun bootstrap(displayName: String, relayUrl: String): AppSnapshot =
        decode(SecureChatNative.bootstrapJson(dataDir, displayName, relayUrl))

    fun updateRelay(relayUrl: String): AppSnapshot =
        decode(SecureChatNative.updateRelayJson(dataDir, relayUrl))

    fun ownInvite(): InviteResponse =
        decode(SecureChatNative.inviteJson(dataDir))

    fun temporaryInvite(): TemporaryInviteResponse =
        decode(SecureChatNative.temporaryInviteJson(dataDir))

    fun previewInvite(inviteText: String): InvitePreview =
        decode(SecureChatNative.previewInviteJson(dataDir, inviteText))

    fun addContact(displayName: String, inviteUri: String): AppSnapshot =
        decode(SecureChatNative.addContactJson(dataDir, displayName, inviteUri))

    fun startTemporaryConnection(inviteUri: String): TemporaryStartResponse =
        decode(SecureChatNative.startTemporaryConnectionJson(dataDir, inviteUri))

    fun sendMessage(contactId: String, body: String): AppSnapshot =
        decode(SecureChatNative.sendMessageJson(dataDir, contactId, body))

    fun createGroup(displayName: String): AppSnapshot =
        decode(SecureChatNative.createGroupJson(dataDir, displayName))

    fun addGroupMember(groupId: String, contactId: String): AppSnapshot =
        decode(SecureChatNative.addGroupMemberJson(dataDir, groupId, contactId))

    fun sendGroupMessage(groupId: String, body: String): AppSnapshot =
        decode(SecureChatNative.sendGroupMessageJson(dataDir, groupId, body))

    fun sendTemporaryMessage(connectionId: String, body: String): AppSnapshot =
        decode(SecureChatNative.sendTemporaryMessageJson(dataDir, connectionId, body))

    fun endTemporaryConnection(connectionId: String): AppSnapshot =
        decode(SecureChatNative.endTemporaryConnectionJson(dataDir, connectionId))

    fun receiveMessages(): ReceiveReport =
        decode(SecureChatNative.receiveJson(dataDir))

    fun runSelfTest(): SelfTestResult =
        decode(SecureChatNative.protocolSelfTestJson())

    fun runRelaySmoke(): RelaySmokeResult =
        decode(SecureChatNative.relaySmokeJson())

    fun runP2pProbe(): P2pProbeResult =
        decode(SecureChatNative.p2pProbeJson(dataDir))

    private inline fun <reified T> decode(payload: String): T {
        val maybeError = runCatching { json.decodeFromString<FfiErrorPayload>(payload) }.getOrNull()
        if (maybeError?.ok == false) {
            throw IllegalStateException(maybeError.error ?: "Rust core returned an error")
        }
        return json.decodeFromString(payload)
    }
}
