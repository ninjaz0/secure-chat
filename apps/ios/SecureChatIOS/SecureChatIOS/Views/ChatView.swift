import SwiftUI
import PhotosUI
import UIKit
import UniformTypeIdentifiers

struct ChatView: View {
    @EnvironmentObject private var store: SecureChatStore
    let contact: AppContact
    @State private var draft = ""
    @State private var showingEditContact = false
    @State private var showingDeleteContact = false

    var messages: [AppChatMessage] {
        store.appSnapshot?.messages.filter { $0.contactId == contact.id } ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            SafetyBanner(contact: contact)
            Divider()
            SmartMessageScrollView(
                threadID: contact.id,
                messages: messages,
                isOutgoing: { $0.direction == .outgoing }
            ) { message in
                MessageBubble(
                    messageID: message.id,
                    direction: message.direction,
                    messageBody: message.body,
                    content: message.content,
                    status: message.status,
                    sentAtUnix: message.sentAtUnix,
                    receivedAtUnix: message.receivedAtUnix,
                    showsStatus: store.showMessageStatus,
                    showsTimestamp: store.showMessageTimestamps,
                    openBurn: { Task { await store.openBurnMessage(messageID: message.id) } }
                )
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
                    showingEditContact = true
                } label: {
                    Image(systemName: "pencil")
                }

                Button(role: .destructive) {
                    showingDeleteContact = true
                } label: {
                    Image(systemName: "trash")
                }

                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Image(systemName: "tray.and.arrow.down")
                }
            }
        }
        .sheet(isPresented: $showingEditContact) {
            EditContactSheet(contact: contact)
                .environmentObject(store)
        }
        .confirmationDialog(
            "Delete this contact and local chat history?",
            isPresented: $showingDeleteContact,
            titleVisibility: .visible
        ) {
            Button("Delete Contact", role: .destructive) {
                Task { await store.deleteContact(contactID: contact.id) }
            }
            Button("Cancel", role: .cancel) {}
        }
        .onAppear {
            store.selectedContactID = contact.id
            store.selectedTemporaryConnectionID = nil
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
            SmartMessageScrollView(
                threadID: connection.id,
                messages: messages,
                isOutgoing: { $0.direction == .outgoing }
            ) { message in
                MessageBubble(
                    messageID: message.id,
                    direction: message.direction,
                    messageBody: message.body,
                    content: message.content,
                    status: message.status,
                    sentAtUnix: message.sentAtUnix,
                    receivedAtUnix: message.receivedAtUnix,
                    showsStatus: store.showMessageStatus,
                    showsTimestamp: store.showMessageTimestamps,
                    openBurn: { Task { await store.openBurnMessage(messageID: message.id) } }
                )
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

private struct SmartMessageScrollView<Message: Identifiable, RowContent: View>: View where Message.ID: Hashable {
    let threadID: String
    let messages: [Message]
    let isOutgoing: (Message) -> Bool
    @ViewBuilder let rowContent: (Message) -> RowContent

    @State private var viewportHeight: CGFloat = 0
    @State private var bottomY: CGFloat = 0
    @State private var isPinnedToBottom = true
    @State private var hasNewMessages = false
    @State private var lastSignature = ""

    private let bottomID = "message-bottom-sentinel"
    private let coordinateSpace = "smart-message-scroll"
    private let bottomThreshold: CGFloat = 80

    private var signature: String {
        "\(messages.count)-\(String(describing: messages.last?.id))"
    }

    var body: some View {
        ScrollViewReader { proxy in
            ZStack(alignment: .bottom) {
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(messages) { message in
                            rowContent(message)
                                .id(message.id)
                        }
                        Color.clear
                            .frame(height: 1)
                            .id(bottomID)
                            .background(
                                GeometryReader { geometry in
                                    Color.clear.preference(
                                        key: MessageBottomOffsetPreferenceKey.self,
                                        value: geometry.frame(in: .named(coordinateSpace)).maxY
                                    )
                                }
                            )
                    }
                    .padding(14)
                }
                .coordinateSpace(name: coordinateSpace)
                .background(
                    GeometryReader { geometry in
                        Color.clear.preference(
                            key: MessageViewportHeightPreferenceKey.self,
                            value: geometry.size.height
                        )
                    }
                )
                .onPreferenceChange(MessageViewportHeightPreferenceKey.self) { height in
                    viewportHeight = height
                    updatePinnedState()
                }
                .onPreferenceChange(MessageBottomOffsetPreferenceKey.self) { offset in
                    bottomY = offset
                    updatePinnedState()
                }
                .onAppear {
                    resetThread(proxy)
                }
                .onChange(of: threadID) { _ in
                    resetThread(proxy)
                }
                .onChange(of: signature) { newSignature in
                    guard newSignature != lastSignature else { return }
                    defer { lastSignature = newSignature }
                    guard let lastMessage = messages.last else {
                        hasNewMessages = false
                        return
                    }
                    if isPinnedToBottom || isOutgoing(lastMessage) {
                        scrollToBottom(proxy, animated: true)
                        hasNewMessages = false
                    } else {
                        hasNewMessages = true
                    }
                }

                if hasNewMessages && !isPinnedToBottom {
                    Button {
                        scrollToBottom(proxy, animated: true)
                        hasNewMessages = false
                    } label: {
                        Label("New Messages", systemImage: "arrow.down.circle.fill")
                    }
                    .font(.footnote.weight(.semibold))
                    .buttonStyle(.borderedProminent)
                    .padding(.bottom, 10)
                }
            }
        }
    }

    private func updatePinnedState() {
        let pinned = bottomY <= viewportHeight + bottomThreshold
        isPinnedToBottom = pinned
        if pinned {
            hasNewMessages = false
        }
    }

    private func scrollToBottom(_ proxy: ScrollViewProxy, animated: Bool) {
        DispatchQueue.main.async {
            if animated {
                withAnimation(.easeOut(duration: 0.2)) {
                    proxy.scrollTo(bottomID, anchor: .bottom)
                }
            } else {
                proxy.scrollTo(bottomID, anchor: .bottom)
            }
        }
    }

    private func resetThread(_ proxy: ScrollViewProxy) {
        lastSignature = signature
        hasNewMessages = false
        isPinnedToBottom = true
        scrollToBottom(proxy, animated: false)
    }
}

