using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.Windows.AppNotifications;
using Microsoft.Windows.AppNotifications.Builder;
using SecureChatWindows.Models;
using SecureChatWindows.ViewModels;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage.Pickers;
using Windows.System;

namespace SecureChatWindows;

public sealed partial class MainWindow : Window
{
    private readonly DispatcherQueueTimer receiveTimer;
    private ScrollViewer? messageScrollViewer;
    private bool isPinnedToBottom = true;

    public MainViewModel ViewModel { get; } = new();

    public MainWindow()
    {
        InitializeComponent();
        Title = "SecureChat";
        receiveTimer = DispatcherQueue.CreateTimer();
        receiveTimer.Interval = TimeSpan.FromSeconds(5);
        receiveTimer.Tick += async (_, _) => await ReceiveAndNotifyAsync();
        ViewModel.VisibleMessages.CollectionChanged += (_, _) => FollowMessagesIfNeeded();
        RegisterNotifications();
    }

    private async void Root_Loaded(object sender, RoutedEventArgs e)
    {
        await ViewModel.InitializeAsync();
        receiveTimer.Start();
    }

    private void RegisterNotifications()
    {
        try
        {
            AppNotificationManager.Default.Register();
        }
        catch
        {
        }
    }

    private async Task ReceiveAndNotifyAsync()
    {
        var received = await ViewModel.ReceiveAsync();
        if (received > 0 && ViewModel.NotifyOnNewMessages && !isPinnedToBottom)
        {
            try
            {
                var notification = new AppNotificationBuilder()
                    .AddText("SecureChat")
                    .AddText($"{received} new encrypted message(s)")
                    .BuildNotification();
                AppNotificationManager.Default.Show(notification);
            }
            catch
            {
            }
        }
    }

    private async void Bootstrap_Click(object sender, RoutedEventArgs e) => await ViewModel.BootstrapAsync();
    private async void UpdateRelay_Click(object sender, RoutedEventArgs e) => await ViewModel.UpdateRelayAsync();
    private async void Receive_Click(object sender, RoutedEventArgs e) => await ReceiveAndNotifyAsync();

    private async void Invite_Click(object sender, RoutedEventArgs e)
    {
        var invite = await ViewModel.CreateInviteAsync();
        CopyToClipboard(invite.InviteUri);
        await ShowTextAsync("Invite copied", invite.InviteUri);
    }

    private async void TemporaryInvite_Click(object sender, RoutedEventArgs e)
    {
        var invite = await ViewModel.CreateTemporaryInviteAsync();
        CopyToClipboard(invite.InviteUri);
        await ShowTextAsync("Temporary invite copied", invite.InviteUri);
    }

    private async void AddContact_Click(object sender, RoutedEventArgs e)
    {
        var invite = await PromptAsync("Add contact", "Paste schat://invite/...");
        if (string.IsNullOrWhiteSpace(invite)) return;
        var name = await PromptAsync("Contact nickname", "Display name");
        if (string.IsNullOrWhiteSpace(name)) name = "Contact";
        await ViewModel.AddContactAsync(name, invite);
    }

    private async void StartTemporary_Click(object sender, RoutedEventArgs e)
    {
        var invite = await PromptAsync("Start temporary session", "Paste temporary schat://invite/...");
        if (!string.IsNullOrWhiteSpace(invite))
        {
            await ViewModel.StartTemporaryConnectionAsync(invite);
        }
    }

    private async void CreateGroup_Click(object sender, RoutedEventArgs e)
    {
        var name = await PromptAsync("New group", "Group name");
        if (!string.IsNullOrWhiteSpace(name))
        {
            await ViewModel.CreateGroupAsync(name);
        }
    }

    private async void RenameContact_Click(object sender, RoutedEventArgs e)
    {
        var name = await PromptAsync("Rename contact", "New nickname");
        if (!string.IsNullOrWhiteSpace(name))
        {
            await ViewModel.RenameSelectedContactAsync(name);
        }
    }

