import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var showingInvite = false
    @State private var showingAddContact = false
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
                        showingAddContact = true
                    } label: {
                        Image(systemName: "person.badge.plus")
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

func shortDevice(_ deviceID: String) -> String {
    String(deviceID.prefix(8))
}
