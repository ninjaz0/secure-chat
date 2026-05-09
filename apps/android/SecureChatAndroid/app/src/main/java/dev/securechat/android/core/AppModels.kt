package dev.securechat.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class ProtocolCheck(
    val name: String,
    val passed: Boolean,
    val detail: String,
)

@Serializable
data class SelfTestResult(
    val ok: Boolean,
    val checks: List<ProtocolCheck> = emptyList(),
)

@Serializable
data class RelayHealth(
    val ok: Boolean = false,
    val service: String = "",
    @SerialName("stores_plaintext") val storesPlaintext: Boolean = false,
)

@Serializable
data class RelaySmokePeer(
    @SerialName("account_id") val accountId: String = "",
    @SerialName("device_id") val deviceId: String = "",
    val received: List<String> = emptyList(),
)

@Serializable
data class RelaySmokeResult(
    val ok: Boolean = false,
    val relay: String = "",
    @SerialName("relay_health") val relayHealth: RelayHealth = RelayHealth(),
    val alice: RelaySmokePeer = RelaySmokePeer(),
    val bob: RelaySmokePeer = RelaySmokePeer(),
    @SerialName("bob_invite_uri_prefix") val bobInviteUriPrefix: String = "",
)

@Serializable
data class P2pCandidate(
    val kind: String = "",
    val addr: String = "",
    @SerialName("updated_unix") val updatedUnix: Long = 0,
    @SerialName("expires_unix") val expiresUnix: Long = 0,
)

@Serializable
data class P2pProbeResult(
    val ok: Boolean = false,
    val relay: String = "",
    val rendezvous: String = "",
    @SerialName("local_addr") val localAddr: String = "",
    @SerialName("public_candidate") val publicCandidate: P2pCandidate = P2pCandidate(),
    @SerialName("registered_candidates") val registeredCandidates: List<P2pCandidate> = emptyList(),
)

@Serializable
data class AppSnapshot(
    val ready: Boolean,
    val profile: AppProfile? = null,
    val contacts: List<AppContact> = emptyList(),
    val messages: List<AppChatMessage> = emptyList(),
    val groups: List<AppGroup> = emptyList(),
    @SerialName("group_messages") val groupMessages: List<AppGroupMessage> = emptyList(),
    @SerialName("temporary_connections") val temporaryConnections: List<TemporaryConnection> = emptyList(),
    @SerialName("temporary_messages") val temporaryMessages: List<TemporaryMessage> = emptyList(),
    val stickers: List<StickerItem> = emptyList(),
    @SerialName("attachment_transfers") val attachmentTransfers: List<AttachmentTransfer> = emptyList(),
)

@Serializable
data class AppProfile(
    @SerialName("display_name") val displayName: String,
    @SerialName("account_id") val accountId: String,
    @SerialName("device_id") val deviceId: String,
    @SerialName("relay_url") val relayUrl: String,
    @SerialName("invite_uri") val inviteUri: String,
)

@Serializable
data class AppContact(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("account_id") val accountId: String,
    @SerialName("device_id") val deviceId: String,
    @SerialName("safety_number") val safetyNumber: String,
    val verified: Boolean,
    @SerialName("last_message") val lastMessage: String? = null,
    @SerialName("updated_at_unix") val updatedAtUnix: Long,
)

@Serializable
data class AppChatMessage(
    val id: String,
    @SerialName("contact_id") val contactId: String,
    val direction: AppMessageDirection,
    val body: String,
    val content: MessageContent = MessageContent(text = body),
    val status: AppMessageStatus,
    @SerialName("sent_at_unix") val sentAtUnix: Long,
    @SerialName("received_at_unix") val receivedAtUnix: Long? = null,
)

@Serializable
data class AppGroup(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("member_count") val memberCount: Int,
    @SerialName("last_message") val lastMessage: String? = null,
    @SerialName("updated_at_unix") val updatedAtUnix: Long,
)

@Serializable
data class AppGroupMessage(
    val id: String,
    @SerialName("group_id") val groupId: String,
    @SerialName("sender_display_name") val senderDisplayName: String,
    val direction: AppMessageDirection,
    val body: String,
    val content: MessageContent = MessageContent(text = body),
    val status: AppMessageStatus,
    @SerialName("sent_at_unix") val sentAtUnix: Long,
    @SerialName("received_at_unix") val receivedAtUnix: Long? = null,
)

