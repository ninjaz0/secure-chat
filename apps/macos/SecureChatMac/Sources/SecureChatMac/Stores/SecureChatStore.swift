import Foundation

@MainActor
final class SecureChatStore: ObservableObject {
    @Published private(set) var demo: DemoState?
    @Published private(set) var selfTest: SelfTestResult?
    @Published private(set) var relaySmoke: RelaySmokeResult?
    @Published private(set) var appSnapshot: AppSnapshot?
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

    func loadDemo() async {
        isLoading = true
        defer { isLoading = false }
        do {
            demo = try SecureChatCoreClient.loadDemoState()
            errorMessage = demo?.ok == true ? nil : demo?.error
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func runSelfTest() async {
        isLoading = true
        defer { isLoading = false }
        do {
            selfTest = try SecureChatCoreClient.runSelfTest()
            errorMessage = selfTest?.ok == true ? nil : "Protocol self-test failed."
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func runRelaySmoke() async {
        isLoading = true
        defer { isLoading = false }
        do {
            relaySmoke = try SecureChatCoreClient.runRelaySmoke()
            errorMessage = relaySmoke?.ok == true ? nil : "Relay smoke failed."
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func loadAppSnapshot() async {
        isLoading = true
        defer { isLoading = false }
        do {
            apply(snapshot: try SecureChatCoreClient.appSnapshot())
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func bootstrap(displayName: String, relayURL: String) async {
        isLoading = true
        defer { isLoading = false }
        do {
            apply(snapshot: try SecureChatCoreClient.bootstrap(displayName: displayName, relayURL: relayURL))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func updateRelay(_ relayURL: String) async {
        isLoading = true
        defer { isLoading = false }
        do {
            apply(snapshot: try SecureChatCoreClient.updateRelay(relayURL))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func addContact(displayName: String, inviteURI: String) async {
        isLoading = true
        defer { isLoading = false }
        do {
            apply(snapshot: try SecureChatCoreClient.addContact(displayName: displayName, inviteURI: inviteURI))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func sendMessage(_ body: String) async {
        guard let selectedContactID else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            apply(snapshot: try SecureChatCoreClient.sendMessage(contactID: selectedContactID, body: body))
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func receiveMessages() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let report = try SecureChatCoreClient.receiveMessages()
            apply(snapshot: report.snapshot)
            NotificationService.notifyNewMessages(count: report.receivedCount)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
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

    private func apply(snapshot: AppSnapshot) {
        appSnapshot = snapshot
        if selectedContactID == nil || !(snapshot.contacts.contains { $0.id == selectedContactID }) {
            selectedContactID = snapshot.contacts.first?.id
        }
    }
}
