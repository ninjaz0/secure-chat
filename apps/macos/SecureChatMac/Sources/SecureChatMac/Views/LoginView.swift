import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var displayName = NSFullUserName().isEmpty ? "Me" : NSFullUserName()
    @State private var relayURL = "http://127.0.0.1:8787"

    var body: some View {
        VStack(spacing: 24) {
            Image(systemName: "lock.fill")
                .font(.system(size: 56))
                .foregroundStyle(.secondary)

            VStack(spacing: 8) {
                Text("SecureChat")
                    .font(.largeTitle.weight(.semibold))
                Text("Create a local encrypted identity and connect to a relay.")
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 12) {
                TextField("Display name", text: $displayName)
                    .textFieldStyle(.roundedBorder)
                TextField("Relay URL", text: $relayURL)
                    .textFieldStyle(.roundedBorder)

                Button {
                    Task {
                        await store.bootstrap(displayName: displayName, relayURL: relayURL)
                    }
                } label: {
                    Label("Create / Login", systemImage: "person.badge.key")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(store.isLoading || displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .frame(width: 420)
            .padding(18)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 8))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
