namespace SecureChatWindows.Models;

public sealed class FfiErrorPayload
{
    [JsonPropertyName("ok")]
    public bool? Ok { get; set; }

    [JsonPropertyName("error")]
    public string? Error { get; set; }
}

public sealed class ProtocolCheck
{
    [JsonPropertyName("name")]
    public string Name { get; set; } = "";

    [JsonPropertyName("passed")]
    public bool Passed { get; set; }

    [JsonPropertyName("detail")]
    public string Detail { get; set; } = "";
}

public sealed class SelfTestResult
{
    [JsonPropertyName("ok")]
    public bool Ok { get; set; }

    [JsonPropertyName("checks")]
    public List<ProtocolCheck> Checks { get; set; } = new();
}

public sealed class AppSnapshot
{
    [JsonPropertyName("ready")]
    public bool Ready { get; set; }

    [JsonPropertyName("profile")]
    public AppProfile? Profile { get; set; }

    [JsonPropertyName("contacts")]
    public List<AppContact> Contacts { get; set; } = new();

    [JsonPropertyName("messages")]
    public List<AppChatMessage> Messages { get; set; } = new();

    [JsonPropertyName("temporary_connections")]
    public List<TemporaryConnection> TemporaryConnections { get; set; } = new();

    [JsonPropertyName("temporary_messages")]
    public List<TemporaryMessage> TemporaryMessages { get; set; } = new();

    [JsonPropertyName("stickers")]
    public List<StickerItem> Stickers { get; set; } = new();

    [JsonPropertyName("attachment_transfers")]
    public List<AttachmentTransfer> AttachmentTransfers { get; set; } = new();
}

public sealed class AppProfile
{
    [JsonPropertyName("display_name")]
    public string DisplayName { get; set; } = "";

    [JsonPropertyName("account_id")]
    public string AccountId { get; set; } = "";

    [JsonPropertyName("device_id")]
    public string DeviceId { get; set; } = "";

    [JsonPropertyName("relay_url")]
    public string RelayUrl { get; set; } = "";

    [JsonPropertyName("invite_uri")]
    public string InviteUri { get; set; } = "";
}

public sealed class AppContact
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("display_name")]
    public string DisplayName { get; set; } = "";

    [JsonPropertyName("account_id")]
    public string AccountId { get; set; } = "";

    [JsonPropertyName("device_id")]
    public string DeviceId { get; set; } = "";

    [JsonPropertyName("safety_number")]
    public string SafetyNumber { get; set; } = "";

    [JsonPropertyName("verified")]
    public bool Verified { get; set; }

    [JsonPropertyName("last_message")]
    public string? LastMessage { get; set; }

    [JsonPropertyName("updated_at_unix")]
    public ulong UpdatedAtUnix { get; set; }
}

public sealed class AppChatMessage
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("contact_id")]
    public string ContactId { get; set; } = "";

    [JsonPropertyName("direction")]
    public AppMessageDirection Direction { get; set; }

    [JsonPropertyName("body")]
    public string Body { get; set; } = "";

    [JsonPropertyName("content")]
    public MessageContent Content { get; set; } = new();

    [JsonPropertyName("status")]
    public AppMessageStatus Status { get; set; }

    [JsonPropertyName("sent_at_unix")]
    public ulong SentAtUnix { get; set; }

    [JsonPropertyName("received_at_unix")]
    public ulong? ReceivedAtUnix { get; set; }
}

public sealed class TemporaryConnection
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("display_name")]
    public string DisplayName { get; set; } = "";

    [JsonPropertyName("account_id")]
    public string AccountId { get; set; } = "";

    [JsonPropertyName("device_id")]
    public string DeviceId { get; set; } = "";

    [JsonPropertyName("safety_number")]
    public string SafetyNumber { get; set; } = "";

    [JsonPropertyName("last_message")]
    public string? LastMessage { get; set; }

    [JsonPropertyName("updated_at_unix")]
    public ulong UpdatedAtUnix { get; set; }

    [JsonPropertyName("expires_unix")]
    public ulong ExpiresUnix { get; set; }
}

public sealed class TemporaryMessage
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("connection_id")]
    public string ConnectionId { get; set; } = "";

    [JsonPropertyName("direction")]
    public AppMessageDirection Direction { get; set; }

    [JsonPropertyName("body")]
    public string Body { get; set; } = "";

    [JsonPropertyName("content")]
    public MessageContent Content { get; set; } = new();

    [JsonPropertyName("status")]
    public AppMessageStatus Status { get; set; }

    [JsonPropertyName("sent_at_unix")]
    public ulong SentAtUnix { get; set; }

    [JsonPropertyName("received_at_unix")]
    public ulong? ReceivedAtUnix { get; set; }
}

public sealed class MessageContent
{
    [JsonPropertyName("kind")]
    public string Kind { get; set; } = "text";

    [JsonPropertyName("text")]
    public string? Text { get; set; }

