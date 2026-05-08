package dev.securechat.android.core

import android.app.Application
import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

data class SecureChatUiState(
    val snapshot: AppSnapshot? = null,
    val selectedContactId: String? = null,
    val selectedTemporaryConnectionId: String? = null,
    val isLoading: Boolean = false,
    val errorMessage: String? = null,
    val invitePreview: InvitePreview? = null,
    val diagnosticText: String? = null,
    val autoReceiveEnabled: Boolean = true,
    val receivePollIntervalSeconds: Int = 5,
    val notifyOnNewMessages: Boolean = true,
    val playNotificationSound: Boolean = true,
    val showMessageTimestamps: Boolean = false,
    val showMessageStatus: Boolean = true,
    val preferQuic: Boolean = true,
    val coverTrafficEnabled: Boolean = true,
)

class SecureChatViewModel(application: Application) : AndroidViewModel(application) {
    private val client = SecureChatCoreClient(application)
    private val clipboard = SecureClipboard(application)
    private val preferences = application.getSharedPreferences("secure-chat-settings", Context.MODE_PRIVATE)

    var uiState by mutableStateOf(loadInitialState())
        private set

    init {
        loadAppSnapshot()
        startAutoReceiveLoop()
    }

    val selectedContact: AppContact?
        get() = uiState.snapshot?.contacts?.firstOrNull { it.id == uiState.selectedContactId }

    val selectedMessages: List<AppChatMessage>
        get() = uiState.snapshot?.messages?.filter { it.contactId == uiState.selectedContactId }.orEmpty()

    val selectedTemporaryConnection: TemporaryConnection?
        get() = uiState.snapshot?.temporaryConnections?.firstOrNull { it.id == uiState.selectedTemporaryConnectionId }

    val selectedTemporaryMessages: List<TemporaryMessage>
        get() = uiState.snapshot?.temporaryMessages
            ?.filter { it.connectionId == uiState.selectedTemporaryConnectionId }
            .orEmpty()

    fun loadAppSnapshot() {
        runCore { applySnapshot(callCore { client.appSnapshot() }) }
    }

    fun bootstrap(displayName: String, relayUrl: String) {
        runCore { applySnapshot(callCore { client.bootstrap(displayName.trim(), relayUrl.trim()) }) }
    }

    fun updateRelay(relayUrl: String) {
        runCore { applySnapshot(callCore { client.updateRelay(relayUrl.trim()) }) }
    }

    fun selectContact(id: String) {
        uiState = uiState.copy(selectedContactId = id, selectedTemporaryConnectionId = null)
    }

    fun selectTemporaryConnection(id: String) {
        uiState = uiState.copy(selectedContactId = null, selectedTemporaryConnectionId = id)
    }

    fun copyOwnInvite() {
        runCore {
            val invite = callCore { client.ownInvite().inviteUri }
            clipboard.copy("SecureChat invite", invite)
        }
    }

    fun copyTemporaryInvite() {
        runCore {
            val invite = callCore { client.temporaryInvite().inviteUri }
            clipboard.copy("SecureChat temporary invite", invite)
        }
    }

    fun pasteFromClipboard(): String = clipboard.read()

    fun previewInvite(inviteText: String) {
        runCore { uiState = uiState.copy(invitePreview = callCore { client.previewInvite(inviteText.trim()) }) }
    }

    fun clearInvitePreview() {
        uiState = uiState.copy(invitePreview = null)
    }

    fun addContact(displayName: String, inviteUri: String) {
        runCore {
            applySnapshot(callCore { client.addContact(displayName.trim(), inviteUri.trim()) })
            clearInvitePreview()
        }
    }

    fun startTemporaryConnection(inviteUri: String) {
        runCore {
            val response = callCore { client.startTemporaryConnection(inviteUri.trim()) }
            applySnapshot(response.snapshot)
            uiState = uiState.copy(selectedContactId = null, selectedTemporaryConnectionId = response.connectionId)
            clearInvitePreview()
        }
    }

    fun sendMessage(body: String) {
        val contactId = uiState.selectedContactId ?: return
        val text = body.trim()
        if (text.isEmpty()) return
        runCore {
            applySnapshot(callCore { client.sendMessage(contactId, text) })
            uiState = uiState.copy(selectedContactId = contactId)
        }
    }

    fun sendTemporaryMessage(body: String) {
        val connectionId = uiState.selectedTemporaryConnectionId ?: return
        val text = body.trim()
        if (text.isEmpty()) return
        runCore {
            applySnapshot(callCore { client.sendTemporaryMessage(connectionId, text) })
            uiState = uiState.copy(selectedContactId = null, selectedTemporaryConnectionId = connectionId)
        }
    }

    fun endTemporaryConnection() {
        val connectionId = uiState.selectedTemporaryConnectionId ?: return
        runCore {
            applySnapshot(callCore { client.endTemporaryConnection(connectionId) })
            uiState = uiState.copy(selectedTemporaryConnectionId = null)
        }
    }

    fun receiveMessages() {
        runCore {
            val report = callCore { client.receiveMessages() }
            applySnapshot(report.snapshot)
        }
    }

