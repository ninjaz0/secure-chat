using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using SecureChatWindows.Models;
using SecureChatWindows.Services;

namespace SecureChatWindows.ViewModels;

public sealed class ConversationItem
{
    public string ThreadKind { get; init; } = "contact";
    public string ThreadId { get; init; } = "";
    public string Title { get; init; } = "";
    public string Subtitle { get; init; } = "";
    public string SafetyNumber { get; init; } = "";
    public bool IsTemporary => ThreadKind == "temporary";
    public override string ToString() => Title;
}

public sealed class MessageItem
{
    public string Id { get; init; } = "";
    public string ThreadKind { get; init; } = "";
    public string ThreadId { get; init; } = "";
    public string SenderLine { get; init; } = "";
    public string DisplayText { get; init; } = "";
    public string AttachmentSummary { get; init; } = "";
    public string? LocalPath { get; init; }
    public ImageSource? PreviewImage { get; init; }
    public string StatusLine { get; init; } = "";
    public bool IsOutgoing { get; init; }
    public bool IsBurnMessage { get; init; }
    public Visibility BurnButtonVisibility => IsBurnMessage ? Visibility.Visible : Visibility.Collapsed;
    public Visibility PreviewImageVisibility => PreviewImage is null ? Visibility.Collapsed : Visibility.Visible;
    public Visibility OpenAttachmentVisibility => string.IsNullOrWhiteSpace(LocalPath) ? Visibility.Collapsed : Visibility.Visible;
}

public sealed class MainViewModel : ObservableObject
{
    private readonly SecureChatCoreClient client = new();
    private AppSnapshot snapshot = new();
    private ConversationItem? selectedConversation;
    private bool isBusy;
    private string statusText = "Ready";
    private string relayUrl = "http://127.0.0.1:8787";
    private string displayName = "";
    private bool notifyOnNewMessages = true;
    private int lastTotalMessageCount;
    private string visibleMessageSignature = "";

    public ObservableCollection<ConversationItem> Conversations { get; } = new();
    public ObservableCollection<MessageItem> VisibleMessages { get; } = new();
    public ObservableCollection<StickerItem> Stickers { get; } = new();
    public ObservableCollection<AttachmentTransfer> AttachmentTransfers { get; } = new();

    public AppSnapshot Snapshot
    {
        get => snapshot;
        private set
        {
            if (SetProperty(ref snapshot, value))
            {
                OnPropertyChanged(nameof(IsReady));
                OnPropertyChanged(nameof(ProfileLine));
                OnPropertyChanged(nameof(InviteUri));
                OnPropertyChanged(nameof(SelectedSafetyNumber));
            }
        }
    }

    public ConversationItem? SelectedConversation
    {
        get => selectedConversation;
        set
        {
            if (SetProperty(ref selectedConversation, value))
            {
                RebuildVisibleMessages();
                OnPropertyChanged(nameof(HasSelectedConversation));
                OnPropertyChanged(nameof(SelectedTitle));
                OnPropertyChanged(nameof(SelectedSafetyNumber));
            }
        }
    }

    public bool IsReady => Snapshot.Ready;
    public bool HasSelectedConversation => SelectedConversation is not null;
    public string SelectedTitle => SelectedConversation?.Title ?? "No conversation";
    public string SelectedSafetyNumber => SelectedConversation?.SafetyNumber ?? "";
    public string InviteUri => Snapshot.Profile?.InviteUri ?? "";
    public string ProfileLine => Snapshot.Profile is null
        ? "No local identity yet"
        : $"{Snapshot.Profile.DisplayName} · {ShortId(Snapshot.Profile.DeviceId)}";

    public bool IsBusy
    {
        get => isBusy;
        private set => SetProperty(ref isBusy, value);
    }

    public string StatusText
    {
        get => statusText;
        private set => SetProperty(ref statusText, value);
    }

    public string RelayUrl
    {
        get => relayUrl;
        set => SetProperty(ref relayUrl, value);
    }

    public string DisplayName
    {
        get => displayName;
        set => SetProperty(ref displayName, value);
    }

    public bool NotifyOnNewMessages
    {
        get => notifyOnNewMessages;
        set => SetProperty(ref notifyOnNewMessages, value);
    }

    public async Task InitializeAsync()
    {
        await RunAsync("Loading", async () =>
        {
            ApplySnapshot(await client.AppSnapshotAsync());
            if (Snapshot.Profile is not null)
            {
                RelayUrl = Snapshot.Profile.RelayUrl;
                DisplayName = Snapshot.Profile.DisplayName;
            }
        });
    }

