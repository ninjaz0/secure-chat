import Foundation

@MainActor
final class SecureChatStore: ObservableObject {
    @Published private(set) var demo: DemoState?
    @Published private(set) var selfTest: SelfTestResult?
    @Published private(set) var relaySmoke: RelaySmokeResult?
    @Published private(set) var p2pProbe: P2pProbeResult?
    @Published private(set) var appSnapshot: AppSnapshot?
    @Published var selectedContactID: String?
    @Published var selectedGroupID: String?
    @Published var selectedTemporaryConnectionID: String?
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
    @Published private(set) var isReceiving = false
    private let defaults: UserDefaults
    private var autoReceiveTask: Task<Void, Never>?
    private var receiveInFlight = false

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

    var selectedGroup: AppGroup? {
        appSnapshot?.groups.first { $0.id == selectedGroupID }
    }

    var selectedGroupMessages: [AppGroupMessage] {
        guard let selectedGroupID else { return [] }
        return appSnapshot?.groupMessages.filter { $0.groupId == selectedGroupID } ?? []
    }

    var selectedTemporaryConnection: TemporaryConnection? {
        appSnapshot?.temporaryConnections.first { $0.id == selectedTemporaryConnectionID }
    }

    var selectedTemporaryMessages: [TemporaryMessage] {
        guard let selectedTemporaryConnectionID else { return [] }
        return appSnapshot?.temporaryMessages.filter { $0.connectionId == selectedTemporaryConnectionID } ?? []
    }

    var isReady: Bool {
        appSnapshot?.ready == true
    }

    func loadDemo() async {
        await runLoading {
            demo = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.loadDemoState()
            }
            errorMessage = demo?.ok == true ? nil : demo?.error
        }
    }

    func runSelfTest() async {
        await runLoading {
            selfTest = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.runSelfTest()
            }
            errorMessage = selfTest?.ok == true ? nil : "Protocol self-test failed."
        }
    }

    func runRelaySmoke() async {
        await runLoading {
            relaySmoke = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.runRelaySmoke()
            }
            errorMessage = relaySmoke?.ok == true ? nil : "Relay smoke failed."
        }
    }

    func runP2PProbe() async {
        await runLoading {
            p2pProbe = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.runP2PProbe()
            }
            errorMessage = p2pProbe?.ok == true ? nil : "P2P NAT probe failed."
        }
    }

    func loadAppSnapshot() async {
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.appSnapshot()
            }
            apply(snapshot: snapshot)
        }
    }

    func bootstrap(displayName: String, relayURL: String) async {
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.bootstrap(displayName: displayName, relayURL: relayURL)
            }
            apply(snapshot: snapshot)
        }
    }

    func updateRelay(_ relayURL: String) async {
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.updateRelay(relayURL)
            }
            apply(snapshot: snapshot)
        }
    }

    func previewInvite(_ inviteText: String) async throws -> InvitePreview {
        try await SecureChatCoreClient.runInBackground {
            try SecureChatCoreClient.previewInvite(inviteText)
        }
    }

    @discardableResult
    func addContact(displayName: String, inviteURI: String) async -> Bool {
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.addContact(displayName: displayName, inviteURI: inviteURI)
            }
            apply(snapshot: snapshot)
        }
    }

    func sendMessage(_ body: String) async {
        guard let selectedContactID else { return }
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.sendMessage(contactID: selectedContactID, body: body)
            }
            apply(snapshot: snapshot)
        }
    }

    func createGroup(displayName: String) async {
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.createGroup(displayName: displayName)
            }
            apply(snapshot: snapshot)
            selectedGroupID = snapshot.groups.first { $0.displayName == displayName }?.id ?? snapshot.groups.first?.id
        }
    }

    func addSelectedContactToSelectedGroup() async {
        guard let selectedGroupID, let selectedContactID else { return }
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.addGroupMember(groupID: selectedGroupID, contactID: selectedContactID)
            }
            apply(snapshot: snapshot)
        }
    }

    func sendGroupMessage(_ body: String) async {
        guard let selectedGroupID else { return }
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.sendGroupMessage(groupID: selectedGroupID, body: body)
            }
            apply(snapshot: snapshot)
        }
    }

    func registerPushToken(_ token: String, platform: String) async {
        do {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.registerPushToken(token, platform: platform)
            }
            apply(snapshot: snapshot)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func receiveMessages() async {
        guard !receiveInFlight else { return }
        receiveInFlight = true
        isReceiving = true
        defer {
            isReceiving = false
            receiveInFlight = false
        }
        do {
            let report = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.receiveMessages()
            }
            apply(snapshot: report.snapshot)
            if notifyOnNewMessages {
                NotificationService.notifyNewMessages(count: report.receivedCount, soundEnabled: playNotificationSound)
            }
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
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
                    self.autoReceiveEnabled && self.isReady && !self.isLoading && !self.isReceiving
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
        Task { [weak self] in
            do {
                let invite = try await SecureChatCoreClient.runInBackground {
                    try SecureChatCoreClient.ownInvite()
                }
                Clipboard.copy(invite.inviteUri)
            } catch {
                if let invite = self?.appSnapshot?.profile?.inviteUri {
                    Clipboard.copy(invite)
                }
            }
        }
    }

    func copyTemporaryInvite() {
        Task {
            if let invite = try? await SecureChatCoreClient.runInBackground({
                try SecureChatCoreClient.temporaryInvite()
            }) {
                Clipboard.copy(invite.inviteUri)
            }
        }
    }

    @discardableResult
    func startTemporaryConnection(inviteURI: String) async -> Bool {
        await runLoading {
            let response = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.startTemporaryConnection(inviteURI: inviteURI)
            }
            apply(snapshot: response.snapshot)
            selectedTemporaryConnectionID = response.connectionId
        }
    }

    func sendTemporaryMessage(_ body: String) async {
        guard let selectedTemporaryConnectionID else { return }
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.sendTemporaryMessage(connectionID: selectedTemporaryConnectionID, body: body)
            }
            apply(snapshot: snapshot)
            self.selectedTemporaryConnectionID = selectedTemporaryConnectionID
        }
    }

    func endTemporaryConnection() async {
        guard let selectedTemporaryConnectionID else { return }
        await runLoading {
            let snapshot = try await SecureChatCoreClient.runInBackground {
                try SecureChatCoreClient.endTemporaryConnection(connectionID: selectedTemporaryConnectionID)
            }
            apply(snapshot: snapshot)
            self.selectedTemporaryConnectionID = nil
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

    private func apply(snapshot: AppSnapshot) {
        appSnapshot = snapshot
        if selectedContactID == nil || !(snapshot.contacts.contains { $0.id == selectedContactID }) {
            selectedContactID = snapshot.contacts.first?.id
        }
        if selectedGroupID == nil || !(snapshot.groups.contains { $0.id == selectedGroupID }) {
            selectedGroupID = snapshot.groups.first?.id
        }
        if let selectedTemporaryConnectionID,
           !(snapshot.temporaryConnections.contains { $0.id == selectedTemporaryConnectionID }) {
            self.selectedTemporaryConnectionID = nil
        }
    }

    @discardableResult
    private func runLoading(_ work: () async throws -> Void) async -> Bool {
        isLoading = true
        defer { isLoading = false }
        let previousError = errorMessage
        do {
            try await work()
            if errorMessage == previousError {
                errorMessage = nil
            }
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
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
