import SwiftUI

struct AddContactView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var displayName = ""
    @State private var inviteURI = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Display name", text: $displayName)
                    TextField("schat://invite/...", text: $inviteURI, axis: .vertical)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .lineLimit(4...8)
                }

                Section {
                    Button {
                        Task {
                            await store.addContact(displayName: displayName, inviteURI: inviteURI)
                            dismiss()
                        }
                    } label: {
                        Label("Add Contact", systemImage: "person.badge.plus")
                    }
                    .disabled(displayName.isEmpty || inviteURI.isEmpty)
                }
            }
            .navigationTitle("Add Contact")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}