    public async Task BootstrapAsync()
    {
        var name = string.IsNullOrWhiteSpace(DisplayName) ? "Windows User" : DisplayName.Trim();
        var relay = string.IsNullOrWhiteSpace(RelayUrl) ? "http://127.0.0.1:8787" : RelayUrl.Trim();
        await RunAsync("Creating local identity", async () =>
        {
            ApplySnapshot(await client.BootstrapAsync(name, relay));
        });
    }

    public async Task UpdateRelayAsync()
    {
        var relay = RelayUrl.Trim();
        if (relay.Length == 0) return;
        await RunAsync("Updating relay", async () =>
        {
            ApplySnapshot(await client.UpdateRelayAsync(relay));
        });
    }

    public async Task<InviteResponse> CreateInviteAsync()
    {
        InviteResponse? response = null;
        await RunAsync("Creating invite", async () =>
        {
            response = await client.InviteAsync();
        });
        return response ?? new InviteResponse();
    }

    public async Task<TemporaryInviteResponse> CreateTemporaryInviteAsync()
    {
        TemporaryInviteResponse? response = null;
        await RunAsync("Creating temporary invite", async () =>
        {
            response = await client.TemporaryInviteAsync();
        });
        return response ?? new TemporaryInviteResponse();
    }

    public async Task AddContactAsync(string displayName, string inviteUri)
    {
        await RunAsync("Adding contact", async () =>
        {
            ApplySnapshot(await client.AddContactAsync(displayName.Trim(), inviteUri.Trim()));
        });
    }

    public async Task StartTemporaryConnectionAsync(string inviteUri)
    {
        await RunAsync("Starting temporary session", async () =>
        {
            var response = await client.StartTemporaryConnectionAsync(inviteUri.Trim());
            ApplySnapshot(response.Snapshot);
            SelectedConversation = Conversations.FirstOrDefault(c => c.ThreadKind == "temporary" && c.ThreadId == response.ConnectionId)
                ?? SelectedConversation;
        });
    }

    public async Task EndSelectedTemporaryConversationAsync()
    {
        if (SelectedConversation?.ThreadKind != "temporary") return;
        var ended = SelectedConversation.ThreadId;
        await RunAsync("Ending temporary session", async () =>
        {
            ApplySnapshot(await client.EndTemporaryConnectionAsync(ended));
            SelectedConversation = Conversations.FirstOrDefault();
        });
    }

    public async Task RenameSelectedContactAsync(string displayName)
    {
        if (SelectedConversation?.ThreadKind != "contact") return;
        await RunAsync("Renaming contact", async () =>
        {
            ApplySnapshot(await client.UpdateContactDisplayNameAsync(SelectedConversation.ThreadId, displayName.Trim()));
        });
    }

    public async Task DeleteSelectedContactAsync()
    {
        if (SelectedConversation?.ThreadKind != "contact") return;
        var deleted = SelectedConversation.ThreadId;
        await RunAsync("Deleting contact", async () =>
        {
            ApplySnapshot(await client.DeleteContactAsync(deleted));
            SelectedConversation = Conversations.FirstOrDefault();
        });
    }

    public async Task SendTextAsync(string body)
    {
        if (SelectedConversation is null || string.IsNullOrWhiteSpace(body)) return;
        var conversation = SelectedConversation;
        await RunAsync("Sending", async () =>
        {
            var next = conversation.ThreadKind switch
            {
                "temporary" => await client.SendTemporaryMessageAsync(conversation.ThreadId, body),
                _ => await client.SendMessageAsync(conversation.ThreadId, body),
            };
            ApplySnapshot(next, forceThreadId: conversation.ThreadId, forceThreadKind: conversation.ThreadKind);
        });
    }

    public async Task SendAttachmentAsync(string filePath, string kind)
    {
        if (SelectedConversation is null || string.IsNullOrWhiteSpace(filePath)) return;
        var conversation = SelectedConversation;
        await RunAsync("Sending attachment", async () =>
        {
            var response = await client.SendAttachmentAsync(conversation.ThreadKind, conversation.ThreadId, filePath, kind);
            ApplySnapshot(response.Snapshot, forceThreadId: conversation.ThreadId, forceThreadKind: conversation.ThreadKind);
        });
    }

    public async Task SendBurnAsync(string body)
    {
        if (SelectedConversation is null || string.IsNullOrWhiteSpace(body)) return;
        var conversation = SelectedConversation;
        await RunAsync("Sending burn message", async () =>
        {
            var next = await client.SendBurnMessageAsync(conversation.ThreadKind, conversation.ThreadId, body);
            ApplySnapshot(next, forceThreadId: conversation.ThreadId, forceThreadKind: conversation.ThreadKind);
        });
    }

