import SwiftUI

struct SafetyPanelView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 12) {
                    Label("Out-of-Band Verification", systemImage: "qrcode.viewfinder")
                        .font(.headline)
                    Text(store.demo?.safetyNumber ?? "Loading")
                        .font(.system(.title3, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(.vertical, 4)
                    HStack {
                        Button {
                            Clipboard.copy(store.demo?.safetyNumber ?? "")
                        } label: {
                            Label("Copy Safety Number", systemImage: "doc.on.doc")
                        }
                        Button {
                            Clipboard.copy(store.demo?.safetyQrPayload ?? "")
                        } label: {
                            Label("Copy QR Payload", systemImage: "qrcode")
                        }
                    }
                }
                .padding(16)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))

                VStack(alignment: .leading, spacing: 10) {
                    Label("Verification Policy", systemImage: "exclamationmark.shield")
                        .font(.headline)
                    VerificationRow(title: "Unverified contacts", detail: "Allowed to chat, visibly marked as untrusted.")
                    VerificationRow(title: "Identity key changes", detail: "Block silent trust upgrade and require a new safety-number comparison.")
                    VerificationRow(title: "New devices", detail: "Change the device-list digest and trigger a verification warning.")
                }
                .padding(16)
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
            }
            .padding(20)
            .frame(maxWidth: 900, alignment: .leading)
        }
    }
}

private struct VerificationRow: View {
    let title: String
    let detail: String

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "checkmark.circle")
                .foregroundStyle(.green)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                Text(detail)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

