# ReadOS UI-UX 重新设计报告

## 问题诊断

基于对 `src/ReadOS.App` 中所有 XAML、code-behind、ViewModel 和 Model 代码的完整审查，结合 `docs/UI_UX_DESIGN.md` 和 `docs/APP_FRAME_DESIGN.md` 的设计意图，识别出以下根因。

---

### 一、当前布局结构

```
MainWindow RootShell [Grid 3行]
├── Row 0: 顶部工具栏 (52px)
│   └── Grid 3列: [Brand 292px | TitleBar * | Buttons Auto] + 右侧硬编码 Padding 150
├── Row 1: 主内容 (*)
│   └── Grid 3列: [Sidebar 292px | Splitter 1px | 主区域 *]
│       └── 主区域 Padding="10" Grid 3列:
│           ├── ChatColumn (可变宽: Star 或 ChatWidth)
│           │   └── ChatSurfaceView (带自己的 Inspector 右面板 316px)
│           ├── PresenterSplitterColumn (1px)
│           └── PresenterColumn (可变宽: Star 或 520px)
│               └── PresenterSurfaceView (带自己的 ThumbnailDrawer 148px)
└── Row 2: 状态栏 (30px)
```

### 二、核心问题：实际是 5 层嵌套列，不是 3 栏

设计文档 (`UI_UX_DESIGN.md`) 描述的目标：

> 工作区侧栏 | 活跃对话线程 | 审查/证据检查器

但实际实现中存在 **4 个独立可调整的面板**：

| 面板 | 默认宽度 | 所在层级 |
|------|----------|----------|
| SidebarColumn | 292px | MainWindow.xaml |
| 主内容区 Chat 部分 | * (Star) | MainWindow.xaml |
| ChatSurface 内嵌 Inspector | 316px | ChatSurfaceView.xaml |
| PresenterColumn | 520px | MainWindow.xaml |
| Presenter 内嵌 ThumbnailDrawer | 148px | PresenterSurfaceView.xaml |

在 1920px 宽的窗口下：292 + 316 + 520 + 148 = 1276px（仅面板，不含 Chat 自身）。在 1366px 宽度的笔记本屏幕上，Chat 区域仅剩约 90px — 这是**挤压变形的直接原因**。

### 三、导致变形的具体代码问题

#### 3.1 硬编码像素值过多

```xml
<!-- MainWindow.xaml -->
<Grid Padding="12,0,150,0" ColumnSpacing="12">  <!-- 硬编码 Padding 150 -->
    <ColumnDefinition Width="292" />               <!-- Sidebar 固定宽 -->
</Grid>
<ColumnDefinition x:Name="PresenterColumn" Width="520" />  <!-- Presenter 固定宽 -->
```

`150px` 的右内边距是一个硬编码的补丁，意在给工具栏按钮让路，但造成了标题栏区域的不可预测空白。

```xml
<!-- ChatSurfaceView.xaml -->
<ColumnDefinition x:Name="InspectorColumn" Width="316" />  <!-- Inspector 固定宽 -->
```

```xml
<!-- PresenterSurfaceView.xaml -->
<ColumnDefinition x:Name="PresenterThumbnailColumn" Width="148" />  <!-- 缩略图抽屉固定宽 -->
```

这些值在窗口缩小时相互挤压 Chat 区域。

#### 3.2 多重嵌套的 CornerRadius 边框造成额外内边距

```xml
<!-- MainWindow.xaml Line 89 -->
<Grid Grid.Column="2" Padding="10" Background="{ThemeResource ReadOSCanvasBrush}">

<!-- ChatSurfaceView.xaml Line 11-14 -->
<Border ... BorderThickness="1" CornerRadius="8">
```

MainWindow 的 Padding=10 加上 ChatSurface 的 BorderThickness=1 加上 CornerRadius=8，在视觉上消费了大量边缘空间。

