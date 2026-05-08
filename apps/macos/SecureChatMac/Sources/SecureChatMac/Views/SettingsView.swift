import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var relayURL = ""

    var body: some View {
        TabView {
            Form {
                Section("Relay") {
                    TextField("Relay URL", text: $relayURL)
                    HStack {
                        Button {
                            Task { await store.updateRelay(relayURL.trimmingCharacters(in: .whitespacesAndNewlines)) }
                        } label: {
                            Label("Save Relay", systemImage: "network")
                        }
                        .disabled(relayURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                        Button {
                            Task { await store.receiveMessages() }
                        } label: {
                            Label("Receive Now", systemImage: "tray.and.arrow.down")
                        }
                        .disabled(store.isLoading)
                    }
                }

                Section("Receive") {
                    Toggle("Auto receive", isOn: $store.autoReceiveEnabled)
                    Stepper(value: $store.receivePollIntervalSeconds, in: 5...300, step: 5) {
                        LabeledContent("Poll interval", value: "\(store.receivePollIntervalSeconds)s")
                    }
                    Toggle("New message notifications", isOn: $store.notifyOnNewMessages)
                    Toggle("Notification sound", isOn: $store.playNotificationSound)
                        .disabled(!store.notifyOnNewMessages)
                }
            }
            .tabItem {
                Label("General", systemImage: "gearshape")
            }

            Form {
                Section("Messages") {
                    Toggle("Show timestamps", isOn: $store.showMessageTimestamps)
                    Toggle("Show delivery status", isOn: $store.showMessageStatus)
                }

                Section("Transport") {
                    Toggle("Prefer QUIC / UDP", isOn: $store.preferQuic)
                    Toggle("Enable cover traffic", isOn: $store.coverTrafficEnabled)
                }
            }
            .tabItem {
                Label("Chat", systemImage: "bubble.left.and.bubble.right")
            }

            Form {
                Section("Diagnostics") {
                    Button {
                        Task { await store.loadAppSnapshot() }
                    } label: {
                        Label("Refresh App State", systemImage: "arrow.clockwise")
                    }

                    Button {
                        Task { await store.runSelfTest() }
                    } label: {
                        Label("Protocol Self-Test", systemImage: "checkmark.seal")
                    }

                    Button {
                        Task { await store.runRelaySmoke() }
                    } label: {
                        Label("Relay Smoke Test", systemImage: "waveform.path.ecg")
                    }

                    Button {
                        Task { await store.runP2PProbe() }
                    } label: {
                        Label("P2P NAT Probe", systemImage: "point.3.connected.trianglepath.dotted")
                    }
                    .disabled(store.appSnapshot?.ready != true)

                    if let selfTest = store.selfTest {
                        Text(selfTest.ok ? "Protocol self-test passed" : "Protocol self-test failed")
                            .foregroundStyle(selfTest.ok ? .green : .red)
                    }

                    if let relaySmoke = store.relaySmoke {
                        Text(relaySmoke.ok ? "Relay smoke test passed" : "Relay smoke test failed")
                            .foregroundStyle(relaySmoke.ok ? .green : .red)
                    }

                    if let p2pProbe = store.p2pProbe {
                        LabeledContent("Rendezvous", value: p2pProbe.rendezvous)
                        LabeledContent("Public UDP", value: p2pProbe.publicCandidate.addr)
                    }
                }

                Section("Maintenance") {
                    Button("Restore Settings Defaults", role: .destructive) {
                        store.resetPreferences()
                    }
                }
            }
            .tabItem {
                Label("Diagnostics", systemImage: "waveform.path.ecg")
            }
        }
        .padding(20)
        .frame(width: 540, height: 430)
        .onAppear {
            relayURL = store.appSnapshot?.profile?.relayUrl ?? "http://127.0.0.1:8787"
        }
    }
}
