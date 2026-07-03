using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;

namespace ReadOS.App.Controls;

public sealed partial class SplitPane : UserControl
{
    public static readonly DependencyProperty PrimaryContentProperty =
        DependencyProperty.Register(nameof(PrimaryContent), typeof(object), typeof(SplitPane), new PropertyMetadata(null));

    public static readonly DependencyProperty SecondaryContentProperty =
        DependencyProperty.Register(nameof(SecondaryContent), typeof(object), typeof(SplitPane), new PropertyMetadata(null));

    public static readonly DependencyProperty SecondaryWidthProperty =
        DependencyProperty.Register(nameof(SecondaryWidth), typeof(double), typeof(SplitPane), new PropertyMetadata(320.0, OnPaneMetricsChanged));

    public static readonly DependencyProperty SecondaryMinWidthProperty =
        DependencyProperty.Register(nameof(SecondaryMinWidth), typeof(double), typeof(SplitPane), new PropertyMetadata(200.0));

    public static readonly DependencyProperty SecondaryVisibleProperty =
        DependencyProperty.Register(nameof(SecondaryVisible), typeof(bool), typeof(SplitPane), new PropertyMetadata(true, OnPaneMetricsChanged));

    public static readonly DependencyProperty DragInvertProperty =
        DependencyProperty.Register(nameof(DragInvert), typeof(bool), typeof(SplitPane), new PropertyMetadata(false));

    public SplitPane()
    {
        InitializeComponent();
    }

    public object PrimaryContent
    {
        get => GetValue(PrimaryContentProperty);
        set => SetValue(PrimaryContentProperty, value);
    }

    public object SecondaryContent
    {
        get => GetValue(SecondaryContentProperty);
        set => SetValue(SecondaryContentProperty, value);
    }

    public double SecondaryWidth
    {
        get => (double)GetValue(SecondaryWidthProperty);
        set => SetValue(SecondaryWidthProperty, value);
    }

    public double SecondaryMinWidth
    {
        get => (double)GetValue(SecondaryMinWidthProperty);
        set => SetValue(SecondaryMinWidthProperty, value);
    }

    public bool SecondaryVisible
    {
        get => (bool)GetValue(SecondaryVisibleProperty);
        set => SetValue(SecondaryVisibleProperty, value);
    }

    /// <summary>
    /// When true, dragging the splitter adjusts the PRIMARY column width instead
    /// of the secondary column.  Useful when the splitter is on the left side
    /// of a secondary panel (e.g. sidebar).
    /// </summary>
    public bool DragInvert
    {
        get => (bool)GetValue(DragInvertProperty);
        set => SetValue(DragInvertProperty, value);
    }

    private static void OnPaneMetricsChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        ((SplitPane)d).ApplyColumns();
    }

    private void SplitterThumb_DragDelta(object sender, DragDeltaEventArgs e)
    {
        var proposed = DragInvert
            ? SecondaryWidth - e.HorizontalChange
            : SecondaryWidth + e.HorizontalChange;

        if (proposed < SecondaryMinWidth / 2)
        {
            SecondaryVisible = false;
        }
        else
        {
            SecondaryVisible = true;
            SecondaryWidth = Math.Max(SecondaryMinWidth, proposed);
        }

        ApplyColumns();
    }

    private void ApplyColumns()
    {
        if (SecondaryVisible)
        {
            SecondaryColumn.Width = new GridLength(SecondaryWidth);
            SplitterColumn.Width = new GridLength(1);
        }
        else
        {
            SecondaryColumn.Width = new GridLength(0);
            SplitterColumn.Width = new GridLength(0);
        }
    }
}
