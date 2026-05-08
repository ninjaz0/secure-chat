import Foundation
import UserNotifications

enum NotificationService {
    static func notifyNewMessages(count: Int) {
        guard count > 0 else { return }
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral:
                addNotification(count: count)
            case .notDetermined:
                center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
                    if granted {
                        addNotification(count: count)
                    }
                }
            default:
                break
            }
        }
    }

    private static func addNotification(count: Int) {
        let content = UNMutableNotificationContent()
        content.title = "SecureChat"
        content.body = count == 1 ? "New encrypted message" : "\(count) new encrypted messages"
        content.sound = .default
        let request = UNNotificationRequest(
            identifier: "securechat.incoming.\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
