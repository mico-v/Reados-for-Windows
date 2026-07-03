using System;

namespace ReadOS.App.Models;

/// <summary>
/// Describes the current responsive layout configuration for the 3-column shell.
/// </summary>
public sealed class LayoutConfiguration
{
    public static LayoutConfiguration Default { get; } = new()
    {
        SidebarVisible = true,
        SidebarWidth = 260,
        InspectorVisible = true,
        InspectorWidth = 420,
        WindowWidth = 1600,
        Breakpoint = LayoutBreakpoint.Wide
    };

    public bool SidebarVisible { get; init; } = true;
    public double SidebarWidth { get; init; } = 260;
    public bool InspectorVisible { get; init; } = true;
    public double InspectorWidth { get; init; } = 420;
    public double WindowWidth { get; init; }
    public LayoutBreakpoint Breakpoint { get; init; } = LayoutBreakpoint.Wide;

    public bool SidebarSplitterVisible => SidebarVisible;
    public bool InspectorSplitterVisible => InspectorVisible;
}

public enum LayoutBreakpoint
{
    Compact,   // < 900px — sidebar + inspector hidden, flyout overlays
    Medium,    // 900-1280px — sidebar 220px, inspector 320px
    Wide,      // 1280-1600px — sidebar 260px, inspector 420px
    ExtraWide  // 1600px+ — sidebar 300px, inspector 520px
}
