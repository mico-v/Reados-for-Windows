using ReadOS.App.Services.Msp;
using ReadOS.App.ViewModels;

namespace ReadOS.App.Tests.Services.Msp;

public sealed class ReadOsReaderLayoutServiceTests
{
    [Fact]
    public void NavigateReader_routes_to_preview_with_materials_sidebar()
    {
        var service = new ReadOsReaderLayoutService();

        var layout = service.NavigateReader();

        Assert.Equal(ShellRoute.Home, layout.Route);
        Assert.True(layout.IsInspectorVisible);
        Assert.Equal(InspectorTab.Preview, layout.InspectorTab);
        Assert.Equal(WorkspaceSidebarMode.Materials, layout.SidebarMode);
    }

    [Theory]
    [InlineData(WorkspaceLayoutMode.FocusChat, false, null)]
    [InlineData(WorkspaceLayoutMode.FocusPresenter, true, InspectorTab.Preview)]
    [InlineData(WorkspaceLayoutMode.PresenterPrimary, true, InspectorTab.Preview)]
    [InlineData(WorkspaceLayoutMode.ChatPrimary, true, null)]
    public void ApplyWorkspaceLayout_projects_inspector_state(
        WorkspaceLayoutMode layout,
        bool expectedInspectorVisible,
        InspectorTab? expectedTab)
    {
        var service = new ReadOsReaderLayoutService();

        var preset = service.ApplyWorkspaceLayout(layout);

        Assert.True(preset.ShouldApply);
        Assert.Equal(layout, preset.Layout);
        Assert.Equal(expectedInspectorVisible, preset.IsInspectorVisible);
        Assert.Equal(expectedTab, preset.InspectorTab);
    }

    [Fact]
    public void ResolveWorkspaceLayout_parses_case_insensitively_and_ignores_invalid_modes()
    {
        var service = new ReadOsReaderLayoutService();

        var valid = service.ResolveWorkspaceLayout("presenterprimary");
        var invalid = service.ResolveWorkspaceLayout("missing");

        Assert.True(valid.ShouldApply);
        Assert.Equal(WorkspaceLayoutMode.PresenterPrimary, valid.Layout);
        Assert.Equal(InspectorTab.Preview, valid.InspectorTab);
        Assert.False(invalid.ShouldApply);
    }

    [Fact]
    public void SelectSidebarMode_opens_library_for_valid_modes()
    {
        var service = new ReadOsReaderLayoutService();

        var selection = service.SelectSidebarMode("sessions");
        var invalid = service.SelectSidebarMode("missing");

        Assert.True(selection.ShouldApply);
        Assert.Equal(WorkspaceSidebarMode.Sessions, selection.SidebarMode);
        Assert.True(selection.IsLibraryVisible);
        Assert.False(invalid.ShouldApply);
    }

    [Fact]
    public void SelectInspectorTab_opens_chat_for_valid_tabs()
    {
        var service = new ReadOsReaderLayoutService();

        var selection = service.SelectInspectorTab("policy");
        var invalid = service.SelectInspectorTab("missing");

        Assert.True(selection.ShouldApply);
        Assert.Equal(InspectorTab.Policy, selection.InspectorTab);
        Assert.True(selection.IsChatVisible);
        Assert.False(invalid.ShouldApply);
    }

    [Theory]
    [InlineData(true, false)]
    [InlineData(false, true)]
    public void ToggleOutline_keeps_chat_visibility_in_sync(bool currentVisible, bool expectedVisible)
    {
        var service = new ReadOsReaderLayoutService();

        var toggle = service.ToggleOutline(currentVisible);

        Assert.Equal(expectedVisible, toggle.IsOutlineVisible);
        Assert.Equal(expectedVisible, toggle.IsChatVisible);
        Assert.Equal(InspectorTab.Evidence, toggle.InspectorTab);
    }

    [Theory]
    [InlineData(false, false, true, true)]
    [InlineData(false, true, true, true)]
    [InlineData(true, false, false, false)]
    [InlineData(true, true, false, true)]
    public void ToggleRunDrawerPin_opens_drawer_when_pinned_and_preserves_open_state_when_unpinned(
        bool currentPinned,
        bool currentOpen,
        bool expectedPinned,
        bool expectedOpen)
    {
        var service = new ReadOsReaderLayoutService();

        var pin = service.ToggleRunDrawerPin(currentPinned, currentOpen);

        Assert.Equal(expectedPinned, pin.IsRunDrawerPinned);
        Assert.Equal(expectedOpen, pin.IsRunDrawerOpen);
    }
}
