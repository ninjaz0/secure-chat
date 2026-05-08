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
}

struct ReceiveReport: Decodable {
    let receivedCount: Int
    let snapshot: AppSnapshot
}

struct FFIErrorPayload: Decodable {
    let ok: Bool?
    let error: String?
}
