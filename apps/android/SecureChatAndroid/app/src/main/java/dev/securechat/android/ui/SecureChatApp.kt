package dev.securechat.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Timer
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Divider
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.securechat.android.core.AppChatMessage
import dev.securechat.android.core.AppContact
import dev.securechat.android.core.AppGroup
import dev.securechat.android.core.AppGroupMessage
import dev.securechat.android.core.AppMessageDirection
import dev.securechat.android.core.AppMessageStatus
import dev.securechat.android.core.InvitePreview
import dev.securechat.android.core.SecureChatUiState
import dev.securechat.android.core.SecureChatViewModel
import dev.securechat.android.core.TemporaryConnection
import dev.securechat.android.core.TemporaryMessage
import java.text.DateFormat
import java.util.Date

private enum class MainTab {
    Chats,
    Settings,
}

@Composable
fun SecureChatApp(viewModel: SecureChatViewModel) {
    MaterialTheme(
        colorScheme = MaterialTheme.colorScheme.copy(
            primary = Color(0xFF0F766E),
            secondary = Color(0xFF475569),
            tertiary = Color(0xFFB45309),
        ),
    ) {
        Surface(modifier = Modifier.fillMaxSize()) {
            val state = viewModel.uiState
            if (state.snapshot?.ready == true) {
                MainShell(viewModel = viewModel, state = state)
            } else {
                LoginScreen(viewModel = viewModel, state = state)
            }
        }
    }
}