@Serializable
data class TemporaryConnection(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("account_id") val accountId: String,
    @SerialName("device_id") val deviceId: String,
    @SerialName("safety_number") val safetyNumber: String,
    @SerialName("last_message") val lastMessage: String? = null,
    @SerialName("updated_at_unix") val updatedAtUnix: Long,
    @SerialName("expires_unix") val expiresUnix: Long,
)

@Serializable
data class TemporaryMessage(
    val id: String,
    @SerialName("connection_id") val connectionId: String,
    val direction: AppMessageDirection,
    val body: String,
    val content: MessageContent = MessageContent(text = body),
    val status: AppMessageStatus,
    @SerialName("sent_at_unix") val sentAtUnix: Long,
    @SerialName("received_at_unix") val receivedAtUnix: Long? = null,
)

@Serializable
data class MessageContent(
    val kind: String = "text",
    val text: String? = null,
    @SerialName("burn_id") val burnId: String? = null,
    val destroyed: Boolean = false,
    val attachment: AttachmentContent? = null,
)

@Serializable
data class AttachmentContent(
    val id: String,
    val kind: String,
    @SerialName("file_name") val fileName: String,
    @SerialName("mime_type") val mimeType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val sha256: String,
    @SerialName("local_path") val localPath: String? = null,
    @SerialName("transfer_status") val transferStatus: String = "",
)

@Serializable
data class StickerItem(
    val id: String,
    @SerialName("display_name") val displayName: String,
    @SerialName("file_name") val fileName: String,
    @SerialName("mime_type") val mimeType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val sha256: String,
    @SerialName("local_path") val localPath: String,
    @SerialName("created_at_unix") val createdAtUnix: Long,
)

@Serializable
data class AttachmentTransfer(
    val id: String,
    @SerialName("thread_kind") val threadKind: String,
    @SerialName("thread_id") val threadId: String,
    val kind: String,
    @SerialName("file_name") val fileName: String,
    @SerialName("mime_type") val mimeType: String,
    @SerialName("size_bytes") val sizeBytes: Long,
    val sha256: String,
    @SerialName("received_chunks") val receivedChunks: Long,
    @SerialName("total_chunks") val totalChunks: Long,
    val status: String,
)

@Serializable
enum class AppMessageDirection {
    @SerialName("outgoing")
    Outgoing,

    @SerialName("incoming")
    Incoming,
}

@Serializable
enum class AppMessageStatus {
    @SerialName("sent")
    Sent,

    @SerialName("delivered")
    Delivered,

    @SerialName("read")
    Read,

    @SerialName("received")
    Received,

    @SerialName("failed")
    Failed,
}

@Serializable
data class InviteResponse(
    @SerialName("invite_uri") val inviteUri: String,
)

@Serializable
data class TemporaryInviteResponse(
    @SerialName("invite_uri") val inviteUri: String,
    @SerialName("expires_unix") val expiresUnix: Long,
)

@Serializable
data class InvitePreview(
    @SerialName("normalized_invite_uri") val normalizedInviteUri: String,
    @SerialName("suggested_display_name") val suggestedDisplayName: String,
    @SerialName("account_id") val accountId: String,
    @SerialName("device_id") val deviceId: String,
    @SerialName("relay_hint") val relayHint: String? = null,
    @SerialName("expires_unix") val expiresUnix: Long? = null,
    @SerialName("safety_number") val safetyNumber: String,
    @SerialName("already_added") val alreadyAdded: Boolean,
    @SerialName("existing_display_name") val existingDisplayName: String? = null,
    val verified: Boolean,
    val temporary: Boolean,
)

@Serializable
data class ReceiveReport(
    @SerialName("received_count") val receivedCount: Int,
    val snapshot: AppSnapshot,
)

@Serializable
data class TemporaryStartResponse(
    @SerialName("connection_id") val connectionId: String,
    val snapshot: AppSnapshot,
)

@Serializable
data class SendAttachmentResponse(
    @SerialName("attachment_id") val attachmentId: String,
    val snapshot: AppSnapshot,
)

@Serializable
data class ImportStickerResponse(
    val sticker: StickerItem,
    val snapshot: AppSnapshot,
)

@Serializable
data class FfiErrorPayload(
    val ok: Boolean? = null,
    val error: String? = null,
)
