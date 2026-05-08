import SwiftUI

struct MessengerView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var showingInvite = false
    @State private var showingAddContact = false
    @State private var showingSettings = false

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

                Toggle(isOn: $store.autoReceiveEnabled) {
                    Label("Auto", systemImage: "arrow.triangle.2.circlepath")
                }
                .toggleStyle(.switch)
                .help("Automatically receive messages")

                Button {
                    showingSettings = true
                } label: {
                    Label("Relay", systemImage: "network")
                }
                .help("Relay settings")
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
        .sheet(isPresented: $showingSettings) {
            RelaySettingsSheet()
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
                            MessageBubble(message: message)
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

    var isOutgoing: Bool { message.direction == .outgoing }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 80) }
            VStack(alignment: .leading, spacing: 5) {
                Text(message.body)
                    .textSelection(.enabled)
                Text(message.status.rawValue)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(isOutgoing ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
            if !isOutgoing { Spacer(minLength: 80) }
        }
    }
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
    @State private var inviteURI = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Add Contact")
                .font(.headline)
            TextField("Display name", text: $displayName)
                .textFieldStyle(.roundedBorder)
            TextField("schat://invite/...", text: $inviteURI, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(3...6)
            HStack {
                Button("Add") {
                    Task {
                        await store.addContact(displayName: displayName, inviteURI: inviteURI)
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(displayName.isEmpty || inviteURI.isEmpty)
                Spacer()
                Button("Cancel") { dismiss() }
            }
        }
        .padding(20)
        .frame(width: 520)
    }
}

private struct RelaySettingsSheet: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var relayURL = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Relay")
                .font(.headline)
            TextField("Relay URL", text: $relayURL)
                .textFieldStyle(.roundedBorder)
                .onAppear {
                    relayURL = store.appSnapshot?.profile?.relayUrl ?? "http://127.0.0.1:8787"
                }
            HStack {
                Button("Save") {
                    Task {
                        await store.updateRelay(relayURL)
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                Spacer()
                Button("Cancel") { dismiss() }
            }
        }
        .padding(20)
        .frame(width: 460)
    }
}

private func shortDevice(_ deviceID: String) -> String {
    String(deviceID.prefix(8))
}
