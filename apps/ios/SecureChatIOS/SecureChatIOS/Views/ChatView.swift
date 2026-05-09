import SwiftUI

struct ChatView: View {
    @EnvironmentObject private var store: SecureChatStore
    let contact: AppContact
    @State private var draft = ""

    var messages: [AppChatMessage] {
        store.appSnapshot?.messages.filter { $0.contactId == contact.id } ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            SafetyBanner(contact: contact)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(messages) { message in
                            MessageBubble(
                                direction: message.direction,
                                messageBody: message.body,
                                status: message.status,
                                sentAtUnix: message.sentAtUnix,
                                receivedAtUnix: message.receivedAtUnix,
                                showsStatus: store.showMessageStatus,
                                showsTimestamp: store.showMessageTimestamps
                            )
                                .id(message.id)
                        }
                    }
                    .padding(14)
                }
                .onChange(of: messages.count) { _ in
                    if let last = messages.last {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            Divider()
            ComposerView(draft: $draft) {
                let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendMessage(text) }
            }
        }
        .navigationTitle(contact.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Image(systemName: "tray.and.arrow.down")
                }
            }
        }
        .onAppear {
            store.selectedContactID = contact.id
        }
    }
}

struct TemporaryChatView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    let connection: TemporaryConnection
    @State private var draft = ""

    private var currentConnection: TemporaryConnection {
        store.appSnapshot?.temporaryConnections.first { $0.id == connection.id } ?? connection
    }

    private var messages: [TemporaryMessage] {
        store.appSnapshot?.temporaryMessages.filter { $0.connectionId == connection.id } ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            TemporarySafetyBanner(connection: currentConnection)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(messages) { message in
                            MessageBubble(
                                direction: message.direction,
                                messageBody: message.body,
                                status: message.status,
                                sentAtUnix: message.sentAtUnix,
                                receivedAtUnix: message.receivedAtUnix,
                                showsStatus: store.showMessageStatus,
                                showsTimestamp: store.showMessageTimestamps
                            )
                            .id(message.id)
                        }
                    }
                    .padding(14)
                }
                .onChange(of: messages.count) { _ in
                    if let last = messages.last {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            Divider()
            ComposerView(draft: $draft) {
                let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendTemporaryMessage(text) }
            }
        }
        .navigationTitle(currentConnection.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Image(systemName: "tray.and.arrow.down")
                }

                Button(role: .destructive) {
                    Task {
                        await store.endTemporaryConnection()
                        dismiss()
                    }
                } label: {
                    Image(systemName: "trash")
                }
            }
        }
        .onAppear {
            store.selectedContactID = nil
            store.selectedTemporaryConnectionID = connection.id
        }
    }
}

struct GroupChatView: View {
    @EnvironmentObject private var store: SecureChatStore
    let group: AppGroup
    @State private var draft = ""

    private var currentGroup: AppGroup {
        store.appSnapshot?.groups.first { $0.id == group.id } ?? group
    }

    private var messages: [AppGroupMessage] {
        store.appSnapshot?.groupMessages.filter { $0.groupId == group.id } ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            GroupBanner(group: currentGroup)
            Divider()
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(messages) { message in
                            VStack(alignment: message.direction == .outgoing ? .trailing : .leading, spacing: 4) {
                                if message.direction == .incoming {
                                    Text(message.senderDisplayName)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                MessageBubble(
                                    direction: message.direction,
                                    messageBody: message.body,
                                    status: message.status,
                                    sentAtUnix: message.sentAtUnix,
                                    receivedAtUnix: message.receivedAtUnix,
                                    showsStatus: store.showMessageStatus,
                                    showsTimestamp: store.showMessageTimestamps
                                )
                            }
                            .id(message.id)
                        }
                    }
                    .padding(14)
                }
                .onChange(of: messages.count) { _ in
                    if let last = messages.last {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            Divider()
            ComposerView(draft: $draft) {
                let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendGroupMessage(text) }
            }
        }
        .navigationTitle(currentGroup.displayName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button {
                    Task { await store.addSelectedContactToSelectedGroup() }
                } label: {
                    Image(systemName: "person.badge.plus")
                }
                .disabled(store.selectedContactID == nil)

                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Image(systemName: "tray.and.arrow.down")
                }
            }
        }
        .onAppear {
            store.selectedGroupID = group.id
            store.selectedTemporaryConnectionID = nil
        }
    }
}

private struct SafetyBanner: View {
    let contact: AppContact

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Image(systemName: contact.verified ? "checkmark.shield.fill" : "shield.lefthalf.filled")
                    .foregroundStyle(contact.verified ? .green : .orange)
                Text(contact.verified ? "Verified safety code" : "Compare safety code")
                    .font(.subheadline.weight(.semibold))
                Spacer()
            }
            Text(contact.safetyNumber)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(2)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.bar)
    }
}

private struct GroupBanner: View {
    let group: AppGroup

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "person.3.fill")
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text("MLS group")
                    .font(.subheadline.weight(.semibold))
                Text("\(group.memberCount) members")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.bar)
    }
}

private struct TemporarySafetyBanner: View {
    let connection: TemporaryConnection

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Image(systemName: "timer")
                    .foregroundStyle(.orange)
                Text("Temporary encrypted session")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                Text(expiryText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Text(connection.safetyNumber)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(2)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(.bar)
    }

    private var expiryText: String {
        let remaining = Int64(connection.expiresUnix) - Int64(Date().timeIntervalSince1970)
        if remaining <= 0 { return "Expired" }
        let minutes = max(1, remaining / 60)
        if minutes < 60 { return "\(minutes)m left" }
        return "\(minutes / 60)h left"
    }
}

private struct MessageBubble: View {
    let direction: AppMessageDirection
    let messageBody: String
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
    let showsStatus: Bool
    let showsTimestamp: Bool

    private var isOutgoing: Bool {
        direction == .outgoing
    }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 44) }
            VStack(alignment: .leading, spacing: 5) {
                Text(messageBody)
                    .textSelection(.enabled)
                if showsStatus || showsTimestamp {
                    HStack(spacing: 6) {
                        if showsStatus {
                            Text(status.rawValue.capitalized)
                        }
                        if showsTimestamp {
                            Text(messageTime)
                        }
                    }
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                isOutgoing ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.12),
                in: RoundedRectangle(cornerRadius: 8)
            )
            if !isOutgoing { Spacer(minLength: 44) }
        }
    }

    private var messageTime: String {
        let unix = receivedAtUnix ?? sentAtUnix
        return Self.timeFormatter.string(from: Date(timeIntervalSince1970: TimeInterval(unix)))
    }

    private static let timeFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        return formatter
    }()
}

private struct ComposerView: View {
    @Binding var draft: String
    let send: () -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Message", text: $draft, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...5)
            Button(action: send) {
                Image(systemName: "paperplane.fill")
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.borderedProminent)
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
        .background(.bar)
    }
}
