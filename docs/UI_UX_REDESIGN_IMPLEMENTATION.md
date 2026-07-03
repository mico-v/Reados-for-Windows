# UI-UX Redesign — 实施计划

## 实施顺序

Phase 按影响程度和风险排序，每个 Phase 独立可测试。

---

## Phase 1：消除挤压变形（最小改动，最大效果）

### 1.1 合并 Inspector 和 Presenter 为统一审查面板

当前 `MainWindow.xaml` 的主内容区（Row 1）结构：

```
Grid [3列]
├── SidebarColumn (292px)
├── Splitter (1px)
└── 主区域 [3列]
    ├── ChatColumn (ChatSurfaceView + 内嵌 Inspector)
    ├── Splitter (1px)
    └── PresenterColumn (PresenterSurfaceView + 内嵌 Drawer)
```

改为：

```
Grid [3列]
├── SidebarColumn (响应式宽)
├── Splitter (1px)
└── 主区域 [2列]
    ├── ChatColumn (*)          — 纯对话视图，无内嵌 Inspector
    └── InspectorColumn (响应式宽) — 统一的审查面板
```

**ChatSurfaceView 变更**：移除内部的 InspectorColumn 和 InspectorSplitterColumn，变成一个纯对话线程视图。

**PresenterSurfaceView 变更**：重命名为 `InspectorView`，将其内容迁移为 Inspector 标签页之一（新增"预览"标签页）。

### 1.2 MainWindow.xaml 改动

```xml
<Window>
    <Grid x:Name="RootShell" Background="{ThemeResource ReadOSCanvasBrush}">
        <Grid.RowDefinitions>
            <RowDefinition Height="48" />     <!-- 紧凑工具栏 -->
            <RowDefinition Height="*" />       <!-- 主内容 -->
            <RowDefinition Height="28" />      <!-- 紧凑状态栏 -->
        </Grid.RowDefinitions>

        <!-- 工具栏：去掉 hard-coded Padding 150 -->
        <Border Grid.Row="0" ...>
            <Grid Padding="12,0">
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="Auto" />
                    <ColumnDefinition Width="*" />
                    <ColumnDefinition Width="Auto" />
                </Grid.ColumnDefinitions>

                <!-- Brand -->
                <StackPanel Grid.Column="0" Orientation="Horizontal" Spacing="10">
                    <Border Width="28" Height="28" CornerRadius="6" Background="...">
                        <TextBlock Text="R" ... />
                    </Border>
                    <TextBlock Text="ReadOS" ... />
                </StackPanel>

                <!-- 标题栏可拖拽区域 -->
                <Grid Grid.Column="1" x:Name="TitleBarDragRegion">
                    <StackPanel Orientation="Horizontal" Spacing="8">
                        <!-- 状态指示器 -->
                        <Border Padding="6,3" CornerRadius="6" ...>
                            <StackPanel Orientation="Horizontal" Spacing="6">
                                <Ellipse Width="6" Height="6" Fill="..." />
                                <TextBlock Text="{Binding WorkspaceModeLabel}" FontSize="11" />
                            </StackPanel>
                        </Border>
                        <TextBlock Text="{Binding ActiveTitle}" FontSize="12" ... />
                    </StackPanel>
                </Grid>

                <!-- 工具栏按钮组 -->
                <StackPanel Grid.Column="2" Orientation="Horizontal" Spacing="4">
                    <Button Width="32" Height="32" ToolTipService.ToolTip="侧栏" Command="..." />
                    <Button Width="32" Height="32" ToolTipService.ToolTip="导入" Command="..." />
                    <Button Width="32" Height="32" ToolTipService.ToolTip="新建线程" Command="..." />
                    <!-- 分隔线 -->
                    <Border Width="1" Height="20" Background="..." Margin="4,0" />
                    <Button Width="32" Height="32" ToolTipService.ToolTip="主题" Command="..." />
                    <Button Width="32" Height="32" ToolTipService.ToolTip="设置" Command="..." />
                </StackPanel>
            </Grid>
        </Border>

        <!-- 主内容区域（响应式 3 栏） -->
        <Grid Grid.Row="1" x:Name="MainContentArea">
            <Grid.ColumnDefinitions>
                <ColumnDefinition x:Name="SidebarCol" Width="260" />
                <ColumnDefinition x:Name="SidebarSplitterCol" Width="1" />
                <ColumnDefinition x:Name="ThreadCol" Width="*" />
                <ColumnDefinition x:Name="InspectorSplitterCol" Width="1" />
                <ColumnDefinition x:Name="InspectorCol" Width="420" />
            </Grid.ColumnDefinitions>

            <!-- 侧栏 -->
            <views:SidebarView Grid.Column="0" />

            <!-- 侧栏分割器 -->
            <Grid Grid.Column="1">
                <Rectangle Width="1" Fill="..." HorizontalAlignment="Center" />
                <primitives:Thumb Width="10" HorizontalAlignment="Center"
                    DragDelta="SidebarSplitter_DragDelta" />
            </Grid>

            <!-- 对话线程 -->
            <views:ThreadView Grid.Column="2" />

            <!-- 审查分割器 -->
            <Grid Grid.Column="3">
                <Rectangle Width="1" Fill="..." HorizontalAlignment="Center" />
                <primitives:Thumb Width="10" HorizontalAlignment="Center"
                    DragDelta="InspectorSplitter_DragDelta" />
            </Grid>

            <!-- 审查面板（统一：上下文/运行/证据/附件/预览） -->
            <views:InspectorView Grid.Column="4" />
        </Grid>

        <!-- 状态栏 -->
        <Border Grid.Row="2" ...>
            <Grid Padding="10,0">
                <Grid.ColumnDefinitions>
                    <ColumnDefinition Width="*" />
                    <ColumnDefinition Width="Auto" />
                    <ColumnDefinition Width="Auto" />
                </Grid.ColumnDefinitions>
                <TextBlock Text="{Binding StatusMessage}" FontSize="11" ... />
                <TextBlock Grid.Column="1" Text="{Binding ActiveModelLabel}" FontSize="11" Margin="12,0,0,0" ... />
                <ProgressRing Grid.Column="2" Width="16" Height="16" IsActive="{Binding IsBusy}" Margin="8,0,0,0" />
            </Grid>
        </Border>
    </Grid>
</Window>
```