@Composable
private fun LoginScreen(viewModel: SecureChatViewModel, state: SecureChatUiState) {
    var displayName by rememberSaveable { mutableStateOf("") }
    var relayUrl by rememberSaveable { mutableStateOf("https://") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(20.dp)
            .verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("SecureChat", style = MaterialTheme.typography.headlineLarge, fontWeight = FontWeight.Bold)
        Text("End-to-end encrypted relay and P2P chat", color = MaterialTheme.colorScheme.secondary)
        Spacer(Modifier.height(24.dp))
        OutlinedTextField(
            value = displayName,
            onValueChange = { displayName = it },
            label = { Text("Display name") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(12.dp))
        OutlinedTextField(
            value = relayUrl,
            onValueChange = { relayUrl = it },
            label = { Text("Relay URL") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(16.dp))
        Button(
            onClick = { viewModel.bootstrap(displayName, relayUrl) },
            enabled = displayName.isNotBlank() && relayUrl.isNotBlank() && !state.isLoading,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Start")
        }
        StateMessages(state)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MainShell(viewModel: SecureChatViewModel, state: SecureChatUiState) {
    var tab by rememberSaveable { mutableStateOf(MainTab.Chats) }
    var showingInvite by rememberSaveable { mutableStateOf(false) }
    var showingAddContact by rememberSaveable { mutableStateOf(false) }
    var showingCreateGroup by rememberSaveable { mutableStateOf(false) }
    var showingTemporary by rememberSaveable { mutableStateOf(false) }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        state.snapshot?.profile?.displayName ?: "SecureChat",
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                actions = {
                    IconButton(onClick = { viewModel.receiveMessages() }) {
                        Icon(Icons.Filled.Refresh, contentDescription = "Receive")
                    }
                    IconButton(onClick = { showingInvite = true }) {
                        Icon(Icons.Filled.ContentCopy, contentDescription = "Invite")
                    }
                    IconButton(onClick = { showingTemporary = true }) {
                        Icon(Icons.Filled.Timer, contentDescription = "Temporary")
                    }
                    IconButton(onClick = { showingAddContact = true }) {
                        Icon(Icons.Filled.PersonAdd, contentDescription = "Add")
                    }
                    IconButton(onClick = { showingCreateGroup = true }) {
                        Icon(Icons.Filled.PersonAdd, contentDescription = "Group")
                    }
                },
            )
        },
        bottomBar = {
            NavigationBar {
                NavigationBarItem(
                    selected = tab == MainTab.Chats,
                    onClick = { tab = MainTab.Chats },
                    icon = { Icon(Icons.Filled.PersonAdd, contentDescription = null) },
                    label = { Text("Chats") },
                )
                NavigationBarItem(
                    selected = tab == MainTab.Settings,
                    onClick = { tab = MainTab.Settings },
                    icon = { Icon(Icons.Filled.Settings, contentDescription = null) },
                    label = { Text("Settings") },
                )
            }
        },
    ) { innerPadding ->
        Column(
            modifier = Modifier
                .padding(innerPadding)
                .fillMaxSize(),
        ) {
            if (state.isLoading) {
                LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
            }
            when (tab) {
                MainTab.Chats -> ChatWorkbench(viewModel, state)
                MainTab.Settings -> SettingsScreen(viewModel, state)
            }
            StateMessages(state)
        }
    }

    if (showingInvite) {
        InviteDialog(
            inviteUri = state.snapshot?.profile?.inviteUri.orEmpty(),
            onCopyInvite = {
                viewModel.copyOwnInvite()
                showingInvite = false
            },
            onCopyTemporaryInvite = {
                viewModel.copyTemporaryInvite()
                showingInvite = false
            },
            onDismiss = { showingInvite = false },
        )
    }
    if (showingAddContact) {
        AddContactDialog(
            viewModel = viewModel,
            preview = state.invitePreview,
            onDismiss = {
                viewModel.clearInvitePreview()
                showingAddContact = false
            },
        )
    }
    if (showingCreateGroup) {
        CreateGroupDialog(
            viewModel = viewModel,
            contacts = state.snapshot?.contacts.orEmpty(),
            onDismiss = { showingCreateGroup = false },
        )
    }
    if (showingTemporary) {
        TemporaryDialog(
            viewModel = viewModel,
            onDismiss = { showingTemporary = false },
        )
    }
}

@Composable
private fun ChatWorkbench(viewModel: SecureChatViewModel, state: SecureChatUiState) {
    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(12.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            ProfileCard(state)
        }
        item {
            Text("Chats", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        items(state.snapshot?.contacts.orEmpty(), key = { it.id }) { contact ->
            ContactRow(
                contact = contact,
                selected = contact.id == state.selectedContactId,
                onClick = { viewModel.selectContact(contact.id) },
            )
        }
        item {
            Text("Groups", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        items(state.snapshot?.groups.orEmpty(), key = { it.id }) { group ->
            GroupRow(
                group = group,
                selected = group.id == state.selectedGroupId,
                onClick = { viewModel.selectGroup(group.id) },
            )
        }
        item {
            Text("Temporary", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        items(state.snapshot?.temporaryConnections.orEmpty(), key = { it.id }) { connection ->
            TemporaryRow(
                connection = connection,
                selected = connection.id == state.selectedTemporaryConnectionId,
                onClick = { viewModel.selectTemporaryConnection(connection.id) },
            )
        }
        item {
            ChatPanel(viewModel, state)
        }
    }
}

@Composable
private fun ProfileCard(state: SecureChatUiState) {
    val profile = state.snapshot?.profile ?: return
    Card(shape = RoundedCornerShape(8.dp), colors = CardDefaults.cardColors(containerColor = Color(0xFFEFF6FF))) {
        Column(Modifier.padding(12.dp)) {
            Text(profile.displayName, fontWeight = FontWeight.SemiBold)
            Text("Device ${profile.deviceId.shortId()}", style = MaterialTheme.typography.bodySmall)
            Text(profile.relayUrl, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.secondary)
        }
    }
}

@Composable
private fun ContactRow(contact: AppContact, selected: Boolean, onClick: () -> Unit) {
    Card(
        shape = RoundedCornerShape(8.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else MaterialTheme.colorScheme.surfaceVariant
        ),
        modifier = Modifier.clickable(onClick = onClick),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(contact.displayName, fontWeight = FontWeight.SemiBold, maxLines = 1)
                Text(contact.lastMessage ?: "Device ${contact.deviceId.shortId()}", maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            AssistChip(onClick = {}, label = { Text(if (contact.verified) "Verified" else "Safety") })
        }
    }
}

@Composable
private fun TemporaryRow(connection: TemporaryConnection, selected: Boolean, onClick: () -> Unit) {
    Card(
        shape = RoundedCornerShape(8.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (selected) Color(0xFFFFF7ED) else MaterialTheme.colorScheme.surfaceVariant
        ),
        modifier = Modifier.clickable(onClick = onClick),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(connection.displayName, fontWeight = FontWeight.SemiBold, maxLines = 1)
                Text(connection.lastMessage ?: "Device ${connection.deviceId.shortId()}", maxLines = 1)
            }
            AssistChip(onClick = {}, label = { Text(connection.expiresUnix.expiryText()) })
        }
    }
}

@Composable
private fun GroupRow(group: AppGroup, selected: Boolean, onClick: () -> Unit) {
    Card(
        shape = RoundedCornerShape(8.dp),
        colors = CardDefaults.cardColors(
            containerColor = if (selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.12f) else MaterialTheme.colorScheme.surfaceVariant
        ),
        modifier = Modifier.clickable(onClick = onClick),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(group.displayName, fontWeight = FontWeight.SemiBold, maxLines = 1)
                Text(group.lastMessage ?: "${group.memberCount} members", maxLines = 1)
            }
            AssistChip(onClick = {}, label = { Text("MLS") })
        }
    }
}

@Composable
private fun ChatPanel(viewModel: SecureChatViewModel, state: SecureChatUiState) {
    val temporary = viewModel.selectedTemporaryConnection
    val group = viewModel.selectedGroup
    val contact = viewModel.selectedContact
    if (temporary != null) {
        TemporaryChatPanel(viewModel, state, temporary, viewModel.selectedTemporaryMessages)
    } else if (group != null) {
        GroupChatPanel(viewModel, state, group, viewModel.selectedGroupMessages)
    } else if (contact != null) {
        ContactChatPanel(viewModel, state, contact, viewModel.selectedMessages)
    } else {
        Card(shape = RoundedCornerShape(8.dp)) {
            Text("No chat selected", modifier = Modifier.padding(16.dp))
        }
    }
}

@Composable
private fun ContactChatPanel(
    viewModel: SecureChatViewModel,
    state: SecureChatUiState,
    contact: AppContact,
    messages: List<AppChatMessage>,
) {
    ChatContainer(
        title = contact.displayName,
        safetyNumber = contact.safetyNumber,
        temporary = false,
        messages = messages,
        showStatus = state.showMessageStatus,
        showTimestamp = state.showMessageTimestamps,
        onSend = viewModel::sendMessage,
    )
}

@Composable
private fun GroupChatPanel(
    viewModel: SecureChatViewModel,
    state: SecureChatUiState,
    group: AppGroup,
    messages: List<AppGroupMessage>,
) {
    ChatContainer(
        title = group.displayName,
        safetyNumber = "${group.memberCount} MLS members",
        temporary = false,
        messages = messages,
        showStatus = state.showMessageStatus,
        showTimestamp = state.showMessageTimestamps,
        onSend = viewModel::sendGroupMessage,
    )
}

@Composable
private fun TemporaryChatPanel(
    viewModel: SecureChatViewModel,
    state: SecureChatUiState,
    connection: TemporaryConnection,
    messages: List<TemporaryMessage>,
) {
    ChatContainer(
        title = connection.displayName,
        safetyNumber = connection.safetyNumber,
        temporary = true,
        messages = messages,
        showStatus = state.showMessageStatus,
        showTimestamp = state.showMessageTimestamps,
        onSend = viewModel::sendTemporaryMessage,
        onEndTemporary = viewModel::endTemporaryConnection,
    )
}

@Composable
private fun <T> ChatContainer(
    title: String,
    safetyNumber: String,
    temporary: Boolean,
    messages: List<T>,
    showStatus: Boolean,
    showTimestamp: Boolean,
    onSend: (String) -> Unit,
    onEndTemporary: (() -> Unit)? = null,
) {
    var draft by rememberSaveable(title) { mutableStateOf("") }
    Card(shape = RoundedCornerShape(8.dp), modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    Text("Safety $safetyNumber", style = MaterialTheme.typography.bodySmall, maxLines = 1)
                }
                if (temporary && onEndTemporary != null) {
                    OutlinedButton(onClick = onEndTemporary) { Text("End") }
                }
            }
            Divider()
            messages.forEach { message ->
                when (message) {
                    is AppChatMessage -> MessageBubble(
                        direction = message.direction,
                        body = message.body,
                        status = message.status,
                        unix = message.receivedAtUnix ?: message.sentAtUnix,
                        showStatus = showStatus,
                        showTimestamp = showTimestamp,
                    )
                    is TemporaryMessage -> MessageBubble(
                        direction = message.direction,
                        body = message.body,
                        status = message.status,
                        unix = message.receivedAtUnix ?: message.sentAtUnix,
                        showStatus = showStatus,
                        showTimestamp = showTimestamp,
                    )
                    is AppGroupMessage -> MessageBubble(
                        direction = message.direction,
                        body = message.body,
                        status = message.status,
                        unix = message.receivedAtUnix ?: message.sentAtUnix,
                        showStatus = showStatus,
                        showTimestamp = showTimestamp,
                    )
                }
            }
            Row(verticalAlignment = Alignment.Bottom) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = { draft = it },
                    label = { Text("Message") },
                    modifier = Modifier.weight(1f),
                    minLines = 1,
                    maxLines = 4,
                )
                Spacer(Modifier.width(8.dp))
                Button(
                    onClick = {
                        val text = draft
                        draft = ""
                        onSend(text)
                    },
                    enabled = draft.isNotBlank(),
                ) {
                    Text("Send")
                }
            }
        }
    }
}

