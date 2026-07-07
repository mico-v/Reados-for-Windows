using ReadOS.App.ViewModels;

namespace ReadOS.App.Services.Msp;

internal readonly record struct ReadOsReaderNavigationLayout(
    ShellRoute Route,
    bool IsInspectorVisible,
    InspectorTab InspectorTab,
    WorkspaceSidebarMode SidebarMode);

internal readonly record struct ReadOsWorkspaceLayoutPreset(
    bool ShouldApply,
    WorkspaceLayoutMode Layout,
    bool IsInspectorVisible,
    InspectorTab? InspectorTab);

internal readonly record struct ReadOsSidebarModeSelection(
    bool ShouldApply,
    WorkspaceSidebarMode SidebarMode,
    bool IsLibraryVisible);

internal readonly record struct ReadOsInspectorTabSelection(
    bool ShouldApply,
    InspectorTab InspectorTab,
    bool IsChatVisible);

internal readonly record struct ReadOsOutlineToggle(
    bool IsOutlineVisible,
    bool IsChatVisible,
    InspectorTab InspectorTab);

internal readonly record struct ReadOsRunDrawerPin(
    bool IsRunDrawerPinned,
    bool IsRunDrawerOpen);

internal sealed class ReadOsReaderLayoutService
{
    public ReadOsReaderNavigationLayout NavigateReader()
    {
        return new ReadOsReaderNavigationLayout(
            ShellRoute.Home,
            true,
            InspectorTab.Preview,
            WorkspaceSidebarMode.Materials);
    }

    public ReadOsWorkspaceLayoutPreset ResolveWorkspaceLayout(string mode)
    {
        return Enum.TryParse<WorkspaceLayoutMode>(mode, ignoreCase: true, out var layout)
            ? ApplyWorkspaceLayout(layout)
            : new ReadOsWorkspaceLayoutPreset(false, WorkspaceLayoutMode.FocusChat, false, null);
    }

    public ReadOsWorkspaceLayoutPreset ApplyWorkspaceLayout(WorkspaceLayoutMode layout)
    {
        return layout switch
        {
            WorkspaceLayoutMode.FocusChat =>
                new ReadOsWorkspaceLayoutPreset(true, layout, false, null),
            WorkspaceLayoutMode.FocusPresenter or WorkspaceLayoutMode.PresenterPrimary =>
                new ReadOsWorkspaceLayoutPreset(true, layout, true, InspectorTab.Preview),
            _ =>
                new ReadOsWorkspaceLayoutPreset(true, layout, true, null)
        };
    }

    public ReadOsSidebarModeSelection SelectSidebarMode(string mode)
    {
        return Enum.TryParse<WorkspaceSidebarMode>(mode, ignoreCase: true, out var sidebarMode)
            ? new ReadOsSidebarModeSelection(true, sidebarMode, true)
            : new ReadOsSidebarModeSelection(false, WorkspaceSidebarMode.Conversations, false);
    }

    public ReadOsInspectorTabSelection SelectInspectorTab(string tab)
    {
        return Enum.TryParse<InspectorTab>(tab, ignoreCase: true, out var inspectorTab)
            ? new ReadOsInspectorTabSelection(true, inspectorTab, true)
            : new ReadOsInspectorTabSelection(false, InspectorTab.Evidence, false);
    }

    public ReadOsOutlineToggle ToggleOutline(bool isOutlineVisible)
    {
        var nextVisible = !isOutlineVisible;
        return new ReadOsOutlineToggle(nextVisible, nextVisible, InspectorTab.Evidence);
    }

    public ReadOsRunDrawerPin ToggleRunDrawerPin(bool isRunDrawerPinned, bool isRunDrawerOpen)
    {
        var nextPinned = !isRunDrawerPinned;
        return new ReadOsRunDrawerPin(nextPinned, nextPinned || isRunDrawerOpen);
    }
}