    private async void DeleteContact_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConversation?.ThreadKind != "contact") return;
        var confirmed = await ConfirmAsync("Delete contact", "This deletes the local contact, one-to-one history, session keys, and unfinished attachments.");
        if (confirmed)
        {
            await ViewModel.DeleteSelectedContactAsync();
        }
    }

    private async void AddGroupMember_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConversation?.ThreadKind != "group") return;
        var names = ViewModel.Snapshot.Contacts.Select(c => $"{c.DisplayName} ({c.Id})");
        var value = await PromptAsync("Add group member", "Paste contact id", string.Join(Environment.NewLine, names));
        if (!string.IsNullOrWhiteSpace(value))
        {
            await ViewModel.AddContactToSelectedGroupAsync(value.Trim());
        }
    }

    private async void EndTemporary_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConversation?.ThreadKind != "temporary") return;
        var confirmed = await ConfirmAsync("End temporary session", "This removes the temporary session and local temporary messages.");
        if (confirmed)
        {
            await ViewModel.EndSelectedTemporaryConversationAsync();
        }
    }

    private async void Send_Click(object sender, RoutedEventArgs e)
    {
        var body = ComposerBox.Text;
        ComposerBox.Text = "";
        await ViewModel.SendTextAsync(body);
        ScrollToBottom();
    }

    private async void SendBurn_Click(object sender, RoutedEventArgs e)
    {
        var body = await PromptAsync("Burn after reading", "Message");
        if (!string.IsNullOrWhiteSpace(body))
        {
            await ViewModel.SendBurnAsync(body);
            ScrollToBottom();
        }
    }

    private async void SendFile_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            await ViewModel.SendAttachmentAsync(path, "file");
            ScrollToBottom();
        }
    }

    private async void SendImage_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".png", ".jpg", ".jpeg", ".gif", ".webp");
        if (path is not null)
        {
            await ViewModel.SendAttachmentAsync(path, "image");
            ScrollToBottom();
        }
    }

    private async void ImportSticker_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".png", ".jpg", ".jpeg", ".gif", ".webp");
        if (path is not null)
        {
            await ViewModel.ImportStickerAsync(path);
        }
    }

    private async void SendSticker_Click(object sender, RoutedEventArgs e)
    {
        if (StickerBox.SelectedItem is StickerItem sticker)
        {
            await ViewModel.SendStickerAsync(sticker);
            ScrollToBottom();
        }
    }

    private async void DeleteSticker_Click(object sender, RoutedEventArgs e)
    {
        if (StickerBox.SelectedItem is StickerItem sticker)
        {
            var confirmed = await ConfirmAsync("Delete sticker", $"Delete {sticker.DisplayName} from this device?");
            if (confirmed)
            {
                await ViewModel.DeleteStickerAsync(sticker);
            }
        }
    }

    private async void OpenBurn_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: MessageItem item })
        {
            var confirmed = await ConfirmAsync("Open burn message", "The message content will be destroyed locally after opening.");
            if (confirmed)
            {
                await ViewModel.OpenBurnMessageAsync(item);
            }
        }
    }

    private async void OpenAttachment_Click(object sender, RoutedEventArgs e)
    {
        if (sender is Button { Tag: MessageItem item })
        {
            await OpenAttachmentAsync(item);
        }
    }

    private async void PreviewAttachment_Tapped(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: MessageItem item })
        {
            await OpenAttachmentAsync(item);
        }
    }

    private static async Task OpenAttachmentAsync(MessageItem item)
    {
        if (string.IsNullOrWhiteSpace(item.LocalPath) || !File.Exists(item.LocalPath))
        {
            return;
        }

        var file = await Windows.Storage.StorageFile.GetFileFromPathAsync(item.LocalPath);
        await Launcher.LaunchFileAsync(file);
    }

    private void ConversationList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        ViewModel.SelectedConversation = ConversationList.SelectedItem as ConversationItem;
        if (ViewModel.VisibleMessages.Count > 0)
        {
            ScrollToBottom();
        }
    }

    private void MessageList_Loaded(object sender, RoutedEventArgs e)
    {
        messageScrollViewer = FindChild<ScrollViewer>(MessageList);
        if (messageScrollViewer is not null)
        {
            messageScrollViewer.ViewChanged += (_, _) =>
            {
                var remaining = messageScrollViewer.ScrollableHeight - messageScrollViewer.VerticalOffset;
                isPinnedToBottom = remaining <= 80 || messageScrollViewer.ScrollableHeight <= 0;
                if (isPinnedToBottom)
                {
                    NewMessagesButton.Visibility = Visibility.Collapsed;
                }
            };
        }
    }

    private void FollowMessagesIfNeeded()
    {
        if (isPinnedToBottom)
        {
            ScrollToBottom();
        }
        else
        {
            NewMessagesButton.Visibility = Visibility.Visible;
        }
    }

    private void NewMessages_Click(object sender, RoutedEventArgs e) => ScrollToBottom();

    private void ScrollToBottom()
    {
        if (ViewModel.VisibleMessages.Count == 0) return;
        MessageList.ScrollIntoView(ViewModel.VisibleMessages[^1]);
        isPinnedToBottom = true;
        NewMessagesButton.Visibility = Visibility.Collapsed;
    }

    private async Task<string?> PickFileAsync(params string[] extensions)
    {
        var picker = new FileOpenPicker();
        foreach (var extension in extensions)
        {
            picker.FileTypeFilter.Add(extension);
        }
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        return file?.Path;
    }

    private async Task<string?> PromptAsync(string title, string placeholder, string? details = null)
    {
        var input = new TextBox
        {
            PlaceholderText = placeholder,
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MinHeight = 80,
        };
        var panel = new StackPanel { Spacing = 8 };
        if (!string.IsNullOrWhiteSpace(details))
        {
            panel.Children.Add(new TextBlock { Text = details, TextWrapping = TextWrapping.Wrap });
        }
        panel.Children.Add(input);

        var dialog = new ContentDialog
        {
            Title = title,
            Content = panel,
            PrimaryButtonText = "OK",
            CloseButtonText = "Cancel",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        var result = await dialog.ShowAsync();
        return result == ContentDialogResult.Primary ? input.Text : null;
    }

    private async Task<bool> ConfirmAsync(string title, string content)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = content,
            PrimaryButtonText = "Confirm",
            CloseButtonText = "Cancel",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private async Task ShowTextAsync(string title, string content)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = new TextBlock { Text = content, TextWrapping = TextWrapping.Wrap, IsTextSelectionEnabled = true },
            CloseButtonText = "OK",
            XamlRoot = ((FrameworkElement)Content).XamlRoot,
        };
        await dialog.ShowAsync();
    }

    private static void CopyToClipboard(string text)
    {
        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
    }

    private static T? FindChild<T>(DependencyObject root) where T : DependencyObject
    {
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
        {
            var child = VisualTreeHelper.GetChild(root, i);
            if (child is T typed)
            {
                return typed;
            }

            var nested = FindChild<T>(child);
            if (nested is not null)
            {
                return nested;
            }
        }

        return null;
    }
}
