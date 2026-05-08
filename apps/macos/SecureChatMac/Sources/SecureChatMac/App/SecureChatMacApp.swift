import AppKit
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }
}

@main
struct SecureChatMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var store = SecureChatStore()

    var body: some Scene {
        WindowGroup("SecureChat", id: "main") {
            ContentView()
                .environmentObject(store)
                .frame(minWidth: 1080, minHeight: 700)
                .task {
                    NotificationService.requestAuthorization()
                    await store.loadAppSnapshot()
                }
        }
        .commands {
            CommandMenu("SecureChat") {
                Button("Run Protocol Self Test") {
                    Task { await store.runSelfTest() }
                }
                .keyboardShortcut("r", modifiers: [.command, .shift])

                Button("Copy Invite Link") {
                    store.copyOwnInvite()
                }
                .keyboardShortcut("i", modifiers: [.command, .shift])
                .disabled(!store.isReady)
            }
        }

        Settings {
            SettingsView()
                .environmentObject(store)
        }
    }
}
