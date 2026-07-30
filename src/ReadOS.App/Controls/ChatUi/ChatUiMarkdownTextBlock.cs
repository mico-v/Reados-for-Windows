using System.Text;
using System.Text.RegularExpressions;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using ReadOS.App.Models.ChatUi;

namespace ReadOS.App.Controls.ChatUi;

// Lightweight markdown renderer for ChatUiMarkdownBlock text.
//
// The Default renderer manifest declares `markstream-readex-fade` with KaTeX,
// Codex fade animation, and rich code theming. Re-implementing that browser
// runtime in XAML is out of scope. This control renders the common markdown
// subset (headings, paragraphs, bold/italic/code spans, lists, blockquotes,
// fenced code, tables, hr) into a RichTextBlock. Streaming blocks render a
// trailing caret glyph so live output reads as "still typing".
//
// The parser is intentionally small and forgiving. It is not CommonMark — it
// covers the subset the ReadOS projection service emits plus the conformance
// fixtures under MSP/Implementations/UI/MSPChatUI/Conformance/fixtures.

public sealed class ChatUiMarkdownTextBlock : UserControl
{
    public static readonly DependencyProperty MarkdownProperty =
        DependencyProperty.Register(
            nameof(Markdown),
            typeof(string),
            typeof(ChatUiMarkdownTextBlock),
            new PropertyMetadata(string.Empty, OnMarkdownChanged));

    public static readonly DependencyProperty StreamingProperty =
        DependencyProperty.Register(
            nameof(Streaming),
            typeof(bool),
            typeof(ChatUiMarkdownTextBlock),
            new PropertyMetadata(false, OnMarkdownChanged));

    public string Markdown
    {
        get => (string)GetValue(MarkdownProperty);
        set => SetValue(MarkdownProperty, value);
    }

    public bool Streaming
    {
        get => (bool)GetValue(StreamingProperty);
        set => SetValue(StreamingProperty, value);
    }

    private readonly RichTextBlock _root = new()
    {
        HorizontalAlignment = HorizontalAlignment.Stretch,
        TextWrapping = TextWrapping.Wrap,
        TextTrimming = TextTrimming.CharacterEllipsis,
        IsTextSelectionEnabled = true,
        LineHeight = 22,
        FontSize = (double)Application.Current.Resources["MspBodyFontSize"]
    };

    // Guards the one-shot markstream-readex-fade entrance so streaming
    // deltas (which re-invoke Render) do not re-trigger the fade.
    private bool _hasFadedIn;

    // Shared mono font for code and math treatments (vitesse/∑ styling).
    private static readonly FontFamily MonoFont = new("Cascadia Code, Consolas, Courier New");

    public ChatUiMarkdownTextBlock()
    {
        Content = _root;
    }

