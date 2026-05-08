import SwiftUI

@main
struct SecureChatIOSApp: App {
    @StateObject private var store = SecureChatStore()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(store)
                .task {
                    await store.loadAppSnapshot()
                    store.startAutoReceive()
                }
        }
    }
}
