using Microsoft.Extensions.DependencyInjection;
using Microsoft.UI.Xaml;
using ReadOS.App.Services;
using ReadOS.App.Services.Msp;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public partial class App : Application
{
    private Window? window;
    private readonly ReadOsPackageSmokeOptions? packageSmokeOptions;
    private readonly string? packageSmokeArgumentError;

    public static IServiceProvider Services { get; private set; } = null!;

    public App()
    {
        var arguments = Environment.GetCommandLineArgs().Skip(1).ToArray();
        var packageSmokeRequested = ReadOsPackageSmokeOptions.IsRequested(arguments);
        if (!ReadOsPackageSmokeOptions.TryParse(
            arguments,
            out packageSmokeOptions,
            out var parseError) && packageSmokeRequested)
        {
            packageSmokeArgumentError = parseError ?? "Invalid package smoke arguments.";
        }

        InitializeComponent();
        ConfigureServices(packageSmokeOptions);
    }

    private static void ConfigureServices(ReadOsPackageSmokeOptions? smokeOptions)
    {
        if (smokeOptions is not null)
        {
            Services = ReadOsPackageSmokeComposition.Build(smokeOptions);
            return;
        }

        var services = new ServiceCollection();

        services.AddSingleton(new ReadOsPackagedRuntimeFfiRegistration(
            ReadOsPackagedRuntimeFfiLoader.TryLoad()));
        services.AddSingleton<IPdfDocumentService, PdfDocumentService>();
        services.AddSingleton<IProviderCredentialStore, WindowsDpapiProviderCredentialStore>();
        services.AddSingleton<IWorkspaceStore, WorkspaceStore>();
        services.AddSingleton<IFileDialogService, FileDialogService>();
        services.AddSingleton<IClipboardService, ClipboardService>();
        services.AddSingleton<IAiChatService, AiChatService>();
        services.AddSingleton<LayoutService>();
        services.AddTransient<ShellViewModel>();

        Services = services.BuildServiceProvider();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        if (packageSmokeArgumentError is not null)
        {
            Environment.ExitCode = 2;
            Exit();
            return;
        }

        if (packageSmokeOptions is not null)
        {
            RunPackageSmokeAndExitAsync();
            return;
        }

        var viewModel = Services.GetRequiredService<ShellViewModel>();
        var layoutService = Services.GetRequiredService<LayoutService>();
        window = new MainWindow(viewModel, layoutService);
        window.Activate();
    }

    private async void RunPackageSmokeAndExitAsync()
    {
        var exitCode = 1;
        try
        {
            exitCode = await Services
                .GetRequiredService<ReadOsPackageSmokeService>()
                .RunAsync();
        }
        finally
        {
            Environment.ExitCode = exitCode;
            Exit();
        }
    }
}