    fun runSelfTest() {
        runCore {
            val result = callCore { client.runSelfTest() }
            uiState = uiState.copy(
                diagnosticText = "Protocol self-test: ${if (result.ok) "OK" else "FAILED"}\n" +
                    result.checks.joinToString("\n") { "${if (it.passed) "OK" else "FAIL"} ${it.name}: ${it.detail}" }
            )
        }
    }

    fun runRelaySmoke() {
        runCore {
            val result = callCore { client.runRelaySmoke() }
            uiState = uiState.copy(
                diagnosticText = "Relay smoke: ${if (result.ok) "OK" else "FAILED"}\n" +
                    "Relay: ${result.relay}\nRelay plaintext storage: ${result.relayHealth.storesPlaintext}"
            )
        }
    }

    fun runP2pProbe() {
        runCore {
            val result = callCore { client.runP2pProbe() }
            uiState = uiState.copy(
                diagnosticText = "P2P probe: ${if (result.ok) "OK" else "FAILED"}\n" +
                    "Local: ${result.localAddr}\nPublic: ${result.publicCandidate.addr}"
            )
        }
    }

    fun setAutoReceiveEnabled(value: Boolean) = updatePreference("autoReceiveEnabled", value) {
        copy(autoReceiveEnabled = value)
    }

    fun setReceivePollIntervalSeconds(value: Int) {
        val clamped = value.coerceIn(5, 300)
        preferences.edit().putInt("receivePollIntervalSeconds", clamped).apply()
        uiState = uiState.copy(receivePollIntervalSeconds = clamped)
    }

    fun setNotifyOnNewMessages(value: Boolean) = updatePreference("notifyOnNewMessages", value) {
        copy(notifyOnNewMessages = value)
    }

    fun setPlayNotificationSound(value: Boolean) = updatePreference("playNotificationSound", value) {
        copy(playNotificationSound = value)
    }

    fun setShowMessageTimestamps(value: Boolean) = updatePreference("showMessageTimestamps", value) {
        copy(showMessageTimestamps = value)
    }

    fun setShowMessageStatus(value: Boolean) = updatePreference("showMessageStatus", value) {
        copy(showMessageStatus = value)
    }

    fun setPreferQuic(value: Boolean) = updatePreference("preferQuic", value) {
        copy(preferQuic = value)
    }

    fun setCoverTrafficEnabled(value: Boolean) = updatePreference("coverTrafficEnabled", value) {
        copy(coverTrafficEnabled = value)
    }

    fun resetPreferences() {
        preferences.edit().clear().apply()
        uiState = uiState.copy(
            autoReceiveEnabled = true,
            receivePollIntervalSeconds = 5,
            notifyOnNewMessages = true,
            playNotificationSound = true,
            showMessageTimestamps = false,
            showMessageStatus = true,
            preferQuic = true,
            coverTrafficEnabled = true,
        )
    }

    private fun runCore(work: suspend () -> Unit) {
        viewModelScope.launch {
            uiState = uiState.copy(isLoading = true)
            try {
                work()
                uiState = uiState.copy(errorMessage = null)
            } catch (error: Throwable) {
                uiState = uiState.copy(errorMessage = error.message ?: error.toString())
            } finally {
                uiState = uiState.copy(isLoading = false)
            }
        }
    }

    private suspend fun <T> callCore(work: () -> T): T =
        withContext(Dispatchers.IO) { work() }

    private fun applySnapshot(snapshot: AppSnapshot) {
        val selectedContactId = uiState.selectedContactId
            ?.takeIf { id -> snapshot.contacts.any { it.id == id } }
            ?: snapshot.contacts.firstOrNull()?.id
        val selectedTemporaryConnectionId = uiState.selectedTemporaryConnectionId
            ?.takeIf { id -> snapshot.temporaryConnections.any { it.id == id } }
        uiState = uiState.copy(
            snapshot = snapshot,
            selectedContactId = selectedContactId.takeIf { selectedTemporaryConnectionId == null },
            selectedTemporaryConnectionId = selectedTemporaryConnectionId,
        )
    }

    private fun startAutoReceiveLoop() {
        viewModelScope.launch {
            while (true) {
                delay(uiState.receivePollIntervalSeconds.coerceIn(5, 300) * 1_000L)
                if (uiState.autoReceiveEnabled && uiState.snapshot?.ready == true && !uiState.isLoading) {
                    receiveMessages()
                }
            }
        }
    }

    private fun loadInitialState(): SecureChatUiState {
        return SecureChatUiState(
            autoReceiveEnabled = preferences.getBoolean("autoReceiveEnabled", true),
            receivePollIntervalSeconds = preferences.getInt("receivePollIntervalSeconds", 5).coerceIn(5, 300),
            notifyOnNewMessages = preferences.getBoolean("notifyOnNewMessages", true),
            playNotificationSound = preferences.getBoolean("playNotificationSound", true),
            showMessageTimestamps = preferences.getBoolean("showMessageTimestamps", false),
            showMessageStatus = preferences.getBoolean("showMessageStatus", true),
            preferQuic = preferences.getBoolean("preferQuic", true),
            coverTrafficEnabled = preferences.getBoolean("coverTrafficEnabled", true),
        )
    }

    private fun updatePreference(
        key: String,
        value: Boolean,
        reducer: SecureChatUiState.() -> SecureChatUiState,
    ) {
        preferences.edit().putBoolean(key, value).apply()
        uiState = uiState.reducer()
    }
}
