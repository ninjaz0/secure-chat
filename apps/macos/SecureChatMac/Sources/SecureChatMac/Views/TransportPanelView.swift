import SwiftUI

struct TransportPanelView: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                TransportProfileView(
                    icon: "bolt.horizontal.circle",
                    title: "QUIC / UDP",
                    status: "Primary",
                    detail: "P2P candidate probing, connection migration, 1200-byte padded frames, jittered send scheduling."
                )
                TransportProfileView(
                    icon: "globe.badge.chevron.backward",
                    title: "WebSocket / TLS",
                    status: "Fallback",
                    detail: "Same E2EE wire frame over TLS-friendly infrastructure when UDP is unavailable."
                )
                TransportProfileView(
                    icon: "tray.and.arrow.down",
                    title: "Relay HTTPS",
                    status: "Offline queue",
                    detail: "Stores only ciphertext envelopes and short-lived delivery metadata."
                )
            }
            .padding(20)
            .frame(maxWidth: 900, alignment: .leading)
        }
    }
}

private struct TransportProfileView: View {
    let icon: String
    let title: String
    let status: String
    let detail: String

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 28)
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Text(title)
                        .font(.headline)
                    Spacer()
                    Text(status)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text(detail)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }
}

