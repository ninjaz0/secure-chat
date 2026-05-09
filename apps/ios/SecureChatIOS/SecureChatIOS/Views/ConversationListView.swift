import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var showingInvite = false
    @State private var showingAddContact = false
    @State private var showingCreateGroup = false
    @State private var showingTemporary = false
    @State private var showingSettings = false

    var body: some View {
        NavigationStack {
            List {
                if let profile = store.appSnapshot?.profile {
                    Section {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(profile.displayName)
                                .font(.headline)
                            Text("Device \(shortDevice(profile.deviceId))")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                Section("Chats") {
                    ForEach(store.appSnapshot?.contacts ?? []) { contact in
                        NavigationLink {
                            ChatView(contact: contact)
                        } label: {
                            ContactRow(contact: contact)
                        }
                    }
                }

                Section("Groups") {
                    ForEach(store.appSnapshot?.groups ?? []) { group in
                        NavigationLink {
                            GroupChatView(group: group)
                        } label: {
                            GroupRow(group: group)
                        }
                    }
                }

                Section("Temporary") {
                    ForEach(store.appSnapshot?.temporaryConnections ?? []) { connection in
                        NavigationLink {
                            TemporaryChatView(connection: connection)
                        } label: {
                            TemporaryRow(connection: connection)
                        }
                    }
                }
            }
            .navigationTitle("SecureChat")
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    Button {
                        showingInvite = true
                    } label: {
                        Image(systemName: "qrcode")
                    }

                    Button {
                        showingTemporary = true
                    } label: {
                        Image(systemName: "timer")
                    }

                    Button {
                        showingAddContact = true
                    } label: {
                        Image(systemName: "person.badge.plus")
                    }

                    Button {
                        showingCreateGroup = true
                    } label: {
                        Image(systemName: "person.3")
                    }

                    Button {
                        showingSettings = true
                    } label: {
                        Image(systemName: "gearshape")
                    }
                }
            }
            .refreshable {
                await store.receiveMessages()
            }
            .sheet(isPresented: $showingInvite) {
                InviteView()
                    .environmentObject(store)
            }
            .sheet(isPresented: $showingAddContact) {
                AddContactView()
                    .environmentObject(store)
            }
            .sheet(isPresented: $showingCreateGroup) {
                CreateGroupView()
                    .environmentObject(store)
            }
            .sheet(isPresented: $showingTemporary) {
                TemporaryConnectionView()
                    .environmentObject(store)
            }
            .sheet(isPresented: $showingSettings) {
                SettingsView()
                    .environmentObject(store)
            }
        }
    }
}

private struct ContactRow: View {
    let contact: AppContact

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: contact.verified ? "checkmark.shield.fill" : "person.crop.circle")
                .foregroundStyle(contact.verified ? .green : .secondary)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(contact.displayName)
                    .lineLimit(1)
                Text(contact.lastMessage ?? "Device \(shortDevice(contact.deviceId))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

private struct GroupRow: View {
    let group: AppGroup

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "person.3.fill")
                .foregroundStyle(.secondary)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(group.displayName)
                    .lineLimit(1)
                Text(group.lastMessage ?? "\(group.memberCount) members")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
    }
}

private struct TemporaryRow: View {
    let connection: TemporaryConnection

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "timer")
                .foregroundStyle(.orange)
                .frame(width: 24)
            VStack(alignment: .leading, spacing: 3) {
                Text(connection.displayName)
                    .lineLimit(1)
                Text(connection.lastMessage ?? "Device \(shortDevice(connection.deviceId))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Text(expiryText)
                .font(.caption2)
                .foregroundStyle(.secondary)
        }
    }

    private var expiryText: String {
        let remaining = Int64(connection.expiresUnix) - Int64(Date().timeIntervalSince1970)
        if remaining <= 0 { return "Expired" }
        let hours = max(1, remaining / 3600)
        return "\(hours)h"
    }
}

private struct CreateGroupView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var firstContactID = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Group") {
                    TextField("Group name", text: $name)
                    Picker("First member", selection: $firstContactID) {
                        Text("None").tag("")
                        ForEach(store.appSnapshot?.contacts ?? []) { contact in
                            Text(contact.displayName).tag(contact.id)
                        }
                    }
                }

                Section {
                    Button {
                        Task { await create() }
                    } label: {
                        Label("Create Group", systemImage: "person.3")
                    }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .navigationTitle("Group")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }

    private func create() async {
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        await store.createGroup(displayName: trimmed)
        if !firstContactID.isEmpty {
            store.selectedContactID = firstContactID
            await store.addSelectedContactToSelectedGroup()
        }
        dismiss()
    }
}

private struct TemporaryConnectionView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var inviteText = ""
    @State private var errorText: String?
    @State private var isStarting = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Invite") {
                    TextField("Paste temporary schat://invite/... link", text: $inviteText, axis: .vertical)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .lineLimit(4...8)

                    Button {
                        if let pasted = Clipboard.readString() {
                            inviteText = pasted
                        }
                    } label: {
                        Label("Paste Temporary Invite", systemImage: "doc.on.clipboard")
                    }

                    Button {
                        store.copyTemporaryInvite()
                        dismiss()
                    } label: {
                        Label("Copy Temporary Invite", systemImage: "timer")
                    }
                }

                if let errorText {
                    Section {
                        Label(errorText, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                }

                Section {
                    Button {
                        Task { await startTemporary() }
                    } label: {
                        Label("Start Temporary Chat", systemImage: "bubble.left.and.bubble.right")
                    }
                    .disabled(isStarting || inviteText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .navigationTitle("Temporary")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }

    private func startTemporary() async {
        let candidate = inviteText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !candidate.isEmpty else { return }
        isStarting = true
        defer { isStarting = false }
        let didStart = await store.startTemporaryConnection(inviteURI: candidate)
        if didStart {
            dismiss()
        } else {
            errorText = "Temporary invite invalid or expired"
        }
    }
}

func shortDevice(_ deviceID: String) -> String {
    String(deviceID.prefix(8))
}