    [JsonPropertyName("burn_id")]
    public string? BurnId { get; set; }

    [JsonPropertyName("destroyed")]
    public bool Destroyed { get; set; }

    [JsonPropertyName("attachment")]
    public AttachmentContent? Attachment { get; set; }
}

public sealed class AttachmentContent
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("kind")]
    public string Kind { get; set; } = "";

    [JsonPropertyName("file_name")]
    public string FileName { get; set; } = "";

    [JsonPropertyName("mime_type")]
    public string MimeType { get; set; } = "";

    [JsonPropertyName("size_bytes")]
    public ulong SizeBytes { get; set; }

    [JsonPropertyName("sha256")]
    public string Sha256 { get; set; } = "";

    [JsonPropertyName("local_path")]
    public string? LocalPath { get; set; }

    [JsonPropertyName("transfer_status")]
    public string TransferStatus { get; set; } = "";
}

public sealed class StickerItem
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("display_name")]
    public string DisplayName { get; set; } = "";

    [JsonPropertyName("file_name")]
    public string FileName { get; set; } = "";

    [JsonPropertyName("mime_type")]
    public string MimeType { get; set; } = "";

    [JsonPropertyName("size_bytes")]
    public ulong SizeBytes { get; set; }

    [JsonPropertyName("sha256")]
    public string Sha256 { get; set; } = "";

    [JsonPropertyName("local_path")]
    public string LocalPath { get; set; } = "";

    public override string ToString() => DisplayName;
}

public sealed class AttachmentTransfer
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = "";

    [JsonPropertyName("thread_kind")]
    public string ThreadKind { get; set; } = "";

    [JsonPropertyName("thread_id")]
    public string ThreadId { get; set; } = "";

    [JsonPropertyName("kind")]
    public string Kind { get; set; } = "";

    [JsonPropertyName("file_name")]
    public string FileName { get; set; } = "";

    [JsonPropertyName("mime_type")]
    public string MimeType { get; set; } = "";

    [JsonPropertyName("size_bytes")]
    public ulong SizeBytes { get; set; }

    [JsonPropertyName("sha256")]
    public string Sha256 { get; set; } = "";

    [JsonPropertyName("status")]
    public string Status { get; set; } = "";

    [JsonPropertyName("total_chunks")]
    public ulong TotalChunks { get; set; }

    [JsonPropertyName("received_chunks")]
    public ulong ReceivedChunks { get; set; }
}

public sealed class InviteResponse
{
    [JsonPropertyName("invite_uri")]
    public string InviteUri { get; set; } = "";

}

public sealed class TemporaryInviteResponse
{
    [JsonPropertyName("invite_uri")]
    public string InviteUri { get; set; } = "";

    [JsonPropertyName("expires_unix")]
    public ulong ExpiresUnix { get; set; }
}

public sealed class InvitePreview
{
    [JsonPropertyName("normalized_invite_uri")]
    public string NormalizedInviteUri { get; set; } = "";

    [JsonPropertyName("suggested_display_name")]
    public string SuggestedDisplayName { get; set; } = "";

    [JsonPropertyName("account_id")]
    public string AccountId { get; set; } = "";

    [JsonPropertyName("device_id")]
    public string DeviceId { get; set; } = "";

    [JsonPropertyName("safety_number")]
    public string SafetyNumber { get; set; } = "";

    [JsonPropertyName("already_added")]
    public bool AlreadyAdded { get; set; }

    [JsonPropertyName("existing_display_name")]
    public string? ExistingDisplayName { get; set; }

    [JsonPropertyName("verified")]
    public bool Verified { get; set; }

    [JsonPropertyName("temporary")]
    public bool Temporary { get; set; }

    [JsonPropertyName("expires_unix")]
    public ulong? ExpiresUnix { get; set; }
}

public sealed class TemporaryStartResponse
{
    [JsonPropertyName("connection_id")]
    public string ConnectionId { get; set; } = "";

    [JsonPropertyName("snapshot")]
    public AppSnapshot Snapshot { get; set; } = new();
}

public sealed class SendAttachmentResponse
{
    [JsonPropertyName("attachment_id")]
    public string AttachmentId { get; set; } = "";

    [JsonPropertyName("snapshot")]
    public AppSnapshot Snapshot { get; set; } = new();
}

public sealed class ImportStickerResponse
{
    [JsonPropertyName("sticker")]
    public StickerItem? Sticker { get; set; }

    [JsonPropertyName("snapshot")]
    public AppSnapshot Snapshot { get; set; } = new();
}

public sealed class ReceiveReport
{
    [JsonPropertyName("received_count")]
    public int ReceivedCount { get; set; }

    [JsonPropertyName("snapshot")]
    public AppSnapshot Snapshot { get; set; } = new();
}

public enum AppMessageDirection
{
    Outgoing,
    Incoming,
}

public enum AppMessageStatus
{
    Sent,
    Delivered,
    Read,
    Received,
    Failed,
}
