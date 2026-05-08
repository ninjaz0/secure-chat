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
                            MessageBubble(message: message)
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

                Toggle(isOn: $store.autoReceiveEnabled) {
                    Image(systemName: "arrow.triangle.2.circlepath")
                }
            }
        }
        .onAppear {
            store.selectedContactID = contact.id
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

private struct MessageBubble: View {
    let message: AppChatMessage

    private var isOutgoing: Bool {
        message.direction == .outgoing
    }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 44) }
            VStack(alignment: .leading, spacing: 5) {
                Text(message.body)
                    .textSelection(.enabled)
                Text(message.status.rawValue)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
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
