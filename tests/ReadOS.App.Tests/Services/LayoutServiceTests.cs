using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.Tests.Services;

public sealed class LayoutServiceTests
{
    [Fact]
    public void Responsive_breakpoints_do_not_replace_remembered_user_widths()
    {
        var service = new LayoutService();
        service.RememberPaneWidths(372, 548);

        var compact = service.ComputeConfiguration(800, sidebarRequested: true, inspectorRequested: true);
        var medium = service.ComputeConfiguration(1_000, sidebarRequested: true, inspectorRequested: true);
        var restored = service.ComputeConfiguration(1_400, sidebarRequested: true, inspectorRequested: true);

        Assert.Equal(LayoutBreakpoint.Compact, compact.Breakpoint);
        Assert.False(compact.SidebarVisible);
        Assert.False(compact.InspectorVisible);
        Assert.Equal(0, compact.SidebarWidth);
        Assert.Equal(0, compact.InspectorWidth);

        Assert.Equal(LayoutBreakpoint.Medium, medium.Breakpoint);
        Assert.Equal(220, medium.SidebarWidth);
        Assert.Equal(320, medium.InspectorWidth);

        Assert.Equal(LayoutBreakpoint.Wide, restored.Breakpoint);
        Assert.Equal(372, restored.SidebarWidth);
        Assert.Equal(548, restored.InspectorWidth);
        Assert.Equal(372, service.RememberedSidebarWidth);
        Assert.Equal(548, service.RememberedInspectorWidth);
    }

    [Fact]
    public void Visibility_toggles_do_not_clear_remembered_user_widths()
    {
        var service = new LayoutService();
        service.RememberPaneWidths(344, 512);

        var hidden = service.ComputeConfiguration(1_400, sidebarRequested: false, inspectorRequested: false);
        var restored = service.ComputeConfiguration(1_400, sidebarRequested: true, inspectorRequested: true);

        Assert.False(hidden.SidebarVisible);
        Assert.False(hidden.InspectorVisible);
        Assert.Equal(344, restored.SidebarWidth);
        Assert.Equal(512, restored.InspectorWidth);
    }

    [Theory]
    [InlineData(100, 100, LayoutService.SidebarMin, LayoutService.InspectorMin)]
    [InlineData(900, 900, LayoutService.SidebarMax, LayoutService.InspectorMax)]
    public void RememberPaneWidths_clamps_preferences_to_usable_bounds(
        double sidebarWidth,
        double inspectorWidth,
        double expectedSidebarWidth,
        double expectedInspectorWidth)
    {
        var service = new LayoutService();

        service.RememberPaneWidths(sidebarWidth, inspectorWidth);

        Assert.Equal(expectedSidebarWidth, service.RememberedSidebarWidth);
        Assert.Equal(expectedInspectorWidth, service.RememberedInspectorWidth);
    }
}