    private static void OnMarkdownChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        ((ChatUiMarkdownTextBlock)d).Render();
    }

    private void Render()
    {
        var root = _root;
        root.Blocks.Clear();

        var text = Markdown ?? string.Empty;
        if (string.IsNullOrEmpty(text))
        {
            // Streaming block with no content yet still shows a caret so the
            // user sees live activity, matching the shimmer slot in Default.
            if (Streaming)
            {
                root.Blocks.Add(NewParagraphWithRuns(new[] { StreamingRun() }));
            }
        }
        else
        {
            foreach (var block in MarkdownParser.Parse(text))
            {
                AddBlock(root, block);
            }

            if (Streaming)
            {
                if (root.Blocks.Count > 0 && root.Blocks[root.Blocks.Count - 1] is Paragraph last)
                {
                    last.Inlines.Add(new Run { Text = " " });
                    last.Inlines.Add(StreamingRun());
                }
                else
                {
                    root.Blocks.Add(NewParagraphWithRuns(new[] { StreamingRun() }));
                }
            }
        }

        // markstream-readex-fade: a single soft entrance for the block. The
        // flag prevents re-fading on every streamed text delta.
        if (!_hasFadedIn && root.Blocks.Count > 0)
        {
            _hasFadedIn = true;
            PlayFadeIn();
        }
    }

    private void PlayFadeIn()
    {
        // Snap to transparent first so there is no one-frame flash before the
        // storyboard begins.
        _root.Opacity = 0;
        var storyboard = new Storyboard();
        var animation = new DoubleAnimation
        {
            From = 0.0,
            To = 1.0,
            Duration = new Duration(TimeSpan.FromMilliseconds(220))
        };
        Storyboard.SetTarget(animation, _root);
        Storyboard.SetTargetProperty(animation, "Opacity");
        storyboard.Children.Add(animation);
        storyboard.Begin();
    }

    private static Run StreamingRun() => new()
    {
        Text = "▍",
        Foreground = (Brush)Application.Current.Resources["ReadOSAccentBrush"],
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold
    };

    private static Paragraph NewParagraphWithRuns(IEnumerable<Inline> runs)
    {
        var p = new Paragraph();
        foreach (var run in runs) p.Inlines.Add(run);
        return p;
    }

    private void AddBlock(RichTextBlock root, MarkdownBlock block)
    {
        switch (block)
        {
            case HeadingBlock h:
                root.Blocks.Add(NewParagraphWithRuns(new[]
                {
                    new Run
                    {
                        Text = h.Text,
                        FontSize = h.Level switch { 1 => 22, 2 => 19, 3 => 16, _ => 14 },
                        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                        Foreground = (Brush)Application.Current.Resources["ReadOSTextPrimaryBrush"]
                    }
                }));
                break;

            case ParagraphBlock p:
                root.Blocks.Add(BuildParagraph(p));
                break;

            case ListBlock l:
                AddList(root, l);
                break;

            case CodeBlock c:
                root.Blocks.Add(BuildCodeBlock(c));
                break;

            case BlockMathBlock m:
                root.Blocks.Add(BuildBlockMath(m));
                break;

            case BlockquoteBlock q:
                root.Blocks.Add(BuildBlockquote(q));
                break;

            case TableBlock t:
                AddTable(root, t);
                break;

            case HrBlock:
                root.Blocks.Add(new Paragraph
                {
                    Inlines = { new Run { Text = new string('—', 24), Foreground = (Brush)Application.Current.Resources["ReadOSHairlineBrush"] } }
                });
                break;
        }
    }

    private static Paragraph BuildParagraph(ParagraphBlock p)
    {
        var paragraph = new Paragraph();
        foreach (var run in MarkdownParser.InlineRuns(p.InlineText))
        {
            paragraph.Inlines.Add(run);
        }
        return paragraph;
    }

    private void AddList(RichTextBlock root, ListBlock list)
    {
        var i = 0;
        foreach (var item in list.Items)
        {
            i++;
            var bullet = list.Ordered ? $"{i}. " : "• ";
            var paragraph = new Paragraph();
            paragraph.Inlines.Add(new Run
            {
                Text = bullet,
                Foreground = (Brush)Application.Current.Resources["ReadOSTextTertiaryBrush"]
            });
            foreach (var run in MarkdownParser.InlineRuns(item))
            {
                paragraph.Inlines.Add(run);
            }
            root.Blocks.Add(paragraph);
        }
    }

    private static Paragraph BuildCodeBlock(CodeBlock code)
    {
        // vitesse-style code panel: subtle themed background, hairline border,
        // rounded corners, language label header. Wrapped in an
        // InlineUIContainer so it flows inline within the RichTextBlock.
        // Inner text is a RichTextBlock whose Paragraph is filled with the
        // dependency-free vitesse tokenizer (comments / strings / numbers /
        // keywords) so it stays inside the same Border and keeps wrapping.
        var panel = new StackPanel { Spacing = 6 };

        if (!string.IsNullOrWhiteSpace(code.Language))
        {
            panel.Children.Add(new TextBlock
            {
                Text = code.Language,
                FontSize = 11,
                FontFamily = MonoFont,
                Foreground = (Brush)Application.Current.Resources["ReadOSTextTertiaryBrush"]
            });
        }

        var codeText = code.Text.Replace("\r\n", "\n").Replace('\r', '\n');
        var rtb = new RichTextBlock
        {
            FontSize = 13,
            FontFamily = MonoFont,
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true
        };
        rtb.Blocks.Add(BuildCodeParagraph(codeText));
        panel.Children.Add(rtb);

        var border = new Border
        {
            Background = (Brush)Application.Current.Resources["MspCodeBackgroundBrush"],
            BorderBrush = (Brush)Application.Current.Resources["ReadOSHairlineBrush"],
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(12, 10, 12, 10),
            Margin = new Thickness(0, 4, 0, 4),
            Child = panel
        };

        var paragraph = new Paragraph();
        paragraph.Inlines.Add(new InlineUIContainer { Child = border });
        return paragraph;
    }

    // Builds a Paragraph of colorized Runs for a code string. Robust: never
    // throws on odd input; any unrecognized span is emitted as default text.
    private static Paragraph BuildCodeParagraph(string code)
    {
        var paragraph = new Paragraph();
        foreach (var (text, kind) in TokenizeCode(code))
        {
            AppendCodeToken(paragraph, text, kind);
        }
        return paragraph;
    }

    private static void AppendCodeToken(Paragraph paragraph, string text, CodeToken kind)
    {
        var brush = kind switch
        {
            CodeToken.Comment => (Brush)Application.Current.Resources["ReadOSTextTertiaryBrush"],
            CodeToken.String => (Brush)Application.Current.Resources["ReadOSTextSecondaryBrush"],
            CodeToken.Number => (Brush)Application.Current.Resources["ReadOSTextPrimaryBrush"],
            CodeToken.Keyword => (Brush)Application.Current.Resources["ReadOSAccentBrush"],
            _ => (Brush)Application.Current.Resources["ReadOSTextPrimaryBrush"]
        };

        // A token may contain newlines (block comments, multiline strings,
        // whitespace runs). Split so each fragment wraps and a LineBreak is
        // inserted between logical lines.
        var parts = text.Split('\n');
        for (var k = 0; k < parts.Length; k++)
        {
            if (parts[k].Length > 0)
            {
                paragraph.Inlines.Add(new Run
                {
                    Text = parts[k],
                    FontFamily = MonoFont,
                    Foreground = brush
                });
            }
            if (k < parts.Length - 1)
            {
                paragraph.Inlines.Add(new LineBreak());
            }
        }
    }

    // Block math ($$...$$): a themed panel mirroring the code block style,
    // with a small "∑" language-style label and the formula in a mono run.
    // Dependency-free — shows the raw LaTeX source nicely formatted.
    private static Paragraph BuildBlockMath(BlockMathBlock math)
    {
        var panel = new StackPanel { Spacing = 6 };

        panel.Children.Add(new TextBlock
        {
            Text = "∑",
            FontSize = 11,
            FontFamily = MonoFont,
            Foreground = (Brush)Application.Current.Resources["ReadOSTextTertiaryBrush"]
        });

        panel.Children.Add(new TextBlock
        {
            Text = math.Formula,
            FontSize = 14,
            FontFamily = MonoFont,
            TextWrapping = TextWrapping.Wrap,
            Foreground = (Brush)Application.Current.Resources["ReadOSTextPrimaryBrush"]
        });

        var border = new Border
        {
            Background = (Brush)Application.Current.Resources["MspCodeBackgroundBrush"],
            BorderBrush = (Brush)Application.Current.Resources["ReadOSHairlineBrush"],
            BorderThickness = new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(12, 10, 12, 10),
            Margin = new Thickness(0, 4, 0, 4),
            Child = panel
        };

        var paragraph = new Paragraph();
        paragraph.Inlines.Add(new InlineUIContainer { Child = border });
        return paragraph;
    }

    // ── Dependency-free code tokenizer (vitesse) ─────────────────────────

    private enum CodeToken { Text, Comment, String, Number, Keyword }

    private static readonly HashSet<string> CodeKeywords = new(StringComparer.Ordinal)
    {
        "if", "else", "elif", "for", "while", "do", "return", "function", "fun", "def",
        "var", "let", "const", "class", "struct", "enum", "new", "int", "long", "float",
        "double", "bool", "boolean", "string", "void", "public", "private", "protected",
        "internal", "static", "true", "false", "null", "none", "nil", "this", "self",
        "import", "from", "as", "in", "of", "switch", "case", "break", "continue", "try",
        "catch", "finally", "throw", "async", "await", "type", "interface", "extends",
        "implements", "using", "namespace", "yield", "with", "not", "and", "or", "is"
    };

    // Single-pass scanner. Never throws; any unrecognized character is folded
    // into a Text token. Handles //, #, and /* */ comments, " and ' strings
    // (with escaping), integers/floats/hex, and identifier keywords.
    private static IEnumerable<(string Text, CodeToken Kind)> TokenizeCode(string code)
    {
        var i = 0;
        var n = code.Length;
        while (i < n)
        {
            var c = code[i];

            // Line comment: // or #
            if ((c == '/' && i + 1 < n && code[i + 1] == '/') || c == '#')
            {
                var start = i;
                while (i < n && code[i] != '\n') i++;
                yield return (code.Substring(start, i - start), CodeToken.Comment);
                continue;
            }

            // Block comment: /* ... */
            if (c == '/' && i + 1 < n && code[i + 1] == '*')
            {
                var start = i;
                i += 2;
                while (i < n && !(code[i] == '*' && i + 1 < n && code[i + 1] == '/')) i++;
                if (i < n) i += 2;
                yield return (code.Substring(start, i - start), CodeToken.Comment);
                continue;
            }

            // Strings
            if (c == '"' || c == '\'')
            {
                var q = c;
                var start = i;
                i++;
                while (i < n)
                {
                    if (code[i] == '\\' && i + 1 < n) { i += 2; continue; }
                    if (code[i] == q) { i++; break; }
                    i++;
                }
                yield return (code.Substring(start, i - start), CodeToken.String);
                continue;
            }

            // Numbers (incl. hex and floats)
            if (char.IsDigit(c) || (c == '.' && i + 1 < n && char.IsDigit(code[i + 1])))
            {
                var start = i;
                if (c == '0' && i + 1 < n && (code[i + 1] == 'x' || code[i + 1] == 'X'))
                {
                    i += 2;
                    while (i < n && IsHexDigit(code[i])) i++;
                }
                else
                {
                    while (i < n && (char.IsDigit(code[i]) || code[i] == '.' || code[i] == 'e' || code[i] == 'E' || code[i] == '+' || code[i] == '-')) i++;
                }
                yield return (code.Substring(start, i - start), CodeToken.Number);
                continue;
            }

            // Identifiers / keywords
            if (char.IsLetter(c) || c == '_')
            {
                var start = i;
                while (i < n && (char.IsLetterOrDigit(code[i]) || code[i] == '_')) i++;
                var word = code.Substring(start, i - start);
                yield return (word, CodeKeywords.Contains(word) ? CodeToken.Keyword : CodeToken.Text);
                continue;
            }

            // Other (whitespace, punctuation) — accumulate until a token
            // boundary so newlines stay inside a single splittable run.
            var s2 = i;
            while (i < n && !IsTokenStart(code, i)) i++;
            yield return (code.Substring(s2, i - s2), CodeToken.Text);
        }
    }

    private static bool IsTokenStart(string code, int i)
    {
        var c = code[i];
        if (c == '/' && i + 1 < code.Length && (code[i + 1] == '/' || code[i + 1] == '*')) return true;
        if (c == '#') return true;
        if (c == '"' || c == '\'') return true;
        if (char.IsDigit(c)) return true;
        if (char.IsLetter(c) || c == '_') return true;
        return false;
    }

    private static bool IsHexDigit(char c) =>
        char.IsDigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');

    private static Paragraph BuildBlockquote(BlockquoteBlock quote)
    {
        var paragraph = new Paragraph
        {
            TextIndent = 16
        };
        paragraph.Inlines.Add(new Run
        {
            Text = "│ ",
            Foreground = (Brush)Application.Current.Resources["MspBlockquoteBorderBrush"]
        });
        foreach (var run in MarkdownParser.InlineRuns(quote.Text))
        {
            paragraph.Inlines.Add(run);
        }
        return paragraph;
    }

    private void AddTable(RichTextBlock root, TableBlock table)
    {
        foreach (var row in table.Rows)
        {
            var paragraph = new Paragraph();
            var cellIndex = 0;
            foreach (var cell in row)
            {
                if (cellIndex > 0)
                {
                    paragraph.Inlines.Add(new Run
                    {
                        Text = "  ·  ",
                        Foreground = (Brush)Application.Current.Resources["ReadOSTextTertiaryBrush"]
                    });
                }
                var isHeader = cellIndex == 0 && table.Rows.IndexOf(row) == 0;
                paragraph.Inlines.Add(new Run
                {
                    Text = cell,
                    FontWeight = isHeader ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
                    Foreground = (Brush)Application.Current.Resources[isHeader ? "ReadOSTextPrimaryBrush" : "ReadOSTextSecondaryBrush"]
                });
                cellIndex++;
            }
            root.Blocks.Add(paragraph);
        }
    }
}

