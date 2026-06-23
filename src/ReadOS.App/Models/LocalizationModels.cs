namespace ReadOS.App.Models;

public sealed class LanguageOption
{
    public required string Code { get; init; }

    public required string DisplayName { get; init; }
}

public sealed class AppStrings
{
    public required string AppSubtitle { get; init; }
    public required string NewSession { get; init; }
    public required string Projects { get; init; }
    public required string Sessions { get; init; }
    public required string ImportMaterial { get; init; }
    public required string CreateProject { get; init; }
    public required string OpenLocation { get; init; }
    public required string ComposerPlaceholder { get; init; }
    public required string FullAccess { get; init; }
    public required string Review { get; init; }
    public required string FilesEdited { get; init; }
    public required string Library { get; init; }
    public required string Chat { get; init; }
    public required string CloseTab { get; init; }
    public required string Settings { get; init; }
    public required string Import { get; init; }
    public required string Search { get; init; }
    public required string SearchPlaceholder { get; init; }
    public required string MapPages { get; init; }
    public required string Outline { get; init; }
    public required string AttachPage { get; init; }
    public required string ExplainRegion { get; init; }
    public required string AttachRange { get; init; }
    public required string Reader { get; init; }
    public required string Previous { get; init; }
    public required string Next { get; init; }
    public required string ChatWarehouse { get; init; }
    public required string New { get; init; }
    public required string Conversation { get; init; }
    public required string Clear { get; init; }
    public required string AskPlaceholder { get; init; }
    public required string SendMockPrompt { get; init; }
    public required string Language { get; init; }
    public required string AiProvider { get; init; }
    public required string ProviderName { get; init; }
    public required string BaseUrl { get; init; }
    public required string ApiKey { get; init; }
    public required string ModelName { get; init; }
    public required string Prompts { get; init; }
    public required string AttachmentPrompt { get; init; }
    public required string RegionPrompt { get; init; }
    public required string ChapterPrompt { get; init; }
    public required string MinorU { get; init; }
    public required string MinorUEndpoint { get; init; }
    public required string UseMockResponses { get; init; }
    public required string SaveSettings { get; init; }
    public required string CloseSettings { get; init; }
    public required string DragHint { get; init; }
    public required string NoDocumentSelected { get; init; }
    public required string ChooseDocument { get; init; }
    public required string ReaderPreview { get; init; }
    public required string NoPdfLoaded { get; init; }
    public required string PdfEnginePlaceholder { get; init; }
    public required string NoPage { get; init; }
    public required string OpenPdfToBegin { get; init; }
    public required string NoPdfSelected { get; init; }
    public required string NoAttachments { get; init; }
    public required string AttachmentsReadyFormat { get; init; }
    public required string LibrarySummaryFormat { get; init; }
    public required string ModelLabelFormat { get; init; }
    public required string BookPage { get; init; }
    public required string PdfPage { get; init; }
}

public static class LocalizationCatalog
{
    public static AppStrings GetStrings(string languageCode)
    {
        return languageCode == "en-US" ? English : Chinese;
    }

