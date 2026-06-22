using Microsoft.UI.Xaml;
using ReadOS.App.Services;
using ReadOS.App.ViewModels;

namespace ReadOS.App;

public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        ViewModel = new ShellViewModel(new MockWorkspaceService());
        DataContext = ViewModel;
    }

    public ShellViewModel ViewModel { get; }
}
