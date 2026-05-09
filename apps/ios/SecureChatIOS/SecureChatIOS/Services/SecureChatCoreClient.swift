import Foundation
import SecureChatFFI

enum SecureChatCoreClient {
    static var appDataDirectory: String {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        let url = base.appendingPathComponent("SecureChatIOS", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url.path
    }

    static func runInBackground<T>(_ work: @escaping () throws -> T) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    continuation.resume(returning: try work())
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
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

    static func runP2PProbe() throws -> P2pProbeResult {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try decodeCString(secure_chat_app_p2p_probe_json(dataDirPtr), as: P2pProbeResult.self)
        }
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

    static func temporaryInvite() throws -> TemporaryInviteResponse {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try decodeCString(secure_chat_app_temporary_invite_json(dataDirPtr), as: TemporaryInviteResponse.self)
        }
    }

    static func previewInvite(_ inviteText: String) throws -> InvitePreview {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try inviteText.withCString { invitePtr in
                try decodeCString(
                    secure_chat_app_preview_invite_json(dataDirPtr, invitePtr),
                    as: InvitePreview.self
                )
            }
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

    static func updateContactDisplayName(contactID: String, displayName: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try contactID.withCString { contactPtr in
                try displayName.withCString { displayNamePtr in
                    try decodeCString(
                        secure_chat_app_update_contact_display_name_json(dataDirPtr, contactPtr, displayNamePtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func deleteContact(contactID: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try contactID.withCString { contactPtr in
                try decodeCString(
                    secure_chat_app_delete_contact_json(dataDirPtr, contactPtr),
                    as: AppSnapshot.self
                )
            }
        }
    }

    static func startTemporaryConnection(inviteURI: String) throws -> TemporaryStartResponse {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try inviteURI.withCString { invitePtr in
                try decodeCString(
                    secure_chat_app_start_temporary_connection_json(dataDirPtr, invitePtr),
                    as: TemporaryStartResponse.self
                )
            }
        }
    }

    static func sendTemporaryMessage(connectionID: String, body: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try connectionID.withCString { connectionPtr in
                try body.withCString { bodyPtr in
                    try decodeCString(
                        secure_chat_app_send_temporary_message_json(dataDirPtr, connectionPtr, bodyPtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func endTemporaryConnection(connectionID: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try connectionID.withCString { connectionPtr in
                try decodeCString(
                    secure_chat_app_end_temporary_connection_json(dataDirPtr, connectionPtr),
                    as: AppSnapshot.self
                )
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

    static func sendAttachment(threadKind: String, threadID: String, filePath: String, kind: String) throws -> SendAttachmentResponse {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try threadKind.withCString { kindPtr in
                try threadID.withCString { threadPtr in
                    try filePath.withCString { filePtr in
                        try kind.withCString { attachmentKindPtr in
                            try decodeCString(
                                secure_chat_app_send_attachment_json(dataDirPtr, kindPtr, threadPtr, filePtr, attachmentKindPtr),
                                as: SendAttachmentResponse.self
                            )
                        }
                    }
                }
            }
        }
    }

    static func sendBurnMessage(threadKind: String, threadID: String, body: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try threadKind.withCString { kindPtr in
                try threadID.withCString { threadPtr in
                    try body.withCString { bodyPtr in
                        try decodeCString(
                            secure_chat_app_send_burn_message_json(dataDirPtr, kindPtr, threadPtr, bodyPtr),
                            as: AppSnapshot.self
                        )
                    }
                }
            }
        }
    }

    static func openBurnMessage(threadKind: String, threadID: String, messageID: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try threadKind.withCString { kindPtr in
                try threadID.withCString { threadPtr in
                    try messageID.withCString { messagePtr in
                        try decodeCString(
                            secure_chat_app_open_burn_message_json(dataDirPtr, kindPtr, threadPtr, messagePtr),
                            as: AppSnapshot.self
                        )
                    }
                }
            }
        }
    }

    static func registerPushToken(_ token: String, platform: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try token.withCString { tokenPtr in
                try platform.withCString { platformPtr in
                    try decodeCString(
                        secure_chat_app_register_push_token_json(dataDirPtr, tokenPtr, platformPtr),
                        as: AppSnapshot.self
                    )
                }
            }
        }
    }

    static func importSticker(filePath: String, displayName: String) throws -> ImportStickerResponse {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try filePath.withCString { filePtr in
                try displayName.withCString { namePtr in
                    try decodeCString(
                        secure_chat_app_import_sticker_json(dataDirPtr, filePtr, namePtr),
                        as: ImportStickerResponse.self
                    )
                }
            }
        }
    }

    static func deleteSticker(stickerID: String) throws -> AppSnapshot {
        let dataDir = appDataDirectory
        return try dataDir.withCString { dataDirPtr in
            try stickerID.withCString { stickerPtr in
                try decodeCString(
                    secure_chat_app_delete_sticker_json(dataDirPtr, stickerPtr),
                    as: AppSnapshot.self
                )
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
