import AppKit

enum Clipboard {
    private static let clearAfter: TimeInterval = 120

    static func copy(_ text: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        DispatchQueue.main.asyncAfter(deadline: .now() + clearAfter) {
            guard NSPasteboard.general.string(forType: .string) == text else { return }
            NSPasteboard.general.clearContents()
        }
    }

    static func readString() -> String? {
        NSPasteboard.general.string(forType: .string)
    }
}
