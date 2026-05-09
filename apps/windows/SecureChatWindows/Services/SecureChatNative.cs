namespace SecureChatWindows.Services;

internal static class SecureChatNative
{
    private const string DllName = "secure_chat_ffi";

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_protocol_self_test_json")]
    internal static extern IntPtr ProtocolSelfTestJson();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_demo_state_json")]
    internal static extern IntPtr DemoStateJson();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_relay_smoke_json")]
    internal static extern IntPtr RelaySmokeJson();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_p2p_smoke_json")]
    internal static extern IntPtr P2pSmokeJson();

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_snapshot_json")]
    internal static extern IntPtr AppSnapshotJson(IntPtr dataDir);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_bootstrap_json")]
    internal static extern IntPtr BootstrapJson(IntPtr dataDir, IntPtr displayName, IntPtr relayUrl);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_update_relay_json")]
    internal static extern IntPtr UpdateRelayJson(IntPtr dataDir, IntPtr relayUrl);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_invite_json")]
    internal static extern IntPtr InviteJson(IntPtr dataDir);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_temporary_invite_json")]
    internal static extern IntPtr TemporaryInviteJson(IntPtr dataDir);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_preview_invite_json")]
    internal static extern IntPtr PreviewInviteJson(IntPtr dataDir, IntPtr inviteText);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_start_temporary_connection_json")]
    internal static extern IntPtr StartTemporaryConnectionJson(IntPtr dataDir, IntPtr inviteUri);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_add_contact_json")]
    internal static extern IntPtr AddContactJson(IntPtr dataDir, IntPtr displayName, IntPtr inviteUri);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_update_contact_display_name_json")]
    internal static extern IntPtr UpdateContactDisplayNameJson(IntPtr dataDir, IntPtr contactId, IntPtr displayName);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_delete_contact_json")]
    internal static extern IntPtr DeleteContactJson(IntPtr dataDir, IntPtr contactId);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_send_temporary_message_json")]
    internal static extern IntPtr SendTemporaryMessageJson(IntPtr dataDir, IntPtr connectionId, IntPtr body);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_end_temporary_connection_json")]
    internal static extern IntPtr EndTemporaryConnectionJson(IntPtr dataDir, IntPtr connectionId);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_send_message_json")]
    internal static extern IntPtr SendMessageJson(IntPtr dataDir, IntPtr contactId, IntPtr body);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_send_attachment_json")]
    internal static extern IntPtr SendAttachmentJson(IntPtr dataDir, IntPtr threadKind, IntPtr threadId, IntPtr filePath, IntPtr kind);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_send_burn_message_json")]
    internal static extern IntPtr SendBurnMessageJson(IntPtr dataDir, IntPtr threadKind, IntPtr threadId, IntPtr body);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_open_burn_message_json")]
    internal static extern IntPtr OpenBurnMessageJson(IntPtr dataDir, IntPtr threadKind, IntPtr threadId, IntPtr messageId);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_create_group_json")]
    internal static extern IntPtr CreateGroupJson(IntPtr dataDir, IntPtr displayName);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_add_group_member_json")]
    internal static extern IntPtr AddGroupMemberJson(IntPtr dataDir, IntPtr groupId, IntPtr contactId);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_send_group_message_json")]
    internal static extern IntPtr SendGroupMessageJson(IntPtr dataDir, IntPtr groupId, IntPtr body);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_register_push_token_json")]
    internal static extern IntPtr RegisterPushTokenJson(IntPtr dataDir, IntPtr token, IntPtr platform);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_import_sticker_json")]
    internal static extern IntPtr ImportStickerJson(IntPtr dataDir, IntPtr filePath, IntPtr displayName);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_delete_sticker_json")]
    internal static extern IntPtr DeleteStickerJson(IntPtr dataDir, IntPtr stickerId);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_receive_json")]
    internal static extern IntPtr ReceiveJson(IntPtr dataDir);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_app_p2p_probe_json")]
    internal static extern IntPtr P2pProbeJson(IntPtr dataDir);

    [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint = "secure_chat_free_string")]
    internal static extern void FreeString(IntPtr ptr);
}