    private static AppStrings Chinese { get; } = new()
    {
        AppSubtitle = "AI PDF 学习工作台",
        NewSession = "新对话",
        Projects = "项目",
        Sessions = "会话",
        ImportMaterial = "导入资料",
        CreateProject = "新建项目",
        OpenLocation = "打开位置",
        ComposerPlaceholder = "要求后续变更",
        FullAccess = "完全访问",
        Review = "审核",
        FilesEdited = "已编辑文件",
        Library = "资料库",
        Chat = "对话",
        CloseTab = "关闭标签",
        Settings = "设置",
        Import = "导入",
        Search = "搜索",
        SearchPlaceholder = "查找文件夹和 PDF",
        MapPages = "映射页码",
        Outline = "目录",
        AttachPage = "附加本页",
        ExplainRegion = "框选讲解",
        AttachRange = "附加范围",
        Reader = "阅读器",
        Previous = "上一页",
        Next = "下一页",
        ChatWarehouse = "对话仓库",
        New = "新建",
        Conversation = "对话",
        Clear = "清空",
        AskPlaceholder = "询问这份 PDF...",
        SendMockPrompt = "发送模拟提问",
        Language = "语言",
        AiProvider = "AI 服务商",
        ProviderName = "服务商名称",
        BaseUrl = "Base URL",
        ApiKey = "API Key",
        ModelName = "模型名称",
        Prompts = "提示词",
        AttachmentPrompt = "附件默认提示词",
        RegionPrompt = "框选讲解提示词",
        ChapterPrompt = "章节讲解提示词",
        MinorU = "MinorU",
        MinorUEndpoint = "MinorU 地址",
        UseMockResponses = "使用模拟回答",
        SaveSettings = "保存设置",
        CloseSettings = "关闭",
        DragHint = "拖动分隔条可调整各区域宽度",
        NoDocumentSelected = "未选择文档",
        ChooseDocument = "从资料库选择一份文档",
        ReaderPreview = "阅读器预览",
        NoPdfLoaded = "未加载 PDF",
        PdfEnginePlaceholder = "PDF 引擎占位",
        NoPage = "无页面",
        OpenPdfToBegin = "打开 PDF 后开始阅读。",
        NoPdfSelected = "未选择 PDF",
        NoAttachments = "暂无附件",
        AttachmentsReadyFormat = "{0} 个附件待发送",
        LibrarySummaryFormat = "{0} 个项目",
        ModelLabelFormat = "模型：{0}",
        BookPage = "书本页",
        PdfPage = "PDF 页"
    };

    private static AppStrings English { get; } = new()
    {
        AppSubtitle = "AI PDF Workspace",
        NewSession = "New Session",
        Projects = "Projects",
        Sessions = "Sessions",
        ImportMaterial = "Import Material",
        CreateProject = "Create Project",
        OpenLocation = "Open Location",
        ComposerPlaceholder = "Ask for follow-up changes",
        FullAccess = "Full access",
        Review = "Review",
        FilesEdited = "Files edited",
        Library = "Library",
        Chat = "Chat",
        CloseTab = "Close Tab",
        Settings = "Settings",
        Import = "Import",
        Search = "Search",
        SearchPlaceholder = "Find folders and PDFs",
        MapPages = "Map Pages",
        Outline = "Outline",
        AttachPage = "Attach Page",
        ExplainRegion = "Explain Region",
        AttachRange = "Attach Range",
        Reader = "Reader",
        Previous = "Previous",
        Next = "Next",
        ChatWarehouse = "Chat Warehouse",
        New = "New",
        Conversation = "Conversation",
        Clear = "Clear",
        AskPlaceholder = "Ask about this PDF...",
        SendMockPrompt = "Send Mock Prompt",
        Language = "Language",
        AiProvider = "AI Provider",
        ProviderName = "Provider name",
        BaseUrl = "Base URL",
        ApiKey = "API Key",
        ModelName = "Model name",
        Prompts = "Prompts",
        AttachmentPrompt = "Attachment prompt",
        RegionPrompt = "Region prompt",
        ChapterPrompt = "Chapter prompt",
        MinorU = "MinorU",
        MinorUEndpoint = "MinorU endpoint",
        UseMockResponses = "Use mock responses",
        SaveSettings = "Save Settings",
        CloseSettings = "Close",
        DragHint = "Drag splitters to resize panes",
        NoDocumentSelected = "No document selected",
        ChooseDocument = "Choose a document from the library",
        ReaderPreview = "Reader preview",
        NoPdfLoaded = "No PDF loaded",
        PdfEnginePlaceholder = "PDF engine placeholder",
        NoPage = "No page",
        OpenPdfToBegin = "Open a PDF to begin.",
        NoPdfSelected = "No PDF selected",
        NoAttachments = "No attachments",
        AttachmentsReadyFormat = "{0} attachment(s) ready",
        LibrarySummaryFormat = "{0} item(s)",
        ModelLabelFormat = "Model: {0}",
        BookPage = "Book page",
        PdfPage = "PDF page"
    };
}
