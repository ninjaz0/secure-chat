using Microsoft.UI.Xaml;

namespace SecureChatWindows;

public partial class App : Application
{
    private Window? window;

    public App()
    {
        InitializeComponent();
        UnhandledException += (_, args) =>
        {
            args.Handled = true;
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        window = new MainWindow();
        window.Activate();
    }
}
