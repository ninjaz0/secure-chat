using SecureChatWindows.Models;

namespace SecureChatWindows.Services;

public sealed class SecureChatCoreClient
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        Converters =
        {
            new JsonStringEnumConverter(JsonNamingPolicy.SnakeCaseLower),
        },
    };

    public string DataDirectory { get; }

    public SecureChatCoreClient()
    {
        var local = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        DataDirectory = Path.Combine(local, "SecureChat");
        Directory.CreateDirectory(DataDirectory);
    }

    public Task<SelfTestResult> ProtocolSelfTestAsync() =>
        Task.Run(() => Decode<SelfTestResult>(Call(() => SecureChatNative.ProtocolSelfTestJson())));

    public Task<JsonDocument> DemoStateAsync() =>
        Task.Run(() => DecodeJsonDocument(Call(() => SecureChatNative.DemoStateJson())));

    public Task<JsonDocument> RelaySmokeAsync() =>
        Task.Run(() => DecodeJsonDocument(Call(() => SecureChatNative.RelaySmokeJson())));

    public Task<JsonDocument> P2pSmokeAsync() =>
        Task.Run(() => DecodeJsonDocument(Call(() => SecureChatNative.P2pSmokeJson())));

    public Task<AppSnapshot> AppSnapshotAsync() =>
        Task.Run(() => With1(DataDirectory, dataDir => Decode<AppSnapshot>(Call(() => SecureChatNative.AppSnapshotJson(dataDir)))));

    public Task<AppSnapshot> BootstrapAsync(string displayName, string relayUrl) =>
        Task.Run(() => With3(DataDirectory, displayName, relayUrl,
            (dataDir, name, relay) => Decode<AppSnapshot>(Call(() => SecureChatNative.BootstrapJson(dataDir, name, relay)))));

    public Task<AppSnapshot> UpdateRelayAsync(string relayUrl) =>
        Task.Run(() => With2(DataDirectory, relayUrl,
            (dataDir, relay) => Decode<AppSnapshot>(Call(() => SecureChatNative.UpdateRelayJson(dataDir, relay)))));

    public Task<InviteResponse> InviteAsync() =>
        Task.Run(() => With1(DataDirectory, dataDir => Decode<InviteResponse>(Call(() => SecureChatNative.InviteJson(dataDir)))));

    public Task<TemporaryInviteResponse> TemporaryInviteAsync() =>
        Task.Run(() => With1(DataDirectory, dataDir => Decode<TemporaryInviteResponse>(Call(() => SecureChatNative.TemporaryInviteJson(dataDir)))));

    public Task<InvitePreview> PreviewInviteAsync(string inviteText) =>
        Task.Run(() => With2(DataDirectory, inviteText,
            (dataDir, invite) => Decode<InvitePreview>(Call(() => SecureChatNative.PreviewInviteJson(dataDir, invite)))));

    public Task<TemporaryStartResponse> StartTemporaryConnectionAsync(string inviteUri) =>
        Task.Run(() => With2(DataDirectory, inviteUri,
            (dataDir, invite) => Decode<TemporaryStartResponse>(Call(() => SecureChatNative.StartTemporaryConnectionJson(dataDir, invite)))));

    public Task<AppSnapshot> AddContactAsync(string displayName, string inviteUri) =>
        Task.Run(() => With3(DataDirectory, displayName, inviteUri,
            (dataDir, name, invite) => Decode<AppSnapshot>(Call(() => SecureChatNative.AddContactJson(dataDir, name, invite)))));

    public Task<AppSnapshot> UpdateContactDisplayNameAsync(string contactId, string displayName) =>
        Task.Run(() => With3(DataDirectory, contactId, displayName,
            (dataDir, contact, name) => Decode<AppSnapshot>(Call(() => SecureChatNative.UpdateContactDisplayNameJson(dataDir, contact, name)))));

    public Task<AppSnapshot> DeleteContactAsync(string contactId) =>
        Task.Run(() => With2(DataDirectory, contactId,
            (dataDir, contact) => Decode<AppSnapshot>(Call(() => SecureChatNative.DeleteContactJson(dataDir, contact)))));

    public Task<AppSnapshot> SendTemporaryMessageAsync(string connectionId, string body) =>
        Task.Run(() => With3(DataDirectory, connectionId, body,
            (dataDir, connection, text) => Decode<AppSnapshot>(Call(() => SecureChatNative.SendTemporaryMessageJson(dataDir, connection, text)))));

    public Task<AppSnapshot> EndTemporaryConnectionAsync(string connectionId) =>
        Task.Run(() => With2(DataDirectory, connectionId,
            (dataDir, connection) => Decode<AppSnapshot>(Call(() => SecureChatNative.EndTemporaryConnectionJson(dataDir, connection)))));

    public Task<AppSnapshot> SendMessageAsync(string contactId, string body) =>
        Task.Run(() => With3(DataDirectory, contactId, body,
            (dataDir, contact, text) => Decode<AppSnapshot>(Call(() => SecureChatNative.SendMessageJson(dataDir, contact, text)))));

    public Task<SendAttachmentResponse> SendAttachmentAsync(string threadKind, string threadId, string filePath, string kind) =>
        Task.Run(() => With5(DataDirectory, threadKind, threadId, filePath, kind,
            (dataDir, tKind, tId, path, attachmentKind) =>
                Decode<SendAttachmentResponse>(Call(() => SecureChatNative.SendAttachmentJson(dataDir, tKind, tId, path, attachmentKind)))));

    public Task<AppSnapshot> SendBurnMessageAsync(string threadKind, string threadId, string body) =>
        Task.Run(() => With4(DataDirectory, threadKind, threadId, body,
            (dataDir, tKind, tId, text) => Decode<AppSnapshot>(Call(() => SecureChatNative.SendBurnMessageJson(dataDir, tKind, tId, text)))));

    public Task<AppSnapshot> OpenBurnMessageAsync(string threadKind, string threadId, string messageId) =>
        Task.Run(() => With4(DataDirectory, threadKind, threadId, messageId,
            (dataDir, tKind, tId, message) => Decode<AppSnapshot>(Call(() => SecureChatNative.OpenBurnMessageJson(dataDir, tKind, tId, message)))));

    public Task<AppSnapshot> CreateGroupAsync(string displayName) =>
        Task.Run(() => With2(DataDirectory, displayName,
            (dataDir, name) => Decode<AppSnapshot>(Call(() => SecureChatNative.CreateGroupJson(dataDir, name)))));

    public Task<AppSnapshot> AddGroupMemberAsync(string groupId, string contactId) =>
        Task.Run(() => With3(DataDirectory, groupId, contactId,
            (dataDir, group, contact) => Decode<AppSnapshot>(Call(() => SecureChatNative.AddGroupMemberJson(dataDir, group, contact)))));

    public Task<AppSnapshot> SendGroupMessageAsync(string groupId, string body) =>
        Task.Run(() => With3(DataDirectory, groupId, body,
            (dataDir, group, text) => Decode<AppSnapshot>(Call(() => SecureChatNative.SendGroupMessageJson(dataDir, group, text)))));

    public Task<AppSnapshot> RegisterPushTokenAsync(string token, string platform = "ios") =>
        Task.Run(() => With3(DataDirectory, token, platform,
            (dataDir, pushToken, pushPlatform) => Decode<AppSnapshot>(Call(() => SecureChatNative.RegisterPushTokenJson(dataDir, pushToken, pushPlatform)))));

    public Task<ImportStickerResponse> ImportStickerAsync(string filePath, string displayName) =>
        Task.Run(() => With3(DataDirectory, filePath, displayName,
            (dataDir, path, name) => Decode<ImportStickerResponse>(Call(() => SecureChatNative.ImportStickerJson(dataDir, path, name)))));

    public Task<AppSnapshot> DeleteStickerAsync(string stickerId) =>
        Task.Run(() => With2(DataDirectory, stickerId,
            (dataDir, sticker) => Decode<AppSnapshot>(Call(() => SecureChatNative.DeleteStickerJson(dataDir, sticker)))));

    public Task<ReceiveReport> ReceiveAsync() =>
        Task.Run(() => With1(DataDirectory, dataDir => Decode<ReceiveReport>(Call(() => SecureChatNative.ReceiveJson(dataDir)))));

    public Task<JsonDocument> P2pProbeAsync() =>
        Task.Run(() => With1(DataDirectory, dataDir => DecodeJsonDocument(Call(() => SecureChatNative.P2pProbeJson(dataDir)))));

    private static T Decode<T>(string payload)
    {
        var maybeError = JsonSerializer.Deserialize<FfiErrorPayload>(payload, JsonOptions);
        if (maybeError?.Ok == false)
        {
            throw new InvalidOperationException(maybeError.Error ?? "SecureChat core returned an error.");
        }

        return JsonSerializer.Deserialize<T>(payload, JsonOptions)
            ?? throw new InvalidOperationException("SecureChat core returned an empty payload.");
    }

    private static JsonDocument DecodeJsonDocument(string payload)
    {
        var maybeError = JsonSerializer.Deserialize<FfiErrorPayload>(payload, JsonOptions);
        if (maybeError?.Ok == false)
        {
            throw new InvalidOperationException(maybeError.Error ?? "SecureChat core returned an error.");
        }

        return JsonDocument.Parse(payload);
    }

    private static string Call(Func<IntPtr> nativeCall)
    {
        var result = nativeCall();
        if (result == IntPtr.Zero)
        {
            throw new InvalidOperationException("SecureChat core returned a null response.");
        }

        try
        {
            return Marshal.PtrToStringUTF8(result) ?? "";
        }
        finally
        {
            SecureChatNative.FreeString(result);
        }
    }

    private static IntPtr ToUtf8(string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value + "\0");
        var ptr = Marshal.AllocHGlobal(bytes.Length);
        Marshal.Copy(bytes, 0, ptr, bytes.Length);
        return ptr;
    }

    private static void FreeUtf8(IntPtr ptr)
    {
        if (ptr != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(ptr);
        }
    }

    private static T With1<T>(string a, Func<IntPtr, T> action)
    {
        var pa = ToUtf8(a);
        try { return action(pa); }
        finally { FreeUtf8(pa); }
    }

    private static T With2<T>(string a, string b, Func<IntPtr, IntPtr, T> action)
    {
        var pa = ToUtf8(a);
        var pb = ToUtf8(b);
        try { return action(pa, pb); }
        finally { FreeUtf8(pa); FreeUtf8(pb); }
    }

    private static T With3<T>(string a, string b, string c, Func<IntPtr, IntPtr, IntPtr, T> action)
    {
        var pa = ToUtf8(a);
        var pb = ToUtf8(b);
        var pc = ToUtf8(c);
        try { return action(pa, pb, pc); }
        finally { FreeUtf8(pa); FreeUtf8(pb); FreeUtf8(pc); }
    }

    private static T With4<T>(string a, string b, string c, string d, Func<IntPtr, IntPtr, IntPtr, IntPtr, T> action)
    {
        var pa = ToUtf8(a);
        var pb = ToUtf8(b);
        var pc = ToUtf8(c);
        var pd = ToUtf8(d);
        try { return action(pa, pb, pc, pd); }
        finally { FreeUtf8(pa); FreeUtf8(pb); FreeUtf8(pc); FreeUtf8(pd); }
    }

    private static T With5<T>(string a, string b, string c, string d, string e, Func<IntPtr, IntPtr, IntPtr, IntPtr, IntPtr, T> action)
    {
        var pa = ToUtf8(a);
        var pb = ToUtf8(b);
        var pc = ToUtf8(c);
        var pd = ToUtf8(d);
        var pe = ToUtf8(e);
        try { return action(pa, pb, pc, pd, pe); }
        finally { FreeUtf8(pa); FreeUtf8(pb); FreeUtf8(pc); FreeUtf8(pd); FreeUtf8(pe); }
    }
}