### 1.3 响应式布局逻辑（MainWindow.xaml.cs）

```csharp
public sealed partial class MainWindow : Window
{
    private const double SidebarMinWidth = 200;
    private const double SidebarDefaultWidth = 260;
    private const double InspectorMinWidth = 280;
    private const double InspectorDefaultWidth = 420;
    private const double CompactThreshold = 900;
    private const double MediumThreshold = 1280;

    private double lastSidebarWidth = SidebarDefaultWidth;
    private double lastInspectorWidth = InspectorDefaultWidth;

    public MainWindow()
    {
        InitializeComponent();
        // ... 初始化 ...
        RootShell.SizeChanged += RootShell_SizeChanged;
    }

    private void RootShell_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        ApplyResponsiveColumns(e.NewSize.Width);
    }

    private void ApplyResponsiveColumns(double windowWidth)
    {
        if (windowWidth < CompactThreshold)
        {
            // 紧凑模式：隐藏两个侧面板
            SetColumn(SidebarCol, 0);
            SetColumn(SidebarSplitterCol, 0);
            SetColumn(InspectorCol, 0);
            SetColumn(InspectorSplitterCol, 0);
        }
        else if (windowWidth < MediumThreshold)
        {
            // 中等模式：侧栏 220px，审查面板可折叠
            if (ViewModel.IsSidebarVisible)
                SetColumn(SidebarCol, Math.Min(220, lastSidebarWidth));
            else
                SetColumn(SidebarCol, 0);

            SetColumn(SidebarSplitterCol, ViewModel.IsSidebarVisible ? 1 : 0);

            if (ViewModel.IsInspectorVisible)
                SetColumn(InspectorCol, Math.Min(320, lastInspectorWidth));
            else
                SetColumn(InspectorCol, 0);

            SetColumn(InspectorSplitterCol, ViewModel.IsInspectorVisible ? 1 : 0);
        }
        else
        {
            // 宽屏模式：默认宽度
            if (ViewModel.IsSidebarVisible)
                SetColumn(SidebarCol, lastSidebarWidth);
            else
                SetColumn(SidebarCol, 0);

            SetColumn(SidebarSplitterCol, ViewModel.IsSidebarVisible ? 1 : 0);

            if (ViewModel.IsInspectorVisible)
                SetColumn(InspectorCol, lastInspectorWidth);
            else
                SetColumn(InspectorCol, 0);

            SetColumn(InspectorSplitterCol, ViewModel.IsInspectorVisible ? 1 : 0);
        }
    }

    private static void SetColumn(ColumnDefinition column, double width)
    {
        column.Width = new GridLength(width);
    }
}
```

---

## Phase 2：代码重构与组织

### 2.1 引入通用 SplitPane 控件

