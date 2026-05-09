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

    fun updateContactDisplayName(contactId: String, displayName: String): AppSnapshot =
        decode(SecureChatNative.updateContactDisplayNameJson(dataDir, contactId, displayName))

    fun deleteContact(contactId: String): AppSnapshot =
        decode(SecureChatNative.deleteContactJson(dataDir, contactId))

    fun startTemporaryConnection(inviteUri: String): TemporaryStartResponse =
        decode(SecureChatNative.startTemporaryConnectionJson(dataDir, inviteUri))

    fun sendMessage(contactId: String, body: String): AppSnapshot =
        decode(SecureChatNative.sendMessageJson(dataDir, contactId, body))

    fun sendAttachment(threadKind: String, threadId: String, filePath: String, kind: String): SendAttachmentResponse =
        decode(SecureChatNative.sendAttachmentJson(dataDir, threadKind, threadId, filePath, kind))

    fun sendBurnMessage(threadKind: String, threadId: String, body: String): AppSnapshot =
        decode(SecureChatNative.sendBurnMessageJson(dataDir, threadKind, threadId, body))

    fun openBurnMessage(threadKind: String, threadId: String, messageId: String): AppSnapshot =
        decode(SecureChatNative.openBurnMessageJson(dataDir, threadKind, threadId, messageId))

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

    fun importSticker(filePath: String, displayName: String): ImportStickerResponse =
        decode(SecureChatNative.importStickerJson(dataDir, filePath, displayName))

    fun deleteSticker(stickerId: String): AppSnapshot =
        decode(SecureChatNative.deleteStickerJson(dataDir, stickerId))

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