private struct MessageViewportHeightPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct MessageBottomOffsetPreferenceKey: PreferenceKey {
    static var defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
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
    let messageID: String
    let direction: AppMessageDirection
    let messageBody: String
    let content: MessageContent
    let status: AppMessageStatus
    let sentAtUnix: UInt64
    let receivedAtUnix: UInt64?
    let showsStatus: Bool
    let showsTimestamp: Bool
    let openBurn: () -> Void
    @State private var showingBurn = false

    private var isOutgoing: Bool {
        direction == .outgoing
    }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 44) }
            VStack(alignment: .leading, spacing: 5) {
                contentView
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
        .alert("Burn After Reading", isPresented: $showingBurn) {
            Button("Destroy", role: .destructive) {
                openBurn()
            }
        } message: {
            Text(content.text ?? "")
        }
    }

    @ViewBuilder
    private var contentView: some View {
        switch content.kind {
        case "burn":
            if content.destroyed {
                Label("Burned message", systemImage: "flame")
                    .foregroundStyle(.secondary)
            } else {
                Button {
                    showingBurn = true
                } label: {
                    Label("Open burn after reading", systemImage: "flame.fill")
                }
                .buttonStyle(.bordered)
            }
        case "image", "sticker":
            AttachmentPreview(attachment: content.attachment, compact: content.kind == "sticker")
        case "file":
            FileAttachmentView(attachment: content.attachment)
        default:
            Text(content.text ?? messageBody)
                .textSelection(.enabled)
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
    @EnvironmentObject private var store: SecureChatStore
    @Binding var draft: String
    let send: () -> Void
    @State private var importerKind = "file"
    @State private var showingImporter = false
    @State private var selectedPhotoItem: PhotosPickerItem?

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            Menu {
                Button {
                    importerKind = "file"
                    showingImporter = true
                } label: {
                    Label("Send File", systemImage: "paperclip")
                }
                Button {
                    importerKind = "image"
                    showingImporter = true
                } label: {
                    Label("Send Image", systemImage: "photo")
                }
                Divider()
                ForEach(store.appSnapshot?.stickers ?? []) { sticker in
                    Button(sticker.displayName) {
                        Task { await store.sendSticker(sticker) }
                    }
                }
                Button {
                    importerKind = "sticker"
                    showingImporter = true
                } label: {
                    Label("Import Sticker", systemImage: "face.smiling")
                }
            } label: {
                Image(systemName: "plus.circle.fill")
                    .frame(width: 34, height: 34)
            }

            PhotosPicker(selection: $selectedPhotoItem, matching: .images) {
                Image(systemName: "photo.on.rectangle")
                    .frame(width: 34, height: 34)
            }

            TextField("Message", text: $draft, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...5)
            Button {
                let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendBurnMessage(text) }
            } label: {
                Image(systemName: "flame.fill")
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.bordered)
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Button(action: send) {
                Image(systemName: "paperplane.fill")
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.borderedProminent)
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
        .background(.bar)
        .fileImporter(isPresented: $showingImporter, allowedContentTypes: [.item], allowsMultipleSelection: false) { result in
            guard let url = try? result.get().first else { return }
            Task {
                if importerKind == "sticker" {
                    await store.importSticker(fileURL: url)
                } else {
                    await store.sendAttachment(fileURL: url, kind: importerKind)
                }
            }
        }
        .onChange(of: selectedPhotoItem) { item in
            guard let item else { return }
            Task {
                defer { selectedPhotoItem = nil }
                guard let data = try? await item.loadTransferable(type: Data.self) else { return }
                let fileExtension = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                let url = FileManager.default.temporaryDirectory
                    .appendingPathComponent(UUID().uuidString)
                    .appendingPathExtension(fileExtension)
                do {
                    try data.write(to: url, options: [.atomic])
                    await store.sendAttachment(fileURL: url, kind: "image")
                } catch {
                }
            }
        }
    }
}

