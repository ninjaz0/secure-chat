import Foundation
import UIKit
import UserNotifications

@MainActor
final class PushRegistrationService {
    static let shared = PushRegistrationService()

    private weak var store: SecureChatStore?

    func configure(store: SecureChatStore) {
        self.store = store
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
            guard granted else { return }
            DispatchQueue.main.async {
                UIApplication.shared.registerForRemoteNotifications()
            }
        }
    }

    func didRegister(deviceToken: Data) {
        let token = deviceToken.map { String(format: "%02.2hhx", $0) }.joined()
        Task { await store?.registerPushToken(token, platform: "ios") }
    }

    func didFail(error: Error) {
        NSLog("SecureChat APNs registration failed: \(error.localizedDescription)")
    }
}
