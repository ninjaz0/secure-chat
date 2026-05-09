import Foundation

struct DemoState: Decodable {
    let ok: Bool
    let error: String?
    let accountId: String
    let deviceId: String
    let inviteUri: String
    let safetyNumber: String
    let safetyQrPayload: String
    let relayHint: String
    let protocolChecks: [ProtocolCheck]
    let contacts: [ContactPreview]
    let messages: [MessagePreview]
}

struct ProtocolCheck: Decodable, Identifiable {
    var id: String { name }
    let name: String
    let passed: Bool
    let detail: String
}

struct ContactPreview: Decodable, Identifiable {
    var id: String { deviceId }
    let displayName: String
    let accountId: String
    let deviceId: String
    let verified: Bool
}

struct MessagePreview: Decodable, Identifiable {
    var id: String { sender + body }
    let sender: String
    let body: String
    let encryptedBytes: Int
}

struct SelfTestResult: Decodable {
    let ok: Bool
    let checks: [ProtocolCheck]
}

struct RelaySmokeResult: Decodable {
    let ok: Bool
    let relay: String
    let relayHealth: RelayHealth
    let alice: RelaySmokePeer
    let bob: RelaySmokePeer
    let bobInviteUriPrefix: String
}

struct RelayHealth: Decodable {
    let ok: Bool
    let service: String
    let storesPlaintext: Bool
}

struct RelaySmokePeer: Decodable {
    let accountId: String
    let deviceId: String
    let received: [String]
}

struct P2pProbeResult: Decodable {
    let ok: Bool
    let relay: String
    let rendezvous: String
    let localAddr: String
    let publicCandidate: P2pCandidate
    let registeredCandidates: [P2pCandidate]
}

struct P2pCandidate: Decodable, Identifiable, Hashable {
    var id: String { kind + addr }
    let kind: String
    let addr: String
    let updatedUnix: UInt64
    let expiresUnix: UInt64
}

struct AppSnapshot: Decodable {
    let ready: Bool
    let profile: AppProfile?
    let contacts: [AppContact]
    let messages: [AppChatMessage]
    let groups: [AppGroup]
    let groupMessages: [AppGroupMessage]
    let temporaryConnections: [TemporaryConnection]
    let temporaryMessages: [TemporaryMessage]
    let stickers: [StickerItem]
    let attachmentTransfers: [AttachmentTransfer]
}

struct AppProfile: Decodable {
    let displayName: String
    let accountId: String
    let deviceId: String
    let relayUrl: String
    let inviteUri: String
}

struct AppContact: Decodable, Identifiable, Hashable {
    let id: String
    let displayName: String
    let accountId: String
    let deviceId: String
    let safetyNumber: String
    let verified: Bool
    let lastMessage: String?
    let updatedAtUnix: UInt64
}

struct AppChatMessage: Decodable, Identifiable, Hashable {
    let id: String
    let contactId: String
    let direction: AppMessageDirection
    let body: String
    let content: MessageContent
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
}

struct AppGroup: Decodable, Identifiable, Hashable {
    let id: String
    let displayName: String
    let memberCount: Int
    let lastMessage: String?
    let updatedAtUnix: UInt64
}

struct AppGroupMessage: Decodable, Identifiable, Hashable {
    let id: String
    let groupId: String
    let senderDisplayName: String
    let direction: AppMessageDirection
    let body: String
    let content: MessageContent
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
}

struct TemporaryConnection: Decodable, Identifiable, Hashable {
    let id: String
    let displayName: String
    let accountId: String
    let deviceId: String
    let safetyNumber: String
    let lastMessage: String?
    let updatedAtUnix: UInt64
    let expiresUnix: UInt64
}

struct TemporaryMessage: Decodable, Identifiable, Hashable {
    let id: String
    let connectionId: String
    let direction: AppMessageDirection
    let body: String
    let content: MessageContent
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
}

struct MessageContent: Decodable, Hashable {
    let kind: String
    let text: String?
    let burnId: String?
    let destroyed: Bool
    let attachment: AttachmentContent?
}

struct AttachmentContent: Decodable, Identifiable, Hashable {
    let id: String
    let kind: String
    let fileName: String
    let mimeType: String
    let sizeBytes: UInt64
    let sha256: String
    let localPath: String?
    let transferStatus: String
}

struct StickerItem: Decodable, Identifiable, Hashable {
    let id: String
    let displayName: String
    let fileName: String
    let mimeType: String
    let sizeBytes: UInt64
    let sha256: String
    let localPath: String
    let createdAtUnix: UInt64
}

struct AttachmentTransfer: Decodable, Identifiable, Hashable {
    let id: String
    let threadKind: String
    let threadId: String
    let kind: String
    let fileName: String
    let mimeType: String
    let sizeBytes: UInt64
    let sha256: String
    let receivedChunks: UInt64
    let totalChunks: UInt64
    let status: String
}

struct SendAttachmentResponse: Decodable {
    let attachmentId: String
    let snapshot: AppSnapshot
}

struct ImportStickerResponse: Decodable {
    let sticker: StickerItem
    let snapshot: AppSnapshot
}

enum AppMessageDirection: String, Decodable {
    case outgoing
    case incoming
}

enum AppMessageStatus: String, Decodable {
    case sent
    case delivered
    case read
    case received
    case failed
}

struct InviteResponse: Decodable {
    let inviteUri: String
}

struct TemporaryInviteResponse: Decodable {
    let inviteUri: String
    let expiresUnix: UInt64
}

struct InvitePreview: Decodable, Equatable {
    let normalizedInviteUri: String
    let suggestedDisplayName: String
    let accountId: String
    let deviceId: String
    let relayHint: String?
    let expiresUnix: UInt64?
    let safetyNumber: String
    let alreadyAdded: Bool
    let existingDisplayName: String?
    let verified: Bool
    let temporary: Bool
}

struct ReceiveReport: Decodable {
    let receivedCount: Int
    let snapshot: AppSnapshot
}

struct TemporaryStartResponse: Decodable {
    let connectionId: String
    let snapshot: AppSnapshot
}

struct FFIErrorPayload: Decodable {
    let ok: Bool?
    let error: String?
}

enum SidebarSelection: String, CaseIterable, Identifiable {
    case chat
    case safety
    case transport
    case diagnostics

    var id: String { rawValue }

    var title: String {
        switch self {
        case .chat: return "1:1 Chat"
        case .safety: return "Safety"
        case .transport: return "Transport"
        case .diagnostics: return "Diagnostics"
        }
    }

    var detail: String {
        switch self {
        case .chat: return "E2EE session"
        case .safety: return "OOB verification"
        case .transport: return "P2P + relay"
        case .diagnostics: return "Protocol checks"
        }
    }

    var systemImage: String {
        switch self {
        case .chat: return "bubble.left.and.bubble.right"
        case .safety: return "checkmark.shield"
        case .transport: return "network"
        case .diagnostics: return "waveform.path.ecg"
        }
    }
}
