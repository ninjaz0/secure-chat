import Foundation
import UserNotifications

enum NotificationService {
    static func notifyNewMessages(count: Int, soundEnabled: Bool = true) {
        guard count > 0 else { return }
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral:
                addNotification(count: count, soundEnabled: soundEnabled)
            case .notDetermined:
                center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
                    if granted {
                        addNotification(count: count, soundEnabled: soundEnabled)
                    }
                }
            default:
                break
            }
        }
    }

    private static func addNotification(count: Int, soundEnabled: Bool) {
        let content = UNMutableNotificationContent()
        content.title = "SecureChat"
        content.body = count == 1 ? "New encrypted message" : "\(count) new encrypted messages"
        content.sound = soundEnabled ? .default : nil
        let request = UNNotificationRequest(
            identifier: "securechat.incoming.\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }
}
