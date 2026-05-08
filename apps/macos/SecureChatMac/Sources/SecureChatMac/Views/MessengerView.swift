import SwiftUI

struct MessengerView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var showingInvite = false
    @State private var showingAddContact = false

    var body: some View {
        NavigationSplitView {
            ContactSidebarView()
        } detail: {
            ChatConversationView()
        }
        .toolbar {
            ToolbarItemGroup {
                Button {
                    showingInvite = true
                } label: {
                    Label("Invite", systemImage: "qrcode")
                }
                .help("Show my invite")

                Button {
                    showingAddContact = true
                } label: {
                    Label("Add Contact", systemImage: "person.badge.plus")
                }
                .help("Add contact from invite")

                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Label("Receive", systemImage: "tray.and.arrow.down")
                }
                .help("Pull encrypted messages from relay")
                .disabled(store.isLoading)

                SettingsLink {
                    Label("Settings", systemImage: "gearshape")
                }
                .help("Settings")
            }
        }
        .sheet(isPresented: $showingInvite) {
            InviteSheet()
                .environmentObject(store)
        }
        .sheet(isPresented: $showingAddContact) {
            AddContactSheet()
                .environmentObject(store)
        }
        .task {
            store.startAutoReceive()
        }
    }
}

private struct ContactSidebarView: View {
    @EnvironmentObject private var store: SecureChatStore

    var body: some View {
        List(selection: $store.selectedContactID) {
            Section("Chats") {
                ForEach(store.appSnapshot?.contacts ?? []) { contact in
                    HStack(spacing: 10) {
                        Image(systemName: contact.verified ? "checkmark.shield.fill" : "person.crop.circle")
                            .foregroundStyle(contact.verified ? .green : .secondary)
                            .frame(width: 18)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(contact.displayName)
                                .lineLimit(1)
                            Text(contact.lastMessage ?? shortDevice(contact.deviceId))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .tag(contact.id)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle(store.appSnapshot?.profile?.displayName ?? "SecureChat")
    }
}

private struct ChatConversationView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var draft = ""

    var body: some View {
        if let contact = store.selectedContact {
            VStack(spacing: 0) {
                ChatHeaderView(contact: contact)
                Divider()
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(store.selectedMessages) { message in
                            MessageBubble(
                                message: message,
                                showsStatus: store.showMessageStatus,
                                showsTimestamp: store.showMessageTimestamps
                            )
                        }
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity)
                }
                Divider()
                ComposerView(draft: $draft) {
                    let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                    draft = ""
                    Task { await store.sendMessage(body) }
                }
            }
        } else {
            ContentUnavailableView(
                "No Contact Selected",
                systemImage: "person.2",
                description: Text("Copy your invite or add a friend's invite to start chatting.")
            )
        }
    }
}

private struct ChatHeaderView: View {
    let contact: AppContact

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: contact.verified ? "checkmark.shield.fill" : "shield")
                .foregroundStyle(contact.verified ? .green : .orange)
            VStack(alignment: .leading, spacing: 2) {
                Text(contact.displayName)
                    .font(.headline)
                Text("Safety \(contact.safetyNumber)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .textSelection(.enabled)
            }
            Spacer()
            Text(shortDevice(contact.deviceId))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }
}

private struct MessageBubble: View {
    let message: AppChatMessage
    let showsStatus: Bool
    let showsTimestamp: Bool

