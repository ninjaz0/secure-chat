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
        @JvmStatic external fun startTemporaryConnectionJson(dataDir: String, inviteUri: String): String
        @JvmStatic external fun sendMessageJson(dataDir: String, contactId: String, body: String): String
        @JvmStatic external fun sendTemporaryMessageJson(dataDir: String, connectionId: String, body: String): String
        @JvmStatic external fun endTemporaryConnectionJson(dataDir: String, connectionId: String): String
        @JvmStatic external fun receiveJson(dataDir: String): String
        @JvmStatic external fun p2pProbeJson(dataDir: String): String
    }
}