    public async Task OpenBurnMessageAsync(MessageItem item)
    {
        await RunAsync("Opening burn message", async () =>
        {
            ApplySnapshot(await client.OpenBurnMessageAsync(item.ThreadKind, item.ThreadId, item.Id),
                forceThreadId: item.ThreadId,
                forceThreadKind: item.ThreadKind);
        });
    }

    public async Task ImportStickerAsync(string filePath)
    {
        var display = Path.GetFileNameWithoutExtension(filePath);
        await RunAsync("Importing sticker", async () =>
        {
            var response = await client.ImportStickerAsync(filePath, display);
            ApplySnapshot(response.Snapshot);
        });
    }

    public async Task SendStickerAsync(StickerItem sticker)
    {
        await SendAttachmentAsync(sticker.LocalPath, "sticker");
    }

    public async Task DeleteStickerAsync(StickerItem sticker)
    {
        await RunAsync("Deleting sticker", async () =>
        {
            ApplySnapshot(await client.DeleteStickerAsync(sticker.Id));
        });
    }

    public async Task<int> ReceiveAsync()
    {
        var before = lastTotalMessageCount;
        var received = 0;
        await RunAsync("Receiving", async () =>
        {
            var report = await client.ReceiveAsync();
            received = CountMessages(report.Snapshot) - before;
            ApplySnapshot(report.Snapshot);
        });
        return Math.Max(0, received);
    }