    var isOutgoing: Bool { message.direction == .outgoing }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 80) }
            VStack(alignment: .leading, spacing: 5) {
                Text(message.body)
                    .textSelection(.enabled)
                if showsStatus || showsTimestamp {
                    HStack(spacing: 6) {
                        if showsStatus {
                            Text(message.status.rawValue.capitalized)
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
            .background(isOutgoing ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
            if !isOutgoing { Spacer(minLength: 80) }
        }
    }

    private var messageTime: String {
        let unix = message.receivedAtUnix ?? message.sentAtUnix
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
        HStack(spacing: 10) {
            TextField("Message", text: $draft, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...4)
            Button(action: send) {
                Label("Send", systemImage: "paperplane.fill")
            }
            .keyboardShortcut(.return, modifiers: [.command])
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
    }
}

private struct InviteSheet: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("My Invite")
                .font(.headline)
            Text(store.appSnapshot?.profile?.inviteUri ?? "")
                .font(.system(.caption, design: .monospaced))
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
            HStack {
                Button("Copy") {
                    store.copyOwnInvite()
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                Spacer()
                Button("Close") { dismiss() }
            }
        }
        .padding(20)
        .frame(width: 560)
    }
}

private struct AddContactSheet: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var displayName = ""
    @State private var inviteText = ""
    @State private var invitePreview: InvitePreview?
    @State private var previewError: String?
    @State private var isCheckingInvite = false
    @State private var nameWasEdited = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add Contact")
                .font(.headline)

            TextField("Paste schat://invite/... link", text: $inviteText, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(3...6)

            HStack {
                Button {
                    if let pasted = Clipboard.readString() {
                        inviteText = pasted
                    }
                } label: {
                    Label("Paste Invite", systemImage: "doc.on.clipboard")
                }

                Button {
                    Task { await refreshInvitePreview(inviteText) }
                } label: {
                    Label("Verify", systemImage: "checkmark.shield")
                }
                .disabled(inviteText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }

            if isCheckingInvite {
                ProgressView()
                    .controlSize(.small)
            } else if let invitePreview {
                InvitePreviewCard(preview: invitePreview)
            } else if let previewError {
                Label(previewError, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.red)
            }

            TextField("Name (optional)", text: Binding(
                get: { displayName },
                set: { value in
                    displayName = value
                    nameWasEdited = true
                }
            ))
            .textFieldStyle(.roundedBorder)

            HStack {
                Button(invitePreview?.alreadyAdded == true ? "Update Contact" : "Add Contact") {
                    Task {
                        if invitePreview == nil {
                            await refreshInvitePreview(inviteText)
                        }
                        guard let invitePreview else { return }
                        let didAdd = await store.addContact(
                            displayName: effectiveDisplayName(for: invitePreview),
                            inviteURI: invitePreview.normalizedInviteUri
                        )
                        if didAdd {
                            dismiss()
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(invitePreview == nil || isCheckingInvite)
                Spacer()
                Button("Cancel") { dismiss() }
            }
        }
        .padding(20)
        .frame(width: 560)
        .task(id: inviteText) {
            await refreshInvitePreviewWhenReady(inviteText)
        }
    }

    private func refreshInvitePreviewWhenReady(_ text: String) async {
        let candidate = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !candidate.isEmpty else {
            invitePreview = nil
            previewError = nil
            return
        }
        guard candidate.localizedCaseInsensitiveContains("schat://invite/") else {
            invitePreview = nil
            previewError = nil
            return
        }
        try? await Task.sleep(nanoseconds: 300_000_000)
        guard text == inviteText else { return }
        await refreshInvitePreview(text)
    }

    private func refreshInvitePreview(_ text: String) async {
        let candidate = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !candidate.isEmpty else { return }
        isCheckingInvite = true
        defer { isCheckingInvite = false }
        do {
            let preview = try await store.previewInvite(candidate)
            invitePreview = preview
            previewError = nil
            if !nameWasEdited || displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                displayName = preview.suggestedDisplayName
                nameWasEdited = false
            }
        } catch {
            invitePreview = nil
            previewError = error.localizedDescription
        }
    }

    private func effectiveDisplayName(for preview: InvitePreview) -> String {
        let trimmed = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? preview.suggestedDisplayName : trimmed
    }
}

private struct InvitePreviewCard: View {
    let preview: InvitePreview

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                preview.alreadyAdded ? "Already in contacts" : "Invite verified",
                systemImage: preview.alreadyAdded ? "person.crop.circle.badge.checkmark" : "checkmark.shield.fill"
            )
            .font(.headline)
            .foregroundStyle(preview.alreadyAdded ? Color.secondary : Color.green)

            Text(preview.suggestedDisplayName)
                .font(.callout.weight(.semibold))
                .lineLimit(1)

            Text("Device \(shortDevice(preview.deviceId))")
                .font(.caption)
                .foregroundStyle(.secondary)

            if let relayHint = preview.relayHint, !relayHint.isEmpty {
                Text(relayHint)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            Text(preview.safetyNumber)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(2)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 8))
    }
}

private func shortDevice(_ deviceID: String) -> String {
    String(deviceID.prefix(8))
}