#### 3.3 布局模式切换逻辑脆弱

`MainWindow.xaml.cs` 中的 `ApplyWorkspaceSurfaceDrag` 方法通过拖拽检测来推断布局意图：

```csharp
if (proposedChat <= SurfaceFocusThreshold)
{
    ViewModel.ApplyWorkspaceLayoutPreset(WorkspaceLayoutMode.FocusChat);
}
```

这导致 4 种布局模式 (`ChatPrimary`、`PresenterPrimary`、`FocusChat`、`FocusPresenter`) 之间频繁跳跃，用户拖拽时容易意外触发模式切换。

#### 3.4 非响应式设计

整个应用没有断点逻辑。在 1366px 屏幕、125% DPI 缩放下，有效宽度约 1092px，所有面板同时可见时 Chat 实际宽度为负。

#### 3.5 Inspector 与 Presenter 角色重叠

`ChatSurfaceView` 内的 Inspector（上下文/运行/证据/附件）和 `PresenterSurfaceView`（PDF 预览）概念上都是"审查/证据面板"，但实现在两个不同层级，用户需要同时与两者交互。在 3 栏设计目标下，它们应该共享同一"审查"列。

### 四、代码组织问题

#### 4.1 God ViewModel

`ShellViewModel` 管理所有状态：工作区、导航、主题、侧栏、聊天、Presenter、设置、MSP 命令、对话、附件。这导致：
- 单个类的变更风险集中
- 业务逻辑与 UI 状态耦合
- 测试困难

#### 4.2 布局逻辑散落在 code-behind 中

三个 code-behind 文件各自处理拖拽缩放逻辑：
- `MainWindow.xaml.cs`: `SidebarResizeThumb_DragDelta`、`WorkspaceSurfaceResizeThumb_DragDelta`
- `ChatSurfaceView.xaml.cs`: `InspectorResizeThumb_DragDelta`
- `PresenterSurfaceView.xaml.cs`: `PresenterDrawerResizeThumb_DragDelta`

没有统一的布局管理。

#### 4.3 手动依赖注入

```csharp
// MainWindow.xaml.cs
var pdfService = new PdfDocumentService();
var workspaceStore = new WorkspaceStore(pdfService);
ViewModel = new ShellViewModel(workspaceStore, pdfService, new FileDialogService(), new AiChatService());
```

无 DI 容器，服务生命周期由 Window 管理。

---

## 重新设计方案

### 设计目标

1. **真正的 3 栏布局**：侧栏 → 对话线程 → 证据审查（将当前 4-5 栏合并为 3 栏）
2. **响应式伸缩**：基于窗口宽度自动适配
3. **面板可折叠但不挤压**：折叠时彻底隐藏，展开时提供合理的最小宽度保证
4. **减少嵌套层级**：扁平化视觉结构
5. **统一的布局管理**：将布局逻辑集中在服务层
6. **代码可维护性**：拆分 ViewModel，引入 DI 容器

### 新文件组织

```
src/ReadOS.App/
├── App.xaml(.cs)
├── MainWindow.xaml(.cs)              # 仅窗口级行为
├── Views/
│   ├── ShellView.xaml(.cs)           # 新：顶层 Shell 视图（替换 MainWindow 主内容）
│   ├── SidebarView.xaml(.cs)         # 重命名自 WorkspaceSidebarView
│   ├── ThreadView.xaml(.cs)          # 新：对话线程视图（拆分自 ChatSurfaceView）
│   ├── InspectorView.xaml(.cs)       # 新：统一审查面板（合并原 Inspector + Presenter）
│   ├── SettingsView.xaml(.cs)        # 保持，优化
│   └── ComposerView.xaml(.cs)        # 新：消息编辑器视图
├── ViewModels/
│   ├── ShellViewModel.cs             # 精简：仅路由和布局模式
│   ├── SidebarViewModel.cs           # 新
│   ├── ThreadViewModel.cs            # 新
│   ├── InspectorViewModel.cs         # 新
│   ├── SettingsViewModel.cs          # 新
│   └── ComposerViewModel.cs          # 新
├── Services/
│   ├── LayoutService.cs              # 新：统一布局管理
│   └── ...                           # 保持现有 Services
├── Controls/
│   ├── SplitPane.xaml(.cs)           # 新：通用可拖拽分割面板控件
│   └── StatusIndicator.xaml(.cs)     # 新：模型/会话状态指示器
└── Models/                           # 保持，增加 LayoutModels.cs
    ├── LayoutModels.cs               # 新：布局配置模型
    └── ...
```

