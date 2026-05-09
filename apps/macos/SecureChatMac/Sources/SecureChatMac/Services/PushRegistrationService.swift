import AppKit
import Foundation

@MainActor
final class PushRegistrationService {
    static let shared = PushRegistrationService()

    private weak var store: SecureChatStore?

    func configure(store: SecureChatStore) {
        self.store = store
        NotificationService.requestAuthorization()
        NSApplication.shared.registerForRemoteNotifications()
    }

    func didRegister(deviceToken: Data) {
        let token = deviceToken.map { String(format: "%02.2hhx", $0) }.joined()
        Task { await store?.registerPushToken(token, platform: "macos") }
    }

    func didFail(error: Error) {
        NSLog("SecureChat APNs registration failed: \(error.localizedDescription)")
    }
}
