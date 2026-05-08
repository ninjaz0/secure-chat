import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var relayURL = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Relay") {
                    TextField("Relay URL", text: $relayURL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)

                    Button {
                        Task {
                            await store.updateRelay(relayURL)
                            dismiss()
                        }
                    } label: {
                        Label("Save Relay", systemImage: "network")
                    }
                }

                Section("Receive") {
                    Toggle("Auto receive", isOn: $store.autoReceiveEnabled)
                    Button {
                        Task { await store.receiveMessages() }
                    } label: {
                        Label("Receive Now", systemImage: "tray.and.arrow.down")
                    }
                }

                Section("Diagnostics") {
                    Button {
                        Task { await store.runSelfTest() }
                    } label: {
                        Label("Protocol Self-Test", systemImage: "checkmark.seal")
                    }

                    if let selfTest = store.selfTest {
                        Text(selfTest.ok ? "Protocol self-test passed" : "Protocol self-test failed")
                            .foregroundStyle(selfTest.ok ? .green : .red)
                    }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .onAppear {
                relayURL = store.appSnapshot?.profile?.relayUrl ?? "https://chat.example.com"
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
