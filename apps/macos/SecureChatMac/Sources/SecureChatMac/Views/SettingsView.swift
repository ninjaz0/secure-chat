import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: SecureChatStore
    @AppStorage("coverTrafficEnabled") private var coverTrafficEnabled = true
    @AppStorage("preferQuic") private var preferQuic = true

    var body: some View {
        Form {
            Toggle("Prefer QUIC / UDP", isOn: $preferQuic)
            Toggle("Enable cover traffic", isOn: $coverTrafficEnabled)
            Button("Refresh Core State") {
                Task { await store.loadDemo() }
            }
        }
        .padding(20)
        .frame(width: 420)
    }
}