private struct AttachmentPreview: View {
    let attachment: AttachmentContent?
    let compact: Bool

    var body: some View {
        if let attachment {
            VStack(alignment: .leading, spacing: 6) {
                if let path = attachment.localPath,
                   let image = UIImage(contentsOfFile: path) {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: compact ? 128 : 260, maxHeight: compact ? 128 : 220)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
                Text(attachment.fileName)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        } else {
            Label("Image", systemImage: "photo")
        }
    }
}

private struct FileAttachmentView: View {
    let attachment: AttachmentContent?

    var body: some View {
        if let attachment {
            HStack(spacing: 10) {
                Image(systemName: "doc.fill")
                    .font(.title3)
                VStack(alignment: .leading, spacing: 2) {
                    Text(attachment.fileName)
                        .lineLimit(1)
                    Text(byteCount(attachment.sizeBytes))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if let path = attachment.localPath {
                    ShareLink(item: URL(fileURLWithPath: path)) {
                        Image(systemName: "square.and.arrow.up")
                    }
                }
            }
        } else {
            Label("File", systemImage: "doc")
        }
    }
}

private struct EditContactSheet: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    let contact: AppContact
    @State private var displayName = ""

    var body: some View {
        NavigationStack {
            Form {
                TextField("Nickname", text: $displayName)
            }
            .navigationTitle("Edit Nickname")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        let name = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
                        Task {
                            await store.updateContactDisplayName(contactID: contact.id, displayName: name)
                            dismiss()
                        }
                    }
                    .disabled(displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .onAppear { displayName = contact.displayName }
    }
}

private func byteCount(_ bytes: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
}