@Composable
private fun MessageBubble(
    direction: AppMessageDirection,
    body: String,
    status: AppMessageStatus,
    unix: Long,
    showStatus: Boolean,
    showTimestamp: Boolean,
) {
    val outgoing = direction == AppMessageDirection.Outgoing
    Row(Modifier.fillMaxWidth(), horizontalArrangement = if (outgoing) Arrangement.End else Arrangement.Start) {
        Card(
            shape = RoundedCornerShape(8.dp),
            colors = CardDefaults.cardColors(
                containerColor = if (outgoing) MaterialTheme.colorScheme.primary.copy(alpha = 0.15f) else MaterialTheme.colorScheme.surfaceVariant
            ),
            modifier = Modifier.fillMaxWidth(0.82f),
        ) {
            Column(Modifier.padding(10.dp)) {
                Text(body)
                if (showStatus || showTimestamp) {
                    Text(
                        listOfNotNull(
                            status.name.takeIf { showStatus },
                            unix.timeText().takeIf { showTimestamp },
                        ).joinToString("  "),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.secondary,
                    )
                }
            }
        }
    }
}

@Composable
private fun InviteDialog(
    inviteUri: String,
    onCopyInvite: () -> Unit,
    onCopyTemporaryInvite: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("My Invite") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(inviteUri, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                Text("Temporary invites expire quickly and never create a contact.")
            }
        },
        confirmButton = {
            Button(onClick = onCopyInvite) { Text("Copy Invite") }
        },
        dismissButton = {
            TextButton(onClick = onCopyTemporaryInvite) { Text("Copy Temporary") }
        },
    )
}

