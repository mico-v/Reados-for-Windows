using Microsoft.Extensions.DependencyInjection;
using Microsoft.UI.Xaml;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public partial class App : Application
{
    private Window? window;

    public static IServiceProvider Services { get; private set; } = null!;

    public App()
    {
        InitializeComponent();
        ConfigureServices();
    }

    private static void ConfigureServices()
    {
        var services = new ServiceCollection();

        services.AddSingleton<IPdfDocumentService, PdfDocumentService>();
        services.AddSingleton<IWorkspaceStore, WorkspaceStore>();
        services.AddSingleton<IFileDialogService, FileDialogService>();
        services.AddSingleton<IAiChatService, AiChatService>();
        services.AddSingleton<LayoutService>();
        services.AddTransient<ShellViewModel>();

        Services = services.BuildServiceProvider();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var viewModel = Services.GetRequiredService<ShellViewModel>();
        var layoutService = Services.GetRequiredService<LayoutService>();
        window = new MainWindow(viewModel, layoutService);
        window.Activate();
    }
}
