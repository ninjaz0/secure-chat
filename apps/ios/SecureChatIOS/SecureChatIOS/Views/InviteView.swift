import SwiftUI

struct InviteView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(store.appSnapshot?.profile?.inviteUri ?? "")
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .lineLimit(8)
                }

                Section {
                    Button {
                        store.copyOwnInvite()
                        dismiss()
                    } label: {
                        Label("Copy Invite", systemImage: "doc.on.doc")
                    }
                }
            }
            .navigationTitle("My Invite")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}
