import SwiftUI
import AppKit
import UniformTypeIdentifiers

struct MessengerView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var showingInvite = false
    @State private var showingAddContact = false
    @State private var showingTemporary = false

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
                    showingTemporary = true
                } label: {
                    Label("Temporary", systemImage: "timer")
                }
                .help("Temporary connection")

                Button {
                    Task { await store.receiveMessages() }
                } label: {
                    Label("Receive", systemImage: "tray.and.arrow.down")
                }
                .help("Pull encrypted messages from relay")
                .disabled(store.isLoading || store.isReceiving)

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
        .sheet(isPresented: $showingTemporary) {
            TemporaryConnectionSheet()
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
            Section("Temporary") {
                ForEach(store.appSnapshot?.temporaryConnections ?? []) { connection in
                    Button {
                        store.selectedTemporaryConnectionID = connection.id
                        store.selectedContactID = nil
                    } label: {
                        HStack(spacing: 10) {
                            Image(systemName: "timer")
                                .foregroundStyle(.orange)
                                .frame(width: 18)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(connection.displayName)
                                    .lineLimit(1)
                                Text(connection.lastMessage ?? shortDevice(connection.deviceId))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .listStyle(.sidebar)
        .navigationTitle(store.appSnapshot?.profile?.displayName ?? "SecureChat")
        .onChange(of: store.selectedContactID) { _, newValue in
            if newValue != nil {
                store.selectedTemporaryConnectionID = nil
            }
        }
    }
}

private struct ChatConversationView: View {
    @EnvironmentObject private var store: SecureChatStore
    @State private var draft = ""

    var body: some View {
        if let connection = store.selectedTemporaryConnection {
            TemporaryConversationView(connection: connection)
        } else if let contact = store.selectedContact {
            VStack(spacing: 0) {
                ChatHeaderView(contact: contact)
                Divider()
                SmartMessageScrollView(
                    threadID: contact.id,
                    messages: store.selectedMessages,
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

private struct TemporaryConversationView: View {
    @EnvironmentObject private var store: SecureChatStore
    let connection: TemporaryConnection
    @State private var draft = ""

    var body: some View {
        VStack(spacing: 0) {
            TemporaryHeaderView(connection: connection)
            Divider()
            SmartMessageScrollView(
                threadID: connection.id,
                messages: store.selectedTemporaryMessages,
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
                let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendTemporaryMessage(body) }
            }
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
                    .padding(16)
                    .frame(maxWidth: .infinity)
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
                    .controlSize(.small)
                    .buttonStyle(.borderedProminent)
                    .padding(.bottom, 12)
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

private struct ChatHeaderView: View {
    @EnvironmentObject private var store: SecureChatStore
    let contact: AppContact
    @State private var showingEdit = false
    @State private var showingDelete = false

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
            Button {
                showingEdit = true
            } label: {
                Image(systemName: "pencil")
            }
            .help("Edit nickname")
            Button(role: .destructive) {
                showingDelete = true
            } label: {
                Image(systemName: "trash")
            }
            .help("Delete contact")
            Text(shortDevice(contact.deviceId))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .sheet(isPresented: $showingEdit) {
            EditContactSheet(contact: contact)
                .environmentObject(store)
        }
        .confirmationDialog(
            "Delete this contact and local chat history?",
            isPresented: $showingDelete,
            titleVisibility: .visible
        ) {
            Button("Delete Contact", role: .destructive) {
                Task { await store.deleteContact(contactID: contact.id) }
            }
            Button("Cancel", role: .cancel) {}
        }
    }
}

private struct TemporaryHeaderView: View {
    @EnvironmentObject private var store: SecureChatStore
    let connection: TemporaryConnection

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: "timer")
                .foregroundStyle(.orange)
            VStack(alignment: .leading, spacing: 2) {
                Text(connection.displayName)
                    .font(.headline)
                Text("Safety \(connection.safetyNumber)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .textSelection(.enabled)
            }
            Spacer()
            Button(role: .destructive) {
                Task { await store.endTemporaryConnection() }
            } label: {
                Label("End", systemImage: "xmark.circle")
            }
            Text(shortDevice(connection.deviceId))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
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

    var isOutgoing: Bool { direction == .outgoing }

    var body: some View {
        HStack {
            if isOutgoing { Spacer(minLength: 80) }
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
            .background(isOutgoing ? Color.accentColor.opacity(0.18) : Color.secondary.opacity(0.12), in: RoundedRectangle(cornerRadius: 8))
            if !isOutgoing { Spacer(minLength: 80) }
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

    var body: some View {
        HStack(spacing: 10) {
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
                    .frame(width: 28, height: 28)
            }
            TextField("Message", text: $draft, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...4)
            Button {
                let body = draft.trimmingCharacters(in: .whitespacesAndNewlines)
                draft = ""
                Task { await store.sendBurnMessage(body) }
            } label: {
                Image(systemName: "flame.fill")
            }
            .help("Send burn after reading")
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            Button(action: send) {
                Label("Send", systemImage: "paperplane.fill")
            }
            .keyboardShortcut(.return, modifiers: [.command])
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
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
    }
}

private struct AttachmentPreview: View {
    let attachment: AttachmentContent?
    let compact: Bool

    var body: some View {
        if let attachment {
            VStack(alignment: .leading, spacing: 6) {
                if let path = attachment.localPath,
                   let image = NSImage(contentsOfFile: path) {
                    Image(nsImage: image)
                        .resizable()
                        .scaledToFit()
                        .frame(maxWidth: compact ? 120 : 260, maxHeight: compact ? 120 : 220)
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
                    Button {
                        NSWorkspace.shared.open(URL(fileURLWithPath: path))
                    } label: {
                        Image(systemName: "arrow.up.right.square")
                    }
                    .buttonStyle(.plain)
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
        VStack(alignment: .leading, spacing: 14) {
            Text("Edit Nickname")
                .font(.headline)
            TextField("Nickname", text: $displayName)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Save") {
                    let name = displayName.trimmingCharacters(in: .whitespacesAndNewlines)
                    Task {
                        await store.updateContactDisplayName(contactID: contact.id, displayName: name)
                        dismiss()
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 360)
        .onAppear { displayName = contact.displayName }
    }
}

private func byteCount(_ bytes: UInt64) -> String {
    ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)
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
                Button("Copy Temporary") {
                    store.copyTemporaryInvite()
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
                    Label("Check", systemImage: "checkmark.shield")
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
                .disabled(invitePreview == nil || invitePreview?.temporary == true || isCheckingInvite)
                if invitePreview?.temporary == true {
                    Button("Start Temporary") {
                        Task {
                            guard let invitePreview else { return }
                            let didStart = await store.startTemporaryConnection(
                                inviteURI: invitePreview.normalizedInviteUri
                            )
                            if didStart {
                                dismiss()
                            }
                        }
                    }
                }
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

private struct TemporaryConnectionSheet: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var inviteText = ""
    @State private var previewError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Temporary Connection")
                .font(.headline)

            TextField("Paste temporary schat://invite/... link", text: $inviteText, axis: .vertical)
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
                    store.copyTemporaryInvite()
                    dismiss()
                } label: {
                    Label("Copy Temporary Invite", systemImage: "timer")
                }
            }

            if let previewError {
                Label(previewError, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .foregroundStyle(.red)
            }

            HStack {
                Button("Start") {
                    Task {
                        let didStart = await store.startTemporaryConnection(inviteURI: inviteText)
                        if didStart {
                            dismiss()
                        } else {
                            previewError = store.errorMessage
                        }
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(inviteText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Spacer()
                Button("Cancel") { dismiss() }
            }
        }
        .padding(20)
        .frame(width: 560)
    }
}

private struct InvitePreviewCard: View {
    let preview: InvitePreview

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                preview.temporary ? "Temporary invite" : (preview.alreadyAdded ? "Already in contacts" : "Invite valid"),
                systemImage: preview.temporary ? "timer" : (preview.alreadyAdded ? "person.crop.circle.badge.checkmark" : "checkmark.shield.fill")
            )
            .font(.headline)
            .foregroundStyle(preview.temporary ? Color.orange : (preview.alreadyAdded ? Color.secondary : Color.green))

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
