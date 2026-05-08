import SwiftUI

struct AddContactView: View {
    @EnvironmentObject private var store: SecureChatStore
    @Environment(\.dismiss) private var dismiss
    @State private var displayName = ""
    @State private var inviteText = ""
    @State private var invitePreview: InvitePreview?
    @State private var previewError: String?
    @State private var isCheckingInvite = false
    @State private var nameWasEdited = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Invite") {
                    TextField("Paste schat://invite/... link", text: $inviteText, axis: .vertical)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .lineLimit(4...8)

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
                        Label("Check Invite", systemImage: "checkmark.shield")
                    }
                    .disabled(inviteText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }

                if isCheckingInvite {
                    Section {
                        ProgressView()
                    }
                } else if let invitePreview {
                    Section("Preview") {
                        InvitePreviewRows(preview: invitePreview)
                    }
                } else if let previewError {
                    Section {
                        Label(previewError, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.red)
                    }
                }

                Section("Name") {
                    TextField("Name (optional)", text: Binding(
                        get: { displayName },
                        set: { value in
                            displayName = value
                            nameWasEdited = true
                        }
                    ))
                }

                Section {
                    Button {
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
                    } label: {
                        Label(
                            invitePreview?.alreadyAdded == true ? "Update Contact" : "Add Contact",
                            systemImage: "person.badge.plus"
                        )
                    }
                    .disabled(invitePreview == nil || invitePreview?.temporary == true || isCheckingInvite)

                    if invitePreview?.temporary == true {
                        Button {
                            Task {
                                if invitePreview == nil {
                                    await refreshInvitePreview(inviteText)
                                }
                                guard let invitePreview else { return }
                                let didStart = await store.startTemporaryConnection(
                                    inviteURI: invitePreview.normalizedInviteUri
                                )
                                if didStart {
                                    dismiss()
                                }
                            }
                        } label: {
                            Label("Start Temporary Chat", systemImage: "timer")
                        }
                        .disabled(isCheckingInvite)
                    }
                }
            }
            .navigationTitle("Add Contact")
            .navigationBarTitleDisplayMode(.inline)
            .task(id: inviteText) {
                await refreshInvitePreviewWhenReady(inviteText)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Cancel") { dismiss() }
                }
            }
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

private struct InvitePreviewRows: View {
    let preview: InvitePreview

    var body: some View {
        Label(
            preview.temporary ? "Temporary invite" : (preview.alreadyAdded ? "Already in contacts" : "Invite valid"),
            systemImage: preview.temporary ? "timer" : (preview.alreadyAdded ? "person.crop.circle.badge.checkmark" : "checkmark.shield.fill")
        )
        .foregroundStyle(preview.temporary ? Color.orange : (preview.alreadyAdded ? Color.secondary : Color.green))

        LabeledContent("Name", value: preview.suggestedDisplayName)
        LabeledContent("Device", value: shortDevice(preview.deviceId))

        if let relayHint = preview.relayHint, !relayHint.isEmpty {
            LabeledContent("Relay", value: relayHint)
        }

        VStack(alignment: .leading, spacing: 4) {
            Text("Safety Number")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(preview.safetyNumber)
                .font(.caption.monospaced())
                .textSelection(.enabled)
        }
    }
}
