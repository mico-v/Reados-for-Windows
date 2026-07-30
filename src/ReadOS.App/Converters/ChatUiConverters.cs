using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace ReadOS.App.Converters;

// Returns Visible when the bound string is non-empty, otherwise Collapsed.
// Used by ChatTimelineView block templates to hide optional text fields.

public sealed class StringToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return string.IsNullOrEmpty(value as string) ? Visibility.Collapsed : Visibility.Visible;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}

// Converts a nullable double to a progress value (0 when null) for ProgressBar.
public sealed class NullableDoubleConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return value is double d ? d : 0d;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}

// Returns Visible when the bound count (int or collection Count) is > 0.
public sealed class CountToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        var count = value switch
        {
            int i => i,
            System.Collections.ICollection c => c.Count,
            System.Collections.IEnumerable e => e.Cast<object>().Count(),
            _ => 0
        };
        return count > 0 ? Visibility.Visible : Visibility.Collapsed;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}

// Returns "running" styling brush when the bound status string equals Running/Pending/Streaming.
public sealed class ChatUiStatusToBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        // ThemeResource lookup happens via x:Key — return a string token the
        // template binds through a ChoiceStyle pattern. We return a token so
        // the XAML side picks the right StaticResource brush.
        return value?.ToString() switch
        {
            "Running" or "Pending" or "Streaming" => "Running",
            "Failed" => "Failed",
            "Cancelled" => "Cancelled",
            _ => "Success"
        };
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}

// Friendly label for the role chip on each message header.
public sealed class ChatUiRoleLabelConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return value?.ToString() switch
        {
            "User" => "你",
            "Assistant" => "助手",
            "Tool" => "工具",
            "System" => "系统",
            _ => "消息"
        };
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}

// Translates a duration in milliseconds into a short human string (e.g. "1.2s" or "320ms").
public sealed class ChatUiDurationLabelConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is not double ms || ms <= 0) return string.Empty;
        return ms switch
        {
            < 1000 => $"{(int)ms}ms",
            < 60_000 => $"{ms / 1000:F1}s",
            _ => $"{(int)(ms / 60_000)}m {(int)((ms % 60_000) / 1000)}s"
        };
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return DependencyProperty.UnsetValue;
    }
}
