using ReadOS.App.Models;

namespace ReadOS.App.Services;

public sealed class MockWorkspaceService : IWorkspaceService
{
    public WorkspaceSeed CreateSeed()
    {
        var seed = new WorkspaceSeed();

        var reados = CreateReadOsProject();
        var math = CreateMathProject();
        var story = CreateStoryProject();

        seed.Projects.Add(reados);
        seed.Projects.Add(math);
        seed.Projects.Add(story);

        return seed;
    }

    private static ProjectItem CreateReadOsProject()
    {
        var project = new ProjectItem
        {
            Id = "reados",
            Name = "reados",
            Description = "AI PDF 阅读器 MVP",
            UpdatedLabel = "1 小时"
        };

        project.Materials.Add(new MaterialItem
        {
            Id = "goal",
            Name = "PRODUCT_GOAL.md",
            Kind = MaterialKind.Markdown,
            Detail = "产品目标 · MD"
        });
        project.Materials.Add(new MaterialItem
        {
            Id = "mvp",
            Name = "INITIAL_MVP.md",
            Kind = MaterialKind.Markdown,
            Detail = "MVP 规划 · MD"
        });

        var session = new SessionItem
        {
            Id = "understand-docs",
            ProjectId = project.Id,
            Title = "理解应用文档",
            UpdatedLabel = "1 小时"
        };

        session.Activities.Add(new ActivityItem
        {
            Id = "summary",
            Kind = ActivityKind.Text,
            Title = "目标",
            Subtitle = "建立紧凑项目工作台",
            Body = "ReadOS 现在以项目为核心组织资料、会话与后续 AI 阅读动作。"
        });
        session.Activities.Add(new ActivityItem
        {
            Id = "command",
            Kind = ActivityKind.Code,
            Title = "你现在可以：",
            Subtitle = "powershell",
            Body = "git pull\n.\\scripts\\run.ps1"
        });
        session.Activities.Add(CreateMaterialActivity(project));
        session.Activities.Add(new ActivityItem
        {
            Id = "changes",
            Kind = ActivityKind.Changes,
            Title = "已编辑 8 个文件",
            Subtitle = "+528 -78",
            Badge = "待审核"
        });
        session.Activities[^1].FileChanges.Add(new FileChangeItem { Path = "README.md", Added = 1, Removed = 1 });
        session.Activities[^1].FileChanges.Add(new FileChangeItem { Path = "docs/INITIAL_MVP.md", Added = 3, Removed = 0 });
        session.Activities[^1].FileChanges.Add(new FileChangeItem { Path = "src/ReadOS.App/MainWindow.xaml", Added = 120, Removed = 58 });

        project.Sessions.Add(session);
        return project;
    }

    private static ProjectItem CreateMathProject()
    {
        var project = new ProjectItem
        {
            Id = "math",
            Name = "math",
            Description = "整理高数 PPT 复习资料",
            UpdatedLabel = "2 天"
        };

        project.Materials.Add(new MaterialItem
        {
            Id = "math-pdf",
            Name = "高等数学讲义.pdf",
            Kind = MaterialKind.Pdf,
            Detail = "资料 · PDF"
        });

        project.Sessions.Add(new SessionItem
        {
            Id = "math-review",
            ProjectId = project.Id,
            Title = "整理高数PPT复习资料",
            UpdatedLabel = "2 天"
        });

        project.Sessions[0].Activities.Add(CreateMaterialActivity(project));
        return project;
    }

    private static ProjectItem CreateStoryProject()
    {
        var project = new ProjectItem
        {
            Id = "uni-story",
            Name = "Uni-Story",
            Description = "阅读项目进程",
            UpdatedLabel = "1 周"
        };

        project.Sessions.Add(new SessionItem
        {
            Id = "ui-progress",
            ProjectId = project.Id,
            Title = "阅读项目进程 ui_button_ring.tscn...",
            UpdatedLabel = "1 周"
        });
        project.Sessions.Add(new SessionItem
        {
            Id = "mcp-check",
            ProjectId = project.Id,
            Title = "帮我查看 mcp 是否已连接，系统...",
            UpdatedLabel = "1 周"
        });

        project.Sessions[0].Activities.Add(new ActivityItem
        {
            Id = "story-note",
            Kind = ActivityKind.Text,
            Title = "项目记录",
            Subtitle = "模拟会话",
            Body = "这里展示项目会话如何被保留在左侧项目下。"
        });

        return project;
    }

    private static ActivityItem CreateMaterialActivity(ProjectItem project)
    {
        var activity = new ActivityItem
        {
            Id = $"materials-{project.Id}",
            Kind = ActivityKind.Materials,
            Title = "已导入资料",
            Subtitle = $"{project.Materials.Count} 个文件"
        };

        foreach (var material in project.Materials)
        {
            activity.Materials.Add(material);
        }

        return activity;
    }
}