创建 `Controls/SplitPane.xaml` 封装拖拽分割逻辑：

```xml
<UserControl x:Class="ReadOS.App.Controls.SplitPane">
    <Grid>
        <Grid.ColumnDefinitions>
            <ColumnDefinition x:Name="PrimaryCol" Width="*" />
            <ColumnDefinition x:Name="SplitterCol" Width="1" />
            <ColumnDefinition x:Name="SecondaryCol" Width="320" />
        </Grid.ColumnDefinitions>

        <ContentPresenter Grid.Column="0" Content="{x:Bind PrimaryContent}" />
        <Grid Grid.Column="1" Background="Transparent">
            <Rectangle Width="1" Fill="{ThemeResource ReadOSBorderBrush}" ... />
            <primitives:Thumb Width="10" DragDelta="Thumb_DragDelta" />
        </Grid>
        <ContentPresenter Grid.Column="2" Content="{x:Bind SecondaryContent}" />
    </Grid>
</UserControl>
```

### 2.2 拆分 ViewModel

```
ShellViewModel (精简)
├── 路由: CurrentRoute (Home/Settings)
├── 主题: IsDarkTheme
├── 全局状态: StatusMessage, IsBusy, ActiveModelLabel
│
SidebarViewModel
├── 资料列表: NavigationEntries
├── 线程列表: Conversations
├── 搜索: SearchQuery
├── 模式: SidebarMode (Conversations/Materials)
├── 服务信息: ActiveModelLabel
│
ThreadViewModel
├── 消息流: ChatMessages
├── 编辑器: ComposerDraft
├── 附件: PendingAttachments
├── 命令: SendPrompt, AttachPage, AttachRange
│
InspectorViewModel
├── 标签页: InspectorTab (Context/Actions/Evidence/Attachments/Preview)
├── MSP 命令: MspCommandDraft, MspTranscript
├── 上下文: ActiveTitle, ActiveSubtitle
├── 文档操作: Rename, Delete, Export/Import
├── 预览: CurrentPageImage, PageNavigation, Thumbnails, Outline, Search
└── 布局: SetWorkspaceLayout
│
SettingsViewModel
├── 分类: SettingsCategory
├── 编辑: 各设置项
└── 保存: SaveSettings
```

### 2.3 DI 容器

`App.xaml.cs`:

```csharp
public partial class App : Application
{
    private ServiceProvider? serviceProvider;

    public App()
    {
        InitializeComponent();
        ConfigureServices();
    }

    private void ConfigureServices()
    {
        var services = new ServiceCollection();
        services.AddSingleton<IPdfDocumentService, PdfDocumentService>();
        services.AddSingleton<IWorkspaceStore, WorkspaceStore>();
        services.AddSingleton<IFileDialogService, FileDialogService>();
        services.AddSingleton<IAiChatService, AiChatService>();
        services.AddSingleton<ReadOsMspHost>();
        services.AddSingleton<ILayoutService, LayoutService>();

        // ViewModels
        services.AddTransient<ShellViewModel>();
        services.AddTransient<SidebarViewModel>();
        services.AddTransient<ThreadViewModel>();
        services.AddTransient<InspectorViewModel>();
        services.AddTransient<SettingsViewModel>();

        serviceProvider = services.BuildServiceProvider();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var window = new MainWindow(serviceProvider!.GetRequiredService<ShellViewModel>());
        window.Activate();
    }
}
```

---

## Phase 3：视觉刷新

### 3.1 主题色板优化

当前色板是合理的。建议的微调：

- **`ReadOSAccentBrush`**（Light: `#2563EB` → `#1D4ED8`）：略加深以提高可读性
- 增加 `ReadOSAccentMutedBrush`（Light: `#DBEAFE`）：用于选中状态背景
- 增加 `ReadOSDangerMutedBrush`（Light: `#FEE2E2`）：用于错误/拒绝状态背景

### 3.2 统一间距系统

定义资源键避免散布魔法数字：

