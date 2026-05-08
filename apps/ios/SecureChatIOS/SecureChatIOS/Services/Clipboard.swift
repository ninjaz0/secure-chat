import UIKit

enum Clipboard {
    private static let clearAfter: TimeInterval = 120

    static func copy(_ text: String) {
        UIPasteboard.general.setItems(
            [["public.utf8-plain-text": text]],
            options: [.expirationDate: Date().addingTimeInterval(clearAfter)]
        )
    }

    static func readString() -> String? {
        UIPasteboard.general.string
    }
}