@Composable
private fun AddContactDialog(
    viewModel: SecureChatViewModel,
    preview: InvitePreview?,
    onDismiss: () -> Unit,
) {
    var inviteText by rememberSaveable { mutableStateOf("") }
    var displayName by rememberSaveable { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add Contact") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = inviteText,
                    onValueChange = { inviteText = it },
                    label = { Text("Invite link") },
                    minLines = 3,
                    maxLines = 5,
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { inviteText = viewModel.pasteFromClipboard() }) {
                        Text("Paste")
                    }
                    OutlinedButton(onClick = { viewModel.previewInvite(inviteText) }, enabled = inviteText.isNotBlank()) {
                        Text("Check")
                    }
                }
                OutlinedTextField(
                    value = displayName,
                    onValueChange = { displayName = it },
                    label = { Text("Name") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                if (preview != null) {
                    InvitePreviewCard(preview)
                    LaunchedEffect(preview.normalizedInviteUri) {
                        if (displayName.isBlank()) displayName = preview.suggestedDisplayName
                    }
                }
            }
        },
        confirmButton = {
            if (preview?.temporary == true) {
                Button(onClick = { viewModel.startTemporaryConnection(preview.normalizedInviteUri); onDismiss() }) {
                    Text("Start Temporary")
                }
            } else {
                Button(
                    onClick = {
                        val uri = preview?.normalizedInviteUri ?: inviteText
                        viewModel.addContact(displayName, uri)
                        onDismiss()
                    },
                    enabled = preview != null,
                ) {
                    Text(if (preview?.alreadyAdded == true) "Update" else "Add")
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

@Composable
private fun CreateGroupDialog(
    viewModel: SecureChatViewModel,
    contacts: List<AppContact>,
    onDismiss: () -> Unit,
) {
    var name by rememberSaveable { mutableStateOf("") }
    var firstContactId by rememberSaveable { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Create Group") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Group name") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                contacts.forEach { contact ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { firstContactId = contact.id }
                            .padding(vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(contact.displayName, modifier = Modifier.weight(1f))
                        AssistChip(
                            onClick = { firstContactId = contact.id },
                            label = { Text(if (firstContactId == contact.id) "Selected" else "Add") },
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    viewModel.createGroup(name, firstContactId.ifBlank { null })
                    onDismiss()
                },
                enabled = name.isNotBlank(),
            ) { Text("Create") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun TemporaryDialog(viewModel: SecureChatViewModel, onDismiss: () -> Unit) {
    var inviteText by rememberSaveable { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Temporary Chat") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = inviteText,
                    onValueChange = { inviteText = it },
                    label = { Text("Temporary invite link") },
                    minLines = 3,
                    maxLines = 5,
                    modifier = Modifier.fillMaxWidth(),
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = { inviteText = viewModel.pasteFromClipboard() }) { Text("Paste") }
                    OutlinedButton(onClick = { viewModel.copyTemporaryInvite(); onDismiss() }) { Text("Copy Mine") }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    viewModel.startTemporaryConnection(inviteText)
                    onDismiss()
                },
                enabled = inviteText.isNotBlank(),
            ) { Text("Start") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun InvitePreviewCard(preview: InvitePreview) {
    Card(shape = RoundedCornerShape(8.dp), colors = CardDefaults.cardColors(containerColor = Color(0xFFF8FAFC))) {
        Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (preview.temporary) "Temporary invite" else "Invite valid", fontWeight = FontWeight.SemiBold)
            Text("Name: ${preview.suggestedDisplayName}")
            Text("Device: ${preview.deviceId.shortId()}")
            Text("Safety: ${preview.safetyNumber}", fontFamily = FontFamily.Monospace, maxLines = 2)
        }
    }
}

@Composable
private fun SettingsScreen(viewModel: SecureChatViewModel, state: SecureChatUiState) {
    var relayUrl by rememberSaveable(state.snapshot?.profile?.relayUrl) {
        mutableStateOf(state.snapshot?.profile?.relayUrl.orEmpty())
    }
    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Settings", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
        OutlinedTextField(
            value = relayUrl,
            onValueChange = { relayUrl = it },
            label = { Text("Relay URL") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(onClick = { viewModel.updateRelay(relayUrl) }, enabled = relayUrl.isNotBlank()) {
            Text("Update Relay")
        }
        SettingSwitch("Auto receive", state.autoReceiveEnabled, viewModel::setAutoReceiveEnabled)
        Text("Polling interval: ${state.receivePollIntervalSeconds}s")
        Slider(
            value = state.receivePollIntervalSeconds.toFloat(),
            onValueChange = { viewModel.setReceivePollIntervalSeconds(it.toInt()) },
            valueRange = 5f..300f,
        )
        SettingSwitch("Notify on new messages", state.notifyOnNewMessages, viewModel::setNotifyOnNewMessages)
        SettingSwitch("Play notification sound", state.playNotificationSound, viewModel::setPlayNotificationSound)
        SettingSwitch("Show timestamps", state.showMessageTimestamps, viewModel::setShowMessageTimestamps)
        SettingSwitch("Show message status", state.showMessageStatus, viewModel::setShowMessageStatus)
        SettingSwitch("Prefer QUIC", state.preferQuic, viewModel::setPreferQuic)
        SettingSwitch("Cover traffic padding", state.coverTrafficEnabled, viewModel::setCoverTrafficEnabled)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = viewModel::runSelfTest) { Text("Self Test") }
            OutlinedButton(onClick = viewModel::runRelaySmoke) { Text("Relay Smoke") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedButton(onClick = viewModel::runP2pProbe) { Text("P2P Probe") }
            OutlinedButton(onClick = viewModel::resetPreferences) { Text("Reset") }
        }
        state.diagnosticText?.let {
            Card(shape = RoundedCornerShape(8.dp)) {
                Text(it, modifier = Modifier.padding(12.dp), fontFamily = FontFamily.Monospace)
            }
        }
    }
}

@Composable
private fun SettingSwitch(label: String, checked: Boolean, onCheckedChange: (Boolean) -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label)
        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

@Composable
private fun StateMessages(state: SecureChatUiState) {
    state.errorMessage?.let {
        Text(
            it,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.padding(12.dp),
            style = MaterialTheme.typography.bodySmall,
        )
    }
}

private fun String.shortId(): String = take(8)

private fun Long.expiryText(): String {
    val remaining = this - System.currentTimeMillis() / 1000
    if (remaining <= 0) return "Expired"
    val minutes = (remaining / 60).coerceAtLeast(1)
    return if (minutes < 60) "${minutes}m" else "${minutes / 60}h"
}

private fun Long.timeText(): String =
    DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(this * 1000))