### 新窗口布局

```
┌──────────────────────────────────────────────────────┐
│ [R] ReadOS  │ ● MSP Active │ Doc Title       │ ⚙ ☰  │  ← 标题栏 48px（集成工具栏）
├─────────────┼──────────────┼─────────────────┤      │
│ 工作区      │ 对话线程     │ 审查面板         │      │
│             │              │                 │      │
│ [搜索...]   │ ┌──────────┐ │ [上下文|运行|证据│      │
│             │ │ 消息1    │ │  附件|预览]     │      │
│ [线程|资料] │ │ 消息2    │ │                 │      │
│             │ │          │ │  PDF / 文本     │
│  ● 线程1    │ │          │ │  预览内容       │      │
│  ○ 线程2    │ │          │ │                 │      │
│             │ │          │ │                 │      │
│             │ ├──────────┤ │                 │      │
│ 本地服务    │ │ 编辑器   │ │                 │      │
│ ...         │ │ [发送]   │ │                 │      │
│             │ └──────────┘ │                 │      │
├─────────────┴──────────────┴─────────────────┤      │
│ Status: Ready │ Model: gpt-4.1-mini │ ⏳          │  ← 状态栏 28px
└──────────────────────────────────────────────────────┘
```

### 响应式断点

| 断点 | 窗口宽度 | 侧栏 | 对话 | 审查 |
|------|----------|------|------|------|
| Compact | < 900px | 隐藏（浮层） | 全宽 | 隐藏（浮层） |
| Medium | 900-1280px | 220px | * | 320px（可折叠） |
| Wide | 1280-1600px | 260px | * | 420px |
| ExtraWide | 1600px+ | 300px | * | 520px |

### 布局服务设计

```csharp
public interface ILayoutService
{
    LayoutConfiguration Current { get; }
    void ApplyConfiguration(LayoutConfiguration config);
    event EventHandler<LayoutConfiguration> ConfigurationChanged;
}

public record LayoutConfiguration
{
    public double SidebarWidth { get; init; }
    public bool SidebarVisible { get; init; }
    public double InspectorWidth { get; init; }
    public bool InspectorVisible { get; init; }
    public bool IsCompact { get; init; }
}
```

### 关键改动总结

1. **合并 Presenter 面板到 Inspector 的"预览"标签页**：消除 `MainWindow` 中 `ChatColumn | PresenterColumn` 的双列布局，将 PDF/文本预览放入 Inspector 统一管理
2. **用 `SplitPane` 自定义控件替换手动拖拽逻辑**：封装 `Thumb` + `DragDelta` 模式，减少 code-behind
3. **实现自适应断点**：在 `ShellView` 的 `SizeChanged` 事件中调用 `LayoutService` 调整列宽
4. **移除硬编码 Padding 150**：工具栏按钮使用自适应 `StackPanel` 与右侧对齐
5. **减少 Border 嵌套**：`CornerRadius` 仅用于卡片区域，不需要包裹整个面板
6. **拆分 ShellViewModel 为多个 ViewModel**：每个子视图拥有自己的 ViewModel，ShellViewModel 仅管理路由和布局
7. **引入 Microsoft.Extensions.DependencyInjection**：在 `App.xaml.cs` 中配置 DI
