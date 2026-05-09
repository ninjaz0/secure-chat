package dev.securechat.android.core

class SecureChatNative private constructor() {
    companion object {
        init {
            System.loadLibrary("secure_chat_ffi")
        }

        @JvmStatic external fun protocolSelfTestJson(): String
        @JvmStatic external fun relaySmokeJson(): String
        @JvmStatic external fun appSnapshotJson(dataDir: String): String
        @JvmStatic external fun bootstrapJson(dataDir: String, displayName: String, relayUrl: String): String
        @JvmStatic external fun updateRelayJson(dataDir: String, relayUrl: String): String
        @JvmStatic external fun inviteJson(dataDir: String): String
        @JvmStatic external fun temporaryInviteJson(dataDir: String): String
        @JvmStatic external fun previewInviteJson(dataDir: String, inviteText: String): String
        @JvmStatic external fun addContactJson(dataDir: String, displayName: String, inviteUri: String): String
        @JvmStatic external fun updateContactDisplayNameJson(dataDir: String, contactId: String, displayName: String): String
        @JvmStatic external fun deleteContactJson(dataDir: String, contactId: String): String
        @JvmStatic external fun startTemporaryConnectionJson(dataDir: String, inviteUri: String): String
        @JvmStatic external fun sendMessageJson(dataDir: String, contactId: String, body: String): String
        @JvmStatic external fun sendAttachmentJson(dataDir: String, threadKind: String, threadId: String, filePath: String, kind: String): String
        @JvmStatic external fun sendBurnMessageJson(dataDir: String, threadKind: String, threadId: String, body: String): String
        @JvmStatic external fun openBurnMessageJson(dataDir: String, threadKind: String, threadId: String, messageId: String): String
        @JvmStatic external fun createGroupJson(dataDir: String, displayName: String): String
        @JvmStatic external fun addGroupMemberJson(dataDir: String, groupId: String, contactId: String): String
        @JvmStatic external fun sendGroupMessageJson(dataDir: String, groupId: String, body: String): String
        @JvmStatic external fun sendTemporaryMessageJson(dataDir: String, connectionId: String, body: String): String
        @JvmStatic external fun endTemporaryConnectionJson(dataDir: String, connectionId: String): String
        @JvmStatic external fun importStickerJson(dataDir: String, filePath: String, displayName: String): String
        @JvmStatic external fun deleteStickerJson(dataDir: String, stickerId: String): String
        @JvmStatic external fun receiveJson(dataDir: String): String
        @JvmStatic external fun p2pProbeJson(dataDir: String): String
    }
}