// ── Markdown parser ──────────────────────────────────────────────────────

internal abstract class MarkdownBlock { }
internal sealed class HeadingBlock : MarkdownBlock { public string Text = string.Empty; public int Level; }
internal sealed class ParagraphBlock : MarkdownBlock { public string InlineText = string.Empty; }
internal sealed class ListBlock : MarkdownBlock { public List<string> Items = new(); public bool Ordered; }
internal sealed class CodeBlock : MarkdownBlock { public string Text = string.Empty; public string? Language; }
internal sealed class BlockMathBlock : MarkdownBlock { public string Formula = string.Empty; }
internal sealed class BlockquoteBlock : MarkdownBlock { public string Text = string.Empty; }
internal sealed class TableBlock : MarkdownBlock { public List<List<string>> Rows = new(); }
internal sealed class HrBlock : MarkdownBlock { }

internal static class MarkdownParser
{
    private static readonly Regex FencedCode = new(@"^```(?<lang>[\w\+\-]*)\s*\n(?<body>.*?)\n```\s*$", RegexOptions.Singleline);
    private static readonly Regex Heading = new(@"^(?<hash>#{1,6})\s+(?<text>.+)$");
    private static readonly Regex UnorderedItem = new(@"^\s*[-*+]\s+(?<text>.*)$");
    private static readonly Regex OrderedItem = new(@"^\s*(?<n>\d+)\.\s+(?<text>.*)$");
    private static readonly Regex Hr = new(@"^\s*([-*_])\1{2,}\s*$");
    private static readonly Regex Blockquote = new(@"^\s*>\s?(?<text>.*)$");
    private static readonly Regex TableSplit = new(@"\|");

