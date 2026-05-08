import Foundation
import UserNotifications

enum NotificationService {
    static func requestAuthorization() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    static func notifyNewMessages(count: Int) {
        guard count > 0 else { return }
        let content = UNMutableNotificationContent()
        content.title = "SecureChat"
        content.body = count == 1 ? "New encrypted message received." : "\(count) new encrypted messages received."
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "securechat.receive.\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}

