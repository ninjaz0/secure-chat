import SwiftUI

struct DiagnosticsPanelView: View {
    @EnvironmentObject private var store: SecureChatStore

    var checks: [ProtocolCheck] {
        store.selfTest?.checks ?? store.demo?.protocolChecks ?? []
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Label("Protocol Self-Test", systemImage: "checklist")
                        .font(.headline)
                    Spacer()
                    Button {
                        Task { await store.runSelfTest() }
                    } label: {
                        Label("Run", systemImage: "play.fill")
                    }
                    .disabled(store.isLoading)
                }
                .padding(16)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))

                HStack {
                    Label("Relay Delivery Smoke", systemImage: "point.3.connected.trianglepath.dotted")
                        .font(.headline)
                    Spacer()
                    Button {
                        Task { await store.runRelaySmoke() }
                    } label: {
                        Label("Run Relay", systemImage: "paperplane.fill")
                    }
                    .disabled(store.isLoading)
                }
                .padding(16)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))

                if let relaySmoke = store.relaySmoke {
                    RelaySmokeSummaryView(result: relaySmoke)
                }

                ForEach(checks) { check in
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: check.passed ? "checkmark.circle.fill" : "xmark.octagon.fill")
                            .foregroundStyle(check.passed ? .green : .red)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(check.name)
                                .font(.subheadline.weight(.semibold))
                            Text(check.detail)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .padding(14)
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
                }
            }
            .padding(20)
            .frame(maxWidth: 900, alignment: .leading)
        }
    }
}

private struct RelaySmokeSummaryView: View {
    let result: RelaySmokeResult

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            InfoRow(label: "Relay", value: result.relay)
            InfoRow(label: "Service", value: result.relayHealth.service)
            InfoRow(label: "Plaintext", value: result.relayHealth.storesPlaintext ? "stored" : "not stored")
            InfoRow(label: "Invite", value: result.bobInviteUriPrefix)
            Divider()
            InfoRow(label: "Bob got", value: result.bob.received.joined(separator: " | "))
            InfoRow(label: "Alice got", value: result.alice.received.joined(separator: " | "))
        }
        .padding(16)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
    }
}