    public static IEnumerable<MarkdownBlock> Parse(string source)
    {
        var lines = source.Replace("\r\n", "\n").Split('\n');
        var i = 0;
        while (i < lines.Length)
        {
            var line = lines[i];

            // Block math $$...$$ (KaTeX-style, rendered dependency-free).
            // Either a single line `$$ formula $$` or a delimited block.
            if (line.StartsWith("$$", StringComparison.Ordinal))
            {
                var rest = line[2..].TrimEnd();
                string formula;
                if (rest.EndsWith("$$", StringComparison.Ordinal))
                {
                    formula = rest.Substring(0, rest.Length - 2).Trim();
                    i++;
                }
                else
                {
                    var sbm = new StringBuilder(rest);
                    i++;
                    while (i < lines.Length && !lines[i].Contains("$$"))
                    {
                        sbm.AppendLine(lines[i]);
                        i++;
                    }
                    if (i < lines.Length)
                    {
                        var close = lines[i];
                        var idx = close.IndexOf("$$", StringComparison.Ordinal);
                        if (idx >= 0) sbm.Append(close.Substring(0, idx));
                        i++;
                    }
                    formula = sbm.ToString().Trim();
                }
                yield return new BlockMathBlock { Formula = formula };
                continue;
            }

            // Fenced code block — collect until matching ```.
            if (line.StartsWith("```", StringComparison.Ordinal))
            {
                var lang = line[3..].Trim();
                var sb = new StringBuilder();
                i++;
                while (i < lines.Length && !lines[i].StartsWith("```", StringComparison.Ordinal))
                {
                    sb.AppendLine(lines[i]);
                    i++;
                }
                if (i < lines.Length) i++; // skip closing fence
                yield return new CodeBlock { Text = sb.ToString().TrimEnd('\n', '\r'), Language = lang };
                continue;
            }

            // Heading
            var m = Heading.Match(line);
            if (m.Success)
            {
                yield return new HeadingBlock { Text = m.Groups["text"].Value.Trim(), Level = m.Groups["hash"].Length };
                i++;
                continue;
            }

            // Horizontal rule
            if (Hr.IsMatch(line))
            {
                yield return new HrBlock();
                i++;
                continue;
            }

            // Blockquote (consecutive lines)
            var bq = Blockquote.Match(line);
            if (bq.Success)
            {
                var sb = new StringBuilder(bq.Groups["text"].Value);
                i++;
                while (i < lines.Length && Blockquote.Match(lines[i]).Success)
                {
                    sb.AppendLine();
                    sb.Append(Blockquote.Match(lines[i]).Groups["text"].Value);
                    i++;
                }
                yield return new BlockquoteBlock { Text = sb.ToString() };
                continue;
            }

            // List
            var ul = UnorderedItem.Match(line);
            var ol = OrderedItem.Match(line);
            if (ul.Success || ol.Success)
            {
                var ordered = ol.Success;
                var items = new List<string>();
                while (i < lines.Length)
                {
                    var cur = lines[i];
                    var um = UnorderedItem.Match(cur);
                    var om = OrderedItem.Match(cur);
                    if (um.Success) { items.Add(um.Groups["text"].Value); i++; continue; }
                    if (om.Success) { items.Add(om.Groups["text"].Value); i++; continue; }
                    if (string.IsNullOrWhiteSpace(cur)) { i++; break; }
                    break;
                }
                yield return new ListBlock { Items = items, Ordered = ordered };
                continue;
            }

            // Table — pipe-delimited row followed by --- separator
            if (line.Contains('|') && i + 1 < lines.Length && HrOnPipes(lines[i + 1]))
            {
                var rows = new List<List<string>>();
                rows.Add(SplitRow(line));
                i += 2; // skip header + separator
                while (i < lines.Length && lines[i].Contains('|'))
                {
                    rows.Add(SplitRow(lines[i]));
                    i++;
                }
                yield return new TableBlock { Rows = rows };
                continue;
            }

            // Blank line — paragraph separator
            if (string.IsNullOrWhiteSpace(line)) { i++; continue; }

            // Paragraph: gather until blank line or block starter
            var sbp = new StringBuilder(line);
            i++;
            while (i < lines.Length)
            {
                var cur = lines[i];
                if (string.IsNullOrWhiteSpace(cur)) break;
                if (cur.StartsWith("```")) break;
                if (Heading.Match(cur).Success) break;
                if (Hr.IsMatch(cur)) break;
                if (Blockquote.Match(cur).Success) break;
                if (UnorderedItem.Match(cur).Success) break;
                if (OrderedItem.Match(cur).Success) break;
                sbp.Append('\n').Append(cur);
                i++;
            }
            yield return new ParagraphBlock { InlineText = sbp.ToString() };
        }
    }

