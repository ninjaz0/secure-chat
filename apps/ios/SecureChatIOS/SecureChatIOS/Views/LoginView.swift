import SwiftUI

struct LoginView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var displayName = "Me"
    @State private var relayURL = "https://chat.example.com"

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Display name", text: $displayName)
                        .textContentType(.name)
                    TextField("Relay URL", text: $relayURL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                }

                Section {
                    Button {
                        Task {
                            await store.bootstrap(displayName: displayName, relayURL: relayURL)
                        }
                    } label: {
                        Label("Create / Login", systemImage: "person.badge.key")
                    }
                    .disabled(store.isLoading || displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .navigationTitle("SecureChat")
            .safeAreaInset(edge: .top) {
                VStack(spacing: 8) {
                    Image(systemName: "lock.bubble.left.and.bubble.right")
                        .font(.system(size: 44))
                        .foregroundStyle(.secondary)
                    Text("Anonymous E2EE identity")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.top, 16)
                .padding(.bottom, 8)
                .background(.bar)
            }
        }
    }
}
