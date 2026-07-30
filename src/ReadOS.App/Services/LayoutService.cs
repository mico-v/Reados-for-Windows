using System;
using ReadOS.App.Models;

namespace ReadOS.App.Services;

/// <summary>
/// Centralised layout service that computes column widths from window dimensions,
/// user visibility toggles, and remembered drag positions.  Code-behind files call
/// <see cref="ComputeConfiguration"/> on SizeChanged and drag events instead of
/// each owning its own width logic.
/// </summary>
public sealed class LayoutService
{
    // ── breakpoint thresholds ────────────────────────────────────────────────
    private const double CompactMax = 899;
    private const double MediumMax = 1279;
    private const double WideMax = 1599;

    // ── minimum widths that prevent collapsing below usable size ─────────────
    public const double SidebarMin = 200;
    public const double SidebarDefault = 280;
    public const double SidebarMax = 400;
    public const double InspectorMin = 280;
    public const double InspectorDefault = 420;
    public const double InspectorMax = 600;

    private double rememberedSidebarWidth = SidebarDefault;
    private double rememberedInspectorWidth = InspectorDefault;

    /// <summary>
    /// Gets the user's preferred sidebar width independently of the width that
    /// the current responsive breakpoint can display.
    /// </summary>
    public double RememberedSidebarWidth => rememberedSidebarWidth;

    /// <summary>
    /// Gets the user's preferred inspector width independently of the width that
    /// the current responsive breakpoint can display.
    /// </summary>
    public double RememberedInspectorWidth => rememberedInspectorWidth;

    public double RememberSidebarWidth(double width)
    {
        rememberedSidebarWidth = Clamp(width, SidebarMin, SidebarMax);
        return rememberedSidebarWidth;
    }

    public double RememberInspectorWidth(double width)
    {
        rememberedInspectorWidth = Clamp(width, InspectorMin, InspectorMax);
        return rememberedInspectorWidth;
    }

    public void RememberPaneWidths(double sidebarWidth, double inspectorWidth)
    {
        RememberSidebarWidth(sidebarWidth);
        RememberInspectorWidth(inspectorWidth);
    }

    /// <summary>
    /// Call on window SizeChanged and whenever a visibility toggle or drag completes.
    /// </summary>
    public LayoutConfiguration ComputeConfiguration(
        double windowWidth,
        bool sidebarRequested,
        bool inspectorRequested,
        double? dragSidebarWidth = null,
        double? dragInspectorWidth = null)
    {
        if (dragSidebarWidth.HasValue)
            RememberSidebarWidth(dragSidebarWidth.Value);

        if (dragInspectorWidth.HasValue)
            RememberInspectorWidth(dragInspectorWidth.Value);

        var breakpoint = ResolveBreakpoint(windowWidth);

        var (sidebarVisible, sidebarWidth) = ResolveSidebar(breakpoint, sidebarRequested);
        var (inspectorVisible, inspectorWidth) = ResolveInspector(breakpoint, inspectorRequested);

        return new LayoutConfiguration
        {
            WindowWidth = windowWidth,
            Breakpoint = breakpoint,
            SidebarVisible = sidebarVisible,
            SidebarWidth = sidebarWidth,
            InspectorVisible = inspectorVisible,
            InspectorWidth = inspectorWidth
        };
    }

    // ── internal helpers ─────────────────────────────────────────────────────

    private static LayoutBreakpoint ResolveBreakpoint(double windowWidth)
    {
        if (windowWidth <= CompactMax) return LayoutBreakpoint.Compact;
        if (windowWidth <= MediumMax) return LayoutBreakpoint.Medium;
        if (windowWidth <= WideMax) return LayoutBreakpoint.Wide;
        return LayoutBreakpoint.ExtraWide;
    }

    private (bool visible, double width) ResolveSidebar(LayoutBreakpoint breakpoint, bool requested)
    {
        return breakpoint switch
        {
            LayoutBreakpoint.Compact => (false, 0),
            LayoutBreakpoint.Medium => requested
                ? (true, Clamp(220, SidebarMin, rememberedSidebarWidth))
                : (false, 0),
            _ => requested
                ? (true, rememberedSidebarWidth)
                : (false, 0)
        };
    }

    private (bool visible, double width) ResolveInspector(LayoutBreakpoint breakpoint, bool requested)
    {
        return breakpoint switch
        {
            LayoutBreakpoint.Compact => (false, 0),
            LayoutBreakpoint.Medium => requested
                ? (true, Clamp(320, InspectorMin, rememberedInspectorWidth))
                : (false, 0),
            _ => requested
                ? (true, rememberedInspectorWidth)
                : (false, 0)
        };
    }

    private static double Clamp(double value, double min, double max)
    {
        return Math.Max(min, Math.Min(value, max));
    }
}