    private static bool HrOnPipes(string line)
    {
        if (string.IsNullOrWhiteSpace(line)) return false;
        var trimmed = line.Trim();
        if (trimmed.Contains('|')) trimmed = trimmed.Replace("|", "").Trim();
        if (trimmed.Length == 0) return true;
        return trimmed.All(c => c == '-' || c == '_' || c == '*' || c == ' ');
    }

    private static List<string> SplitRow(string line)
    {
        var trimmed = line.Trim().Trim('|');
        return TableSplit.Split(trimmed).Select(c => c.Trim()).ToList();
    }

    // Builds the inline runs for a paragraph — handles **bold**, *italic*,
    // `code`, [link](url), and plain text spans. Inline math ($...$) is left
    // as-is so users at least see the source string (KaTeX is not available).
    public static IEnumerable<Inline> InlineRuns(string source)
    {
        if (string.IsNullOrEmpty(source)) yield break;
        source = source.Replace("\n", " ");

        var pattern = new Regex(
            @"(\*\*(?<bold>.+?)\*\*)|(\*(?<italic>.+?)\*)|(`(?<code>.+?)`)|(\[(?<label>[^\]]+)\]\((?<url>[^)]+)\))|(?<!\$)\$(?!\$)(?<imath>[^$\n]+?)\$(?!\$)",
            RegexOptions.Compiled);

        var pos = 0;
        var accent = (Brush)Application.Current.Resources["ReadOSAccentBrush"];
        var secondary = (Brush)Application.Current.Resources["ReadOSTextSecondaryBrush"];

        foreach (Match match in pattern.Matches(source))
        {
            if (match.Index > pos)
            {
                yield return new Run { Text = source.Substring(pos, match.Index - pos) };
            }

            if (match.Groups["bold"].Success)
            {
                yield return new Run { Text = match.Groups["bold"].Value, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold };
            }
            else if (match.Groups["italic"].Success)
            {
                yield return new Run { Text = match.Groups["italic"].Value, FontStyle = Windows.UI.Text.FontStyle.Italic };
            }
            else if (match.Groups["code"].Success)
            {
                yield return new Run
                {
                    Text = match.Groups["code"].Value,
                    FontFamily = new FontFamily("Cascadia Code, Consolas, Courier New"),
                    Foreground = secondary
                };
            }
            else if (match.Groups["label"].Success)
            {
                var label = match.Groups["label"].Value;
                var url = match.Groups["url"].Value;
                var hyperlink = new Hyperlink
                {
                    NavigateUri = Uri.TryCreate(url, UriKind.Absolute, out var u) ? u : null
                };
                hyperlink.Inlines.Add(new Run { Text = label, Foreground = accent });
                yield return hyperlink;
            }
            else if (match.Groups["imath"].Success)
            {
                // Inline math $...$ — rendered as a distinct mono run with a
                // subtle accent. Shows the raw LaTeX source (no KaTeX typeset).
                yield return new Run
                {
                    Text = match.Groups["imath"].Value.Trim(),
                    FontFamily = new FontFamily("Cascadia Code, Consolas, Courier New"),
                    Foreground = accent
                };
            }

            pos = match.Index + match.Length;
        }

        if (pos < source.Length)
        {
            yield return new Run { Text = source.Substring(pos) };
        }
    }
}
