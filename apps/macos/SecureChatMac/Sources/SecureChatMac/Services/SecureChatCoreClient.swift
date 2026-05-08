import Foundation
import SecureChatFFI

enum SecureChatCoreClient {
    static var appDataDirectory: String {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        let url = base.appendingPathComponent("SecureChat", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url.path
    }

    static func loadDemoState() throws -> DemoState {
        try decodeCString(secure_chat_demo_state_json(), as: DemoState.self)
    }

    static func runSelfTest() throws -> SelfTestResult {
        try decodeCString(secure_chat_protocol_self_test_json(), as: SelfTestResult.self)
    }

    static func runRelaySmoke() throws -> RelaySmokeResult {
        try decodeCString(secure_chat_relay_smoke_json(), as: RelaySmokeResult.self)
    }

    static func appSnapshot() throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try decodeCString(secure_chat_app_snapshot_json(dataDirPtr), as: AppSnapshot.self)
        }
    }

    static func bootstrap(displayName: String, relayURL: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try displayName.withCString { displayNamePtr in
                try relayURL.withCString { relayPtr in
                    try decodeCString(
                        secure_chat_app_bootstrap_json(dataDirPtr, displayNamePtr, relayPtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func updateRelay(_ relayURL: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try relayURL.withCString { relayPtr in
                try decodeCString(
                    secure_chat_app_update_relay_json(dataDirPtr, relayPtr),
                    as: AppSnapshot.self
                )
            }
        }
    }

    static func ownInvite() throws -> InviteResponse {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try decodeCString(secure_chat_app_invite_json(dataDirPtr), as: InviteResponse.self)
        }
    }

    static func addContact(displayName: String, inviteURI: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try displayName.withCString { displayNamePtr in
                try inviteURI.withCString { invitePtr in
                    try decodeCString(
                        secure_chat_app_add_contact_json(dataDirPtr, displayNamePtr, invitePtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func sendMessage(contactID: String, body: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try contactID.withCString { contactPtr in
                try body.withCString { bodyPtr in
                    try decodeCString(
                        secure_chat_app_send_message_json(dataDirPtr, contactPtr, bodyPtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func receiveMessages() throws -> ReceiveReport {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try decodeCString(secure_chat_app_receive_json(dataDirPtr), as: ReceiveReport.self)
        }
    }

    private static func decodeCString<T: Decodable>(_ pointer: UnsafeMutablePointer<CChar>?, as type: T.Type) throws -> T {
        guard let pointer else {
            throw CoreClientError.emptyResponse
        }
        defer {
            secure_chat_free_string(pointer)
        }
        let text = String(cString: pointer)
        let data = Data(text.utf8)
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        if let error = try? decoder.decode(FFIErrorPayload.self, from: data),
           error.ok == false {
            throw CoreClientError.runtime(error.error ?? "Rust core returned an error.")
        }
        return try decoder.decode(T.self, from: data)
    }
}

enum CoreClientError: LocalizedError {
    case emptyResponse
    case runtime(String)

    var errorDescription: String? {
        switch self {
        case .emptyResponse:
            return "Rust core returned an empty response."
        case .runtime(let message):
            return message
        }
    }
}
