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

struct AppSnapshot: Decodable {
    let ready: Bool
    let profile: AppProfile?
    let contacts: [AppContact]
    let messages: [AppChatMessage]
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
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
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

struct ReceiveReport: Decodable {
    let receivedCount: Int
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
