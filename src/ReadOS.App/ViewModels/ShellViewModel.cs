using System;
using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ReadOS.App.Models;
using ReadOS.App.Services;

namespace ReadOS.App.ViewModels;

public partial class ShellViewModel : ObservableObject
{
    private readonly WorkspaceSeed workspace;

    [ObservableProperty]
    public partial double SidebarWidth { get; set; } = 320;

    [ObservableProperty]
    public partial bool IsSettingsOpen { get; set; }

    [ObservableProperty]
    public partial AppStrings Strings { get; set; } = LocalizationCatalog.GetStrings("zh-CN");

    [ObservableProperty]
    public partial LanguageOption? SelectedLanguageOption { get; set; }

    [ObservableProperty]
    public partial NavigationEntry? SelectedNavigationEntry { get; set; }

    [ObservableProperty]
    public partial ProjectItem? SelectedProject { get; set; }

    [ObservableProperty]
    public partial SessionItem? SelectedSession { get; set; }

    [ObservableProperty]
    public partial string SearchQuery { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ComposerDraft { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string StatusMessage { get; set; } = "就绪。";

    [ObservableProperty]
    public partial string ProviderName { get; set; } = "OpenAI Compatible";

    [ObservableProperty]
    public partial string ProviderBaseUrl { get; set; } = "https://api.openai.com/v1";

    [ObservableProperty]
    public partial string ProviderApiKey { get; set; } = string.Empty;

    [ObservableProperty]
    public partial string ModelName { get; set; } = "gpt-4.1";

    [ObservableProperty]
    public partial string AttachmentDefaultPrompt { get; set; } = "请按书本顺序解释我附加的页面。";

    [ObservableProperty]
    public partial string RegionExplainPrompt { get; set; } = "请结合上下文，重点讲解红框内容。";

    [ObservableProperty]
    public partial string ChapterExplainPrompt { get; set; } = "请围绕我附加的这一整节内容进行系统讲解。";

    [ObservableProperty]
    public partial string MinorUEndpoint { get; set; } = "https://mineru.net/api";

    [ObservableProperty]
    public partial bool UseMockResponses { get; set; } = true;

    public ShellViewModel(IWorkspaceService workspaceService)
    {
        workspace = workspaceService.CreateSeed();

        LanguageOptions.Add(new LanguageOption { Code = "zh-CN", DisplayName = "中文" });
        LanguageOptions.Add(new LanguageOption { Code = "en-US", DisplayName = "English" });
        SelectedLanguageOption = LanguageOptions.First();

        RefreshProjects();
        SelectProject(workspace.Projects.First());
    }

    public ObservableCollection<LanguageOption> LanguageOptions { get; } = new();

    public ObservableCollection<ProjectItem> Projects { get; } = new();

    public ObservableCollection<NavigationEntry> NavigationEntries { get; } = new();

    public ObservableCollection<ActivityItem> ActivityStream { get; } = new();

    public ObservableCollection<MaterialItem> ActiveMaterials { get; } = new();

    public string ActiveTitle => SelectedSession?.Title ?? SelectedProject?.Name ?? "ReadOS";

    public string ActiveSubtitle => SelectedProject is null
        ? "选择一个项目开始"
        : $"{SelectedProject.Name} · {SelectedProject.Description}";

    public string ActiveModelLabel => string.Format(Strings.ModelLabelFormat, UseMockResponses ? "mock provider" : ModelName);

    public string MaterialsSummary => $"{ActiveMaterials.Count} 个资料";

    partial void OnSelectedLanguageOptionChanged(LanguageOption? value)
    {
        if (value is null)
        {
            return;
        }

        Strings = LocalizationCatalog.GetStrings(value.Code);
        NotifyLocalizedProperties();
        StatusMessage = value.Code == "zh-CN" ? "已切换到中文界面。" : "Switched to English.";
    }

    partial void OnSelectedNavigationEntryChanged(NavigationEntry? value)
    {
        if (value is null)
        {
            return;
        }

        var project = workspace.Projects.FirstOrDefault(item => item.Id == value.ProjectId);
        if (project is null)
        {
            return;
        }

        if (value.Kind == NavigationEntryKind.Project)
        {
            SelectProject(project);
            return;
        }

        var session = project.Sessions.FirstOrDefault(item => item.Id == value.SessionId);
        if (session is not null)
        {
            SelectProject(project, session);
        }
    }

    partial void OnSearchQueryChanged(string value)
    {
        RefreshNavigation();
    }

    partial void OnUseMockResponsesChanged(bool value)
    {
        OnPropertyChanged(nameof(ActiveModelLabel));
    }

    partial void OnModelNameChanged(string value)
    {
        OnPropertyChanged(nameof(ActiveModelLabel));
    }

    [RelayCommand]
    private void CreateProject()
    {
        var number = workspace.Projects.Count + 1;
        var project = new ProjectItem
        {
            Id = $"project-{number}",
            Name = $"新项目 {number}",
            Description = "新的阅读项目",
            UpdatedLabel = "刚刚"
        };

        var session = new SessionItem
        {
            Id = $"project-{number}-session-1",
            ProjectId = project.Id,
            Title = "启动阅读会话",
            UpdatedLabel = "刚刚"
        };
        session.Activities.Add(new ActivityItem
        {
            Id = $"project-{number}-welcome",
            Kind = ActivityKind.Text,
            Title = "新项目已创建",
            Subtitle = "下一步可以导入资料或直接提问",
            Body = "这是一个模拟项目。后续会接入真实资料库和 PDF 引擎。"
        });

        project.Sessions.Add(session);
        workspace.Projects.Add(project);
        RefreshProjects();
        SelectProject(project, session);
        StatusMessage = $"已创建项目：{project.Name}。";
    }

    [RelayCommand]
    private void StartSession()
    {
        if (SelectedProject is null)
        {
            CreateProject();
            return;
        }

        var number = SelectedProject.Sessions.Count + 1;
        var session = new SessionItem
        {
            Id = $"{SelectedProject.Id}-session-{number}",
            ProjectId = SelectedProject.Id,
            Title = $"新会话 {number}",
            UpdatedLabel = "刚刚"
        };
        session.Activities.Add(new ActivityItem
        {
            Id = $"{session.Id}-start",
            Kind = ActivityKind.Text,
            Title = "会话已开启",
            Subtitle = SelectedProject.Name,
            Body = "你可以在底部输入需求，或者先导入资料。"
        });

        SelectedProject.Sessions.Insert(0, session);
        RefreshNavigation();
        SelectProject(SelectedProject, session);
        StatusMessage = $"已开启会话：{session.Title}。";
    }

    [RelayCommand]
    private void ImportMaterial()
    {
        if (SelectedProject is null)
        {
            CreateProject();
        }

        if (SelectedProject is null)
        {
            return;
        }

        var number = SelectedProject.Materials.Count + 1;
        var material = new MaterialItem
        {
            Id = $"{SelectedProject.Id}-material-{number}",
            Name = $"导入资料 {number}.pdf",
            Kind = MaterialKind.Pdf,
            Detail = "资料 · PDF"
        };

        SelectedProject.Materials.Add(material);
        RefreshActiveMaterials();

        var session = SelectedSession ?? SelectedProject.Sessions.FirstOrDefault();
        if (session is null)
        {
            StartSession();
            session = SelectedSession;
        }

        if (session is not null)
        {
            var activity = new ActivityItem
            {
                Id = $"{session.Id}-material-{number}",
                Kind = ActivityKind.Materials,
                Title = "已导入资料",
                Subtitle = material.Name
            };
            activity.Materials.Add(material);
            session.Activities.Add(activity);
            SelectProject(SelectedProject, session);
        }

        StatusMessage = $"已导入资料：{material.Name}。";
    }

    [RelayCommand]
    private void SendPrompt()
    {
        if (SelectedProject is null)
        {
            CreateProject();
        }

        if (SelectedSession is null)
        {
            StartSession();
        }

        if (SelectedSession is null)
        {
            return;
        }

        var prompt = string.IsNullOrWhiteSpace(ComposerDraft)
            ? "请总结当前项目资料。"
            : ComposerDraft.Trim();

        SelectedSession.Activities.Add(new ActivityItem
        {
            Id = $"{SelectedSession.Id}-user-{SelectedSession.Activities.Count + 1}",
            Kind = ActivityKind.Text,
            Title = "你",
            Subtitle = "刚刚",
            Body = prompt
        });
        SelectedSession.Activities.Add(new ActivityItem
        {
            Id = $"{SelectedSession.Id}-answer-{SelectedSession.Activities.Count + 1}",
            Kind = ActivityKind.Answer,
            Title = "ReadOS",
            Subtitle = UseMockResponses ? "模拟回答" : $"{ProviderName} / {ModelName}",
            Body = UseMockResponses
                ? "已收到。真实 AI 接入后，这里会结合项目资料、当前会话和提示词生成回答。"
                : "真实模型调用尚未接入；设置已保存为后续服务层输入。"
        });

        ComposerDraft = string.Empty;
        RefreshActivityStream();
        StatusMessage = "已发送到当前会话。";
    }

    [RelayCommand]
    private void OpenSettings()
    {
        IsSettingsOpen = true;
        StatusMessage = "已打开设置。";
    }

    [RelayCommand]
    private void CloseSettings()
    {
        IsSettingsOpen = false;
        StatusMessage = "已关闭设置。";
    }

    [RelayCommand]
    private void SaveSettings()
    {
        IsSettingsOpen = false;
        OnPropertyChanged(nameof(ActiveModelLabel));
        StatusMessage = "设置已保存到当前会话。";
    }

    private void RefreshProjects()
    {
        Projects.Clear();
        foreach (var project in workspace.Projects)
        {
            Projects.Add(project);
        }

        RefreshNavigation();
    }

    private void RefreshNavigation()
    {
        var query = SearchQuery.Trim();
        NavigationEntries.Clear();

        foreach (var project in workspace.Projects)
        {
            var projectMatches = string.IsNullOrWhiteSpace(query) ||
                project.Name.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                project.Description.Contains(query, StringComparison.OrdinalIgnoreCase);
            var sessions = project.Sessions.Where(session => string.IsNullOrWhiteSpace(query) ||
                session.Title.Contains(query, StringComparison.OrdinalIgnoreCase) ||
                projectMatches).ToList();

            if (!projectMatches && sessions.Count == 0)
            {
                continue;
            }

            NavigationEntries.Add(new NavigationEntry
            {
                Id = project.Id,
                ProjectId = project.Id,
                Kind = NavigationEntryKind.Project,
                Title = project.Name,
                Detail = project.UpdatedLabel
            });

            foreach (var session in sessions)
            {
                NavigationEntries.Add(new NavigationEntry
                {
                    Id = session.Id,
                    ProjectId = project.Id,
                    SessionId = session.Id,
                    Kind = NavigationEntryKind.Session,
                    Title = session.Title,
                    Detail = session.UpdatedLabel
                });
            }
        }
    }

    private void SelectProject(ProjectItem project, SessionItem? session = null)
    {
        SelectedProject = project;
        SelectedSession = session ?? project.Sessions.FirstOrDefault();
        SelectedNavigationEntry = NavigationEntries.FirstOrDefault(entry =>
            entry.ProjectId == project.Id &&
            (SelectedSession is null ? entry.Kind == NavigationEntryKind.Project : entry.SessionId == SelectedSession.Id));
        RefreshActiveMaterials();
        RefreshActivityStream();
        NotifyActiveContext();
    }

    private void RefreshActiveMaterials()
    {
        ActiveMaterials.Clear();

        if (SelectedProject is null)
        {
            return;
        }

        foreach (var material in SelectedProject.Materials)
        {
            ActiveMaterials.Add(material);
        }

        OnPropertyChanged(nameof(MaterialsSummary));
    }

    private void RefreshActivityStream()
    {
        ActivityStream.Clear();

        if (SelectedSession is null)
        {
            return;
        }

        foreach (var activity in SelectedSession.Activities)
        {
            ActivityStream.Add(activity);
        }
    }

    private void NotifyLocalizedProperties()
    {
        NotifyActiveContext();
        OnPropertyChanged(nameof(ActiveModelLabel));
        OnPropertyChanged(nameof(MaterialsSummary));
    }

    private void NotifyActiveContext()
    {
        OnPropertyChanged(nameof(ActiveTitle));
        OnPropertyChanged(nameof(ActiveSubtitle));
    }
}