```xml
<ResourceDictionary>
    <x:Double x:Key="SpacingXxs">4</x:Double>
    <x:Double x:Key="SpacingXs">6</x:Double>
    <x:Double x:Key="SpacingSm">8</x:Double>
    <x:Double x:Key="SpacingMd">12</x:Double>
    <x:Double x:Key="SpacingLg">16</x:Double>
    <x:Double x:Key="SpacingXl">20</x:Double>

    <x:Double x:Key="ControlHeightSm">28</x:Double>
    <x:Double x:Key="ControlHeightMd">34</x:Double>
    <x:Double x:Key="ControlHeightLg">40</x:Double>

    <x:Double x:Key="IconSizeSm">12</x:Double>
    <x:Double x:Key="IconSizeMd">14</x:Double>
    <x:Double x:Key="IconSizeLg">16</x:Double>

    <CornerRadius x:Key="RadiusSm">4</CornerRadius>
    <CornerRadius x:Key="RadiusMd">6</CornerRadius>
    <CornerRadius x:Key="RadiusLg">8</CornerRadius>

    <x:Double x:Key="FontSizeXs">11</x:Double>
    <x:Double x:Key="FontSizeSm">12</x:Double>
    <x:Double x:Key="FontSizeMd">14</x:Double>
    <x:Double x:Key="FontSizeLg">16</x:Double>
    <x:Double x:Key="FontSizeXl">18</x:Double>
</ResourceDictionary>
```

### 3.3 面板折叠动画

使用 `VisualStateManager` 实现平滑的面板展开/折叠：

```xml
<VisualStateManager.VisualStateGroups>
    <VisualStateGroup x:Name="SidebarStates">
        <VisualState x:Name="SidebarVisible">
            <VisualState.Setters>
                <Setter Target="SidebarCol.Width" Value="260" />
                <Setter Target="SidebarSplitterCol.Width" Value="1" />
            </VisualState.Setters>
        </VisualState>
        <VisualState x:Name="SidebarHidden">
            <VisualState.Setters>
                <Setter Target="SidebarCol.Width" Value="0" />
                <Setter Target="SidebarSplitterCol.Width" Value="0" />
            </VisualState.Setters>
        </VisualState>
    </VisualStateGroup>
</VisualStateManager.VisualStateGroups>
```

---

## Phase 4：具体文件变更清单

### 修改的文件

| 文件 | 变更 |
|------|------|
| `MainWindow.xaml` | 重写为 3 栏布局，移除 Padding 150，添加响应式断点 |
| `MainWindow.xaml.cs` | 移除手动拖拽逻辑，替换为统一 `LayoutService` 调用 |
| `App.xaml` | 添加间距/字号/圆角资源，添加 `SplitPane` 样式 |
| `App.xaml.cs` | 引入 DI 容器 |

### 新建的文件

| 文件 | 职责 |
|------|------|
| `Controls/SplitPane.xaml(.cs)` | 通用可拖拽分割面板 |
| `Views/ShellView.xaml(.cs)` | 顶层 Shell（包含 3 栏布局） |
| `Views/ThreadView.xaml(.cs)` | 对话线程视图（从 ChatSurfaceView 提取） |
| `Views/ComposerView.xaml(.cs)` | 消息编辑器（从 ChatSurfaceView 提取） |
| `Views/InspectorView.xaml(.cs)` | 统一审查面板（合并旧 Inspector + Presenter） |
| `ViewModels/SidebarViewModel.cs` | 侧栏 ViewModel |
| `ViewModels/ThreadViewModel.cs` | 对话线程 ViewModel |
| `ViewModels/InspectorViewModel.cs` | 审查面板 ViewModel |
| `ViewModels/SettingsViewModel.cs` | 设置 ViewModel |
| `ViewModels/ComposerViewModel.cs` | 编辑器 ViewModel |
| `Models/LayoutModels.cs` | 布局配置模型 |
| `Services/LayoutService.cs` | 布局管理服务 |

### 删除/合并的文件

| 文件 | 处理 |
|------|------|
| `Views/ChatSurfaceView.xaml(.cs)` | 拆分为 `ThreadView` + `ComposerView` |
| `Views/PresenterSurfaceView.xaml(.cs)` | 合并到 `InspectorView` |
| `Views/WorkspaceSidebarView.xaml(.cs)` | 重命名为 `SidebarView` |

---

## 风险与注意事项

1. **WinUI 3 的 VisualStateManager 对 GridLength 动画支持有限**。建议用 code-behind 直接设置 `ColumnDefinition.Width` 而非纯 XAML 动画。

2. **DI 容器引入需要测试**。确保 `ReadOsMspHost` 的多个 `Func<>` 委托在 DI 构造下正常工作。

3. **Inspector 标签页"预览"** 需要迁移 PresenterSurface 的 PDF 渲染逻辑，包括 `RegionCanvas` 的指针事件处理。

4. **向后兼容**：`ShellViewModel` 的属性被多处绑定引用，拆分 ViewModel 时应保留向后兼容的包装属性或使用 `x:Bind` 直接绑定子 ViewModel。
