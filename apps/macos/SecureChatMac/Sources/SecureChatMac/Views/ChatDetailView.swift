import SwiftUI

struct ChatDetailView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                AccountSummaryView()
                ContactListView()
                MessageTranscriptView()
            }
            .padding(20)
            .frame(maxWidth: 900, alignment: .leading)
        }
    }
}

private struct AccountSummaryView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label("Anonymous Device", systemImage: "person.crop.circle.badge.checkmark")
                    .font(.headline)
                Spacer()
                if store.isLoading {
                    ProgressView()
                        .controlSize(.small)
                }
            }
            InfoRow(label: "Account", value: store.demo?.accountId ?? "Loading")
            InfoRow(label: "Device", value: store.demo?.deviceId ?? "Loading")
            InfoRow(label: "Relay", value: store.demo?.relayHint ?? "Loading")
        }
        .padding(16)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
    }
}

private struct ContactListView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Contacts")
                .font(.headline)
            ForEach(store.demo?.contacts ?? []) { contact in
                HStack(spacing: 12) {
                    Image(systemName: contact.verified ? "checkmark.shield.fill" : "shield")
                        .foregroundStyle(contact.verified ? .green : .secondary)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(contact.displayName)
                            .font(.body)
                        Text(contact.deviceId)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Spacer()
                    Text(contact.verified ? "Verified" : "Unverified")
                        .font(.caption)
                        .foregroundStyle(contact.verified ? .green : .orange)
                }
                .padding(12)
                .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
            }
        }
    }
}

private struct MessageTranscriptView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Encrypted Transcript")
                .font(.headline)
            ForEach(store.demo?.messages ?? []) { message in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(message.sender)
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        Label("\(message.encryptedBytes) bytes", systemImage: "lock.fill")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Text(message.body)
                        .textSelection(.enabled)
                }
                .padding(14)
                .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
            }
        }
    }
}

struct InfoRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: 80, alignment: .leading)
            Text(value)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
                .lineLimit(2)
            Spacer(minLength: 0)
        }
    }
}

