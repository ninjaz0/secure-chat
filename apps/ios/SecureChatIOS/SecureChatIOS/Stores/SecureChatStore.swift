import Foundation

@MainActor
final class SecureChatStore: ObservableObject {
    @Published private(set) var appSnapshot: AppSnapshot?
    @Published private(set) var selfTest: SelfTestResult?
    @Published private(set) var relaySmoke: RelaySmokeResult?
    @Published var selectedContactID: String?
    @Published var autoReceiveEnabled: Bool {
        didSet { defaults.set(autoReceiveEnabled, forKey: PreferenceKey.autoReceiveEnabled) }
    }
    @Published var receivePollIntervalSeconds: Int {
        didSet { defaults.set(Self.clampedPollInterval(receivePollIntervalSeconds), forKey: PreferenceKey.receivePollIntervalSeconds) }
    }
    @Published var notifyOnNewMessages: Bool {
        didSet { defaults.set(notifyOnNewMessages, forKey: PreferenceKey.notifyOnNewMessages) }
    }
    @Published var playNotificationSound: Bool {
        didSet { defaults.set(playNotificationSound, forKey: PreferenceKey.playNotificationSound) }
    }
    @Published var showMessageTimestamps: Bool {
        didSet { defaults.set(showMessageTimestamps, forKey: PreferenceKey.showMessageTimestamps) }
    }
    @Published var showMessageStatus: Bool {
        didSet { defaults.set(showMessageStatus, forKey: PreferenceKey.showMessageStatus) }
    }
    @Published var preferQuic: Bool {
        didSet { defaults.set(preferQuic, forKey: PreferenceKey.preferQuic) }
    }
    @Published var coverTrafficEnabled: Bool {
        didSet { defaults.set(coverTrafficEnabled, forKey: PreferenceKey.coverTrafficEnabled) }
    }
    @Published private(set) var errorMessage: String?
    @Published private(set) var isLoading = false
    private let defaults: UserDefaults
    private var autoReceiveTask: Task<Void, Never>?

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        autoReceiveEnabled = Self.boolPreference(PreferenceKey.autoReceiveEnabled, defaultValue: true, defaults: defaults)
        receivePollIntervalSeconds = Self.clampedPollInterval(
            Self.intPreference(PreferenceKey.receivePollIntervalSeconds, defaultValue: 5, defaults: defaults)
        )
        notifyOnNewMessages = Self.boolPreference(PreferenceKey.notifyOnNewMessages, defaultValue: true, defaults: defaults)
        playNotificationSound = Self.boolPreference(PreferenceKey.playNotificationSound, defaultValue: true, defaults: defaults)
        showMessageTimestamps = Self.boolPreference(PreferenceKey.showMessageTimestamps, defaultValue: false, defaults: defaults)
        showMessageStatus = Self.boolPreference(PreferenceKey.showMessageStatus, defaultValue: true, defaults: defaults)
        preferQuic = Self.boolPreference(PreferenceKey.preferQuic, defaultValue: true, defaults: defaults)
        coverTrafficEnabled = Self.boolPreference(PreferenceKey.coverTrafficEnabled, defaultValue: true, defaults: defaults)
    }

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

    func previewInvite(_ inviteText: String) async throws -> InvitePreview {
        try SecureChatCoreClient.previewInvite(inviteText)
    }

    @discardableResult
    func addContact(displayName: String, inviteURI: String) async -> Bool {
        return await runLoading {
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
            if notifyOnNewMessages {
                NotificationService.notifyNewMessages(count: report.receivedCount, soundEnabled: playNotificationSound)
            }
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
                guard let self else { return }
                let interval = await MainActor.run {
                    Self.clampedPollInterval(self.receivePollIntervalSeconds)
                }
                try? await Task.sleep(nanoseconds: UInt64(interval) * 1_000_000_000)
                guard !Task.isCancelled else { return }
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

    func resetPreferences() {
        autoReceiveEnabled = true
        receivePollIntervalSeconds = 5
        notifyOnNewMessages = true
        playNotificationSound = true
        showMessageTimestamps = false
        showMessageStatus = true
        preferQuic = true
        coverTrafficEnabled = true
    }

    @discardableResult
    private func runLoading(_ work: () throws -> Void) async -> Bool {
        isLoading = true
        defer { isLoading = false }
        do {
            try work()
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    private func apply(snapshot: AppSnapshot) {
        appSnapshot = snapshot
        if selectedContactID == nil || !(snapshot.contacts.contains { $0.id == selectedContactID }) {
            selectedContactID = snapshot.contacts.first?.id
        }
    }

    private static func boolPreference(_ key: String, defaultValue: Bool, defaults: UserDefaults) -> Bool {
        guard defaults.object(forKey: key) != nil else { return defaultValue }
        return defaults.bool(forKey: key)
    }

    private static func intPreference(_ key: String, defaultValue: Int, defaults: UserDefaults) -> Int {
        guard defaults.object(forKey: key) != nil else { return defaultValue }
        return defaults.integer(forKey: key)
    }

    private static func clampedPollInterval(_ value: Int) -> Int {
        min(max(value, 5), 300)
    }
}

private enum PreferenceKey {
    static let autoReceiveEnabled = "autoReceiveEnabled"
    static let receivePollIntervalSeconds = "receivePollIntervalSeconds"
    static let notifyOnNewMessages = "notifyOnNewMessages"
    static let playNotificationSound = "playNotificationSound"
    static let showMessageTimestamps = "showMessageTimestamps"
    static let showMessageStatus = "showMessageStatus"
    static let preferQuic = "preferQuic"
    static let coverTrafficEnabled = "coverTrafficEnabled"
}