    private async Task RunAsync(string label, Func<Task> work)
    {
        IsBusy = true;
        StatusText = label;
        try
        {
            await work();
            StatusText = "Ready";
        }
        catch (Exception ex)
        {
            StatusText = ex.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void ApplySnapshot(AppSnapshot next, string? forceThreadId = null, string? forceThreadKind = null)
    {
        var selectedKind = forceThreadKind ?? SelectedConversation?.ThreadKind;
        var selectedId = forceThreadId ?? SelectedConversation?.ThreadId;
        Snapshot = next;
        lastTotalMessageCount = CountMessages(next);
        RebuildConversations();
        RebuildStickers();
        RebuildTransfers();
        SelectedConversation = Conversations.FirstOrDefault(c => c.ThreadKind == selectedKind && c.ThreadId == selectedId)
            ?? Conversations.FirstOrDefault();
    }

    private void RebuildConversations()
    {
        Conversations.Clear();
        foreach (var contact in Snapshot.Contacts.OrderByDescending(c => c.UpdatedAtUnix))
        {
            Conversations.Add(new ConversationItem
            {
                ThreadKind = "contact",
                ThreadId = contact.Id,
                Title = contact.DisplayName,
                Subtitle = contact.LastMessage ?? ShortId(contact.DeviceId),
                SafetyNumber = contact.SafetyNumber,
            });
        }

        foreach (var temporary in Snapshot.TemporaryConnections.OrderByDescending(t => t.UpdatedAtUnix))
        {
            Conversations.Add(new ConversationItem
            {
                ThreadKind = "temporary",
                ThreadId = temporary.Id,
                Title = $"Timer {temporary.DisplayName}",
                Subtitle = temporary.LastMessage ?? "Temporary session",
                SafetyNumber = temporary.SafetyNumber,
            });
        }
    }

    private void RebuildVisibleMessages()
    {
        if (SelectedConversation is null)
        {
            visibleMessageSignature = "";
            VisibleMessages.Clear();
            return;
        }

        IEnumerable<MessageItem> messages = SelectedConversation.ThreadKind switch
        {
            "temporary" => Snapshot.TemporaryMessages
                .Where(m => m.ConnectionId == SelectedConversation.ThreadId)
                .OrderBy(m => m.SentAtUnix)
                .Select(m => FromTemporaryMessage(m)),
            _ => Snapshot.Messages
                .Where(m => m.ContactId == SelectedConversation.ThreadId)
                .OrderBy(m => m.SentAtUnix)
                .Select(m => FromChatMessage(m)),
        };

        var next = messages.ToList();
        var nextSignature = MessageSignature(next);
        if (nextSignature == visibleMessageSignature)
        {
            return;
        }

        if (CanAppend(VisibleMessages, next))
        {
            for (var index = VisibleMessages.Count; index < next.Count; index++)
            {
                VisibleMessages.Add(next[index]);
            }
        }
        else
        {
            VisibleMessages.Clear();
            foreach (var item in next)
            {
                VisibleMessages.Add(item);
            }
        }

        visibleMessageSignature = nextSignature;
    }

    private void RebuildStickers()
    {
        Stickers.Clear();
        foreach (var sticker in Snapshot.Stickers)
        {
            Stickers.Add(sticker);
        }
    }

    private void RebuildTransfers()
    {
        AttachmentTransfers.Clear();
        foreach (var transfer in Snapshot.AttachmentTransfers)
        {
            AttachmentTransfers.Add(transfer);
        }
    }

    private MessageItem FromChatMessage(AppChatMessage message) =>
        new()
        {
            Id = message.Id,
            ThreadKind = "contact",
            ThreadId = message.ContactId,
            IsOutgoing = message.Direction == AppMessageDirection.Outgoing,
            SenderLine = message.Direction == AppMessageDirection.Outgoing ? "You" : SelectedConversation?.Title ?? "Contact",
            DisplayText = DisplayText(message.Body, message.Content),
            AttachmentSummary = AttachmentSummary(message.Content.Attachment),
            LocalPath = message.Content.Attachment?.LocalPath,
            PreviewImage = PreviewImage(message.Content),
            StatusLine = $"{message.Status} · {UnixTime(message.SentAtUnix)}",
            IsBurnMessage = message.Content.Kind == "burn" && !message.Content.Destroyed,
        };

    private MessageItem FromTemporaryMessage(TemporaryMessage message) =>
        new()
        {
            Id = message.Id,
            ThreadKind = "temporary",
            ThreadId = message.ConnectionId,
            IsOutgoing = message.Direction == AppMessageDirection.Outgoing,
            SenderLine = message.Direction == AppMessageDirection.Outgoing ? "You" : SelectedConversation?.Title ?? "Temporary",
            DisplayText = DisplayText(message.Body, message.Content),
            AttachmentSummary = AttachmentSummary(message.Content.Attachment),
            LocalPath = message.Content.Attachment?.LocalPath,
            PreviewImage = PreviewImage(message.Content),
            StatusLine = $"{message.Status} · {UnixTime(message.SentAtUnix)}",
            IsBurnMessage = message.Content.Kind == "burn" && !message.Content.Destroyed,
        };

    private static string DisplayText(string body, MessageContent content) =>
        content.Kind switch
        {
            "burn" when content.Destroyed => "Burned message",
            "burn" => content.Text ?? "Burn after reading",
            "image" => content.Text ?? "Image",
            "file" => content.Text ?? "File",
            "sticker" => content.Text ?? "Sticker",
            _ => content.Text ?? body,
        };

    private static string AttachmentSummary(AttachmentContent? attachment) =>
        attachment is null
            ? ""
            : $"{attachment.FileName} · {ByteCount(attachment.SizeBytes)} · {attachment.TransferStatus}";

    private static ImageSource? PreviewImage(MessageContent content)
    {
        var attachment = content.Attachment;
        if (attachment?.LocalPath is not { Length: > 0 } localPath)
        {
            return null;
        }

        var kind = string.IsNullOrWhiteSpace(attachment.Kind) ? content.Kind : attachment.Kind;
        if (kind is not ("image" or "sticker"))
        {
            return null;
        }

        try
        {
            return new BitmapImage(new Uri(localPath, UriKind.Absolute));
        }
        catch
        {
            return null;
        }
    }

    private static int CountMessages(AppSnapshot value) =>
        value.Messages.Count + value.TemporaryMessages.Count;

    private static bool CanAppend(IReadOnlyList<MessageItem> current, IReadOnlyList<MessageItem> next)
    {
        if (current.Count > next.Count)
        {
            return false;
        }

        for (var index = 0; index < current.Count; index++)
        {
            if (current[index].Id != next[index].Id ||
                current[index].StatusLine != next[index].StatusLine ||
                current[index].DisplayText != next[index].DisplayText ||
                current[index].AttachmentSummary != next[index].AttachmentSummary)
            {
                return false;
            }
        }

        return true;
    }

    private static string MessageSignature(IEnumerable<MessageItem> messages) =>
        string.Join("\n", messages.Select(message =>
            $"{message.Id}|{message.StatusLine}|{message.DisplayText}|{message.AttachmentSummary}|{message.LocalPath}"));

    private static string ShortId(string value) =>
        value.Length <= 12 ? value : value[..12];

    private static string UnixTime(ulong unix)
    {
        var date = DateTimeOffset.FromUnixTimeSeconds((long)unix).ToLocalTime();
        return date.ToString("g");
    }

    private static string ByteCount(ulong bytes)
    {
        string[] units = ["B", "KB", "MB", "GB", "TB"];
        var size = (double)bytes;
        var unit = 0;
        while (size >= 1024 && unit < units.Length - 1)
        {
            size /= 1024;
            unit++;
        }
        return $"{size:0.#} {units[unit]}";
    }
}
