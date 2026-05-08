import Foundation

@MainActor
final class SecureChatStore: ObservableObject {
    @Published private(set) var appSnapshot: AppSnapshot?
    @Published private(set) var selfTest: SelfTestResult?
    @Published private(set) var relaySmoke: RelaySmokeResult?
    @Published var selectedContactID: String?
    @Published var autoReceiveEnabled = true
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    private var autoReceiveTask: Task<Void, Never>?

    var selectedContact: AppContact? {
        appSnapshot?.contacts.first { $0.id == selectedContactID }
    }

    var selectedMessages: [AppChatMessage] {
        guard let selectedContactID else { return [] }
        return appSnapshot?.messages.filter { $0.contactId == selectedContactID } ?? []
    }

    var isReady: Bool {
        appSnapshot?.ready == true
    }

    func loadAppSnapshot() async {
        await runLoading {
            apply(snapshot: try SecureChatCoreClient.appSnapshot())
        }
    }

    func bootstrap(displayName: String, relayURL: String) async {
        await runLoading {
            apply(snapshot: try SecureChatCoreClient.bootstrap(displayName: displayName, relayURL: relayURL))
        }
    }

    func updateRelay(_ relayURL: String) async {
        await runLoading {
            apply(snapshot: try SecureChatCoreClient.updateRelay(relayURL))
        }
    }

    func addContact(displayName: String, inviteURI: String) async {
        await runLoading {
            apply(snapshot: try SecureChatCoreClient.addContact(displayName: displayName, inviteURI: inviteURI))
        }
    }

    func sendMessage(_ body: String) async {
        guard let selectedContactID else { return }
        await runLoading {
            apply(snapshot: try SecureChatCoreClient.sendMessage(contactID: selectedContactID, body: body))
        }
    }

    func receiveMessages() async {
        await runLoading {
            let report = try SecureChatCoreClient.receiveMessages()
            apply(snapshot: report.snapshot)
            NotificationService.notifyNewMessages(count: report.receivedCount)
        }
    }

    func runSelfTest() async {
        await runLoading {
            selfTest = try SecureChatCoreClient.runSelfTest()
        }
    }

    func runRelaySmoke() async {
        await runLoading {
            relaySmoke = try SecureChatCoreClient.runRelaySmoke()
        }
    }

    func startAutoReceive() {
        guard autoReceiveTask == nil else { return }
        autoReceiveTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(5))
                guard let self else { return }
                let shouldReceive = await MainActor.run {
                    self.autoReceiveEnabled && self.isReady && !self.isLoading
                }
                if shouldReceive {
                    await self.receiveMessages()
                }
            }
        }
    }

    func stopAutoReceive() {
        autoReceiveTask?.cancel()
        autoReceiveTask = nil
    }

    func copyOwnInvite() {
        if let invite = try? SecureChatCoreClient.ownInvite() {
            Clipboard.copy(invite.inviteUri)
        } else if let invite = appSnapshot?.profile?.inviteUri {
            Clipboard.copy(invite)
        }
    }

    private func runLoading(_ work: () throws -> Void) async {
        isLoading = true
        defer { isLoading = false }
        do {
            try work()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func apply(snapshot: AppSnapshot) {
        appSnapshot = snapshot
        if selectedContactID == nil || !(snapshot.contacts.contains { $0.id == selectedContactID }) {
            selectedContactID = snapshot.contacts.first?.id
        }
    }
}
