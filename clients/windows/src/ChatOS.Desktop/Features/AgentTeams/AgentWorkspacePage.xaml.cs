using System.Collections.ObjectModel;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.DesignSystem;
using ChatOS.Presentation.AgentTeams;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class AgentWorkspacePage : Page
{
    private readonly WindowsLocalAgentArtifactClient _artifactClient;
    private AgentWorkspaceSection _section = AgentWorkspaceSection.Management;
    private string? _artifactNextCursor;
    private bool _isLoadingArtifacts;

    public AgentWorkspacePage(
        AgentTeamWorkspaceViewModel viewModel,
        MainWindowViewModel shell,
        WindowsLocalAgentArtifactClient artifactClient)
    {
        ViewModel = viewModel;
        Shell = shell;
        _artifactClient = artifactClient;
        InitializeComponent();
        ViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        ViewModel.Agents.CollectionChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        ViewModel.Rooms.CollectionChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        Shell.Projects.CollectionChanged += (_, _) => DispatcherQueue.TryEnqueue(() => Bindings.Update());
        RefreshState();
    }

    public event EventHandler<ProjectFeatureRequestedEventArgs>? FeatureRequested;
    public event EventHandler? OpenSkillManagementRequested;

    public AgentTeamWorkspaceViewModel ViewModel { get; }
    public MainWindowViewModel Shell { get; }
    public ObservableCollection<AgentArtifactItemViewModel> ArtifactItems { get; } = [];
    public string SelectedRoomName => ViewModel.SelectedRoom?.Draft.Name ?? "选择一个 Agent 开始私聊";
    public string SelectedRoomGoal => ViewModel.SelectedRoom?.Draft.Goal ?? "";

    private async void OnRefreshClick(object sender, RoutedEventArgs e)
    {
        await IgnoreFailureAsync(() => ViewModel.RefreshAsync());
        if (_section == AgentWorkspaceSection.Documents) await LoadArtifactsAsync(reset: true);
    }

    private async void OnLoadEarlierMessagesClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(ViewModel.LoadEarlierMessagesAsync);

    private async void OnRunAgentsClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(ViewModel.DrainAsync);

    private async void OnAgentClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is AgentProfile profile)
        {
            await IgnoreFailureAsync(() => ViewModel.OpenDirectAsync(profile.Id));
            ShowSection(AgentWorkspaceSection.DirectChat);
        }
    }

    private async void OnRoomSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (ViewModel.SelectedRoom is null) return;
        await IgnoreFailureAsync(() => ViewModel.SelectRoomAsync(ViewModel.SelectedRoom));
        ShowSection(AgentWorkspaceSection.DirectChat);
    }

    private async void OnSendClick(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(ViewModel.MessageText) && !ViewModel.HasPendingAttachments) return;
        await IgnoreFailureAsync(() => ViewModel.SendMessageAsync());
    }

    private async void OnNewAgentClick(object sender, RoutedEventArgs e) =>
        await ShowAgentDialogAsync(null);

    private void OnSkillManagementClick(object sender, RoutedEventArgs e) =>
        OpenSkillManagementRequested?.Invoke(this, EventArgs.Empty);

    private void OnAgentGridSizeChanged(object sender, SizeChangedEventArgs e)
    {
        if (sender is not GridView { ItemsPanelRoot: ItemsWrapGrid panel } grid) return;
        var available = Math.Max(280, grid.ActualWidth - 12);
        var columns = Math.Max(1, (int)Math.Floor((available + 14) / 294));
        panel.ItemWidth = Math.Clamp(Math.Floor(available / columns), 280, 420);
    }

    private async void OnCreateTeamClick(object sender, RoutedEventArgs e)
    {
        var boundProjectIds = ViewModel.TeamRooms
            .Select(value => value.ProjectId)
            .ToHashSet(StringComparer.Ordinal);
        var availableProjects = Shell.Projects
            .Where(value => !boundProjectIds.Contains(value.Id))
            .ToArray();
        if (availableProjects.Length == 0)
        {
            await ShowAlertAsync("无法创建团队", "没有尚未创建 Agent 团队的项目。请先新建项目，或打开已有团队。");
            return;
        }
        if (ViewModel.Agents.Count == 0)
        {
            await ShowAlertAsync("需要 Agent", "请先创建一个 Agent，作为新团队的项目经理。");
            return;
        }

        var project = new ComboBox
        {
            Header = "项目",
            ItemsSource = availableProjects,
            DisplayMemberPath = nameof(ShellResourceViewModel.Title),
            SelectedIndex = 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var manager = new ComboBox
        {
            Header = "项目经理",
            ItemsSource = ViewModel.Agents,
            DisplayMemberPath = "Draft.Name",
            SelectedIndex = 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var name = new TextBox
        {
            Header = "团队名称",
            Text = $"{availableProjects[0].Title} Agent 团队",
        };
        var goal = new TextBox
        {
            Header = "团队目标（可选）",
            AcceptsReturn = true,
            MinHeight = 90,
            TextWrapping = TextWrapping.Wrap,
        };
        project.SelectionChanged += (_, _) =>
        {
            if (project.SelectedItem is ShellResourceViewModel selected)
                name.Text = $"{selected.Title} Agent 团队";
        };
        var panel = new StackPanel { MinWidth = 440, Spacing = 10 };
        foreach (var child in new UIElement[] { project, name, goal, manager })
            panel.Children.Add(child);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "创建 Agent 团队",
            Content = panel,
            PrimaryButtonText = "创建团队",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary ||
            project.SelectedItem is not ShellResourceViewModel selectedProject ||
            manager.SelectedItem is not AgentProfile selectedManager)
            return;
        if (string.IsNullOrWhiteSpace(name.Text))
        {
            await ShowAlertAsync("团队名称不能为空", "请输入团队名称后重试。");
            return;
        }
        await IgnoreFailureAsync(() => ViewModel.CreateTeamForProjectAsync(
            selectedProject.Id,
            name.Text,
            goal.Text,
            selectedManager.Id));
    }

    private async void OnEditAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentProfile profile })
            await ShowAgentDialogAsync(profile);
    }

    private void OnShowAgentManagementClick(object sender, RoutedEventArgs e) =>
        ShowSection(AgentWorkspaceSection.Management);

    private async void OnShowAgentDocumentsClick(object sender, RoutedEventArgs e)
    {
        ShowSection(AgentWorkspaceSection.Documents);
        await LoadArtifactsAsync(reset: true);
    }

    private async void OnRefreshArtifactsClick(object sender, RoutedEventArgs e) =>
        await LoadArtifactsAsync(reset: true);

    private async void OnLoadMoreArtifactsClick(object sender, RoutedEventArgs e) =>
        await LoadArtifactsAsync(reset: false);

    private async void OnArchiveAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentProfile profile }) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "归档 Agent",
            Content = new TextBlock
            {
                Text = $"归档“{profile.Draft.Name}”？现有消息会保留。",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = "归档",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
            await IgnoreFailureAsync(() => ViewModel.ArchiveAgentAsync(profile));
    }

    private async void OnAddAttachmentClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var window = (Application.Current as App)?.MainWindow ??
                throw new InvalidOperationException("无法找到当前窗口。");
            var picker = new FileOpenPicker
            {
                ViewMode = PickerViewMode.List,
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            };
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker,
                WinRT.Interop.WindowNative.GetWindowHandle(window));
            var attachments = new List<AgentMessageAttachment>();
            foreach (var file in await picker.PickMultipleFilesAsync())
            {
                using var stream = await file.OpenReadAsync();
                if (stream.Size is 0 or > 20 * 1024 * 1024)
                    throw new InvalidOperationException($"附件“{file.Name}”为空或超过 20 MB。");
                var bytes = new byte[(int)stream.Size];
                using var reader = new DataReader(stream.GetInputStreamAt(0));
                await reader.LoadAsync((uint)stream.Size);
                reader.ReadBytes(bytes);
                var mime = string.IsNullOrWhiteSpace(file.ContentType)
                    ? "application/octet-stream"
                    : file.ContentType;
                attachments.Add(new AgentMessageAttachment(
                    Guid.NewGuid().ToString("D").ToLowerInvariant(),
                    file.Name,
                    mime,
                    AttachmentKind(mime),
                    bytes.LongLength,
                    bytes));
            }
            ViewModel.AddAttachments(attachments);
        }
        catch (Exception exception)
        {
            await ShowAlertAsync("无法添加附件", exception.Message);
        }
    }

    private void OnRemoveAttachmentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentMessageAttachment attachment })
            ViewModel.RemoveAttachment(attachment);
    }

    private async void OnDownloadAttachmentClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentMessageAttachment metadata }) return;
        try
        {
            await AgentMessageAttachmentPresenter.SaveAsync(ViewModel, metadata);
        }
        catch (Exception exception)
        {
            await ShowAlertAsync("无法保存附件", exception.Message);
        }
    }

    private async void OnPreviewAttachmentClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentMessageAttachment metadata }) return;
        try
        {
            await AgentMessageAttachmentPresenter.PreviewAsync(XamlRoot, ViewModel, metadata);
        }
        catch (Exception exception)
        {
            await ShowAlertAsync("无法预览附件", exception.Message);
        }
    }

    private async void OnDirectAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentProfile profile }) return;
        await IgnoreFailureAsync(() => ViewModel.OpenDirectAsync(profile.Id));
        ShowSection(AgentWorkspaceSection.DirectChat);
    }

    private async void OnPreviewArtifactClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentArtifactItemViewModel item }) return;
        try
        {
            var bytes = await LoadArtifactDataAsync(item);
            var markdown = new UTF8Encoding(false, true).GetString(bytes);
            var dialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = item.Name,
                Content = new ScrollViewer
                {
                    Content = new MarkdownRichTextBlock { Markdown = markdown },
                    Width = 720,
                    MaxHeight = 620,
                    HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled,
                    VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                },
                CloseButtonText = "完成",
            };
            _ = await dialog.ShowAsync();
        }
        catch (Exception exception)
        {
            await ShowAlertAsync("无法预览本地 Agent 文档", exception.Message);
        }
    }

    private async void OnSaveArtifactClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentArtifactItemViewModel item }) return;
        try
        {
            var bytes = await LoadArtifactDataAsync(item);
            var window = (Application.Current as App)?.MainWindow ??
                throw new InvalidOperationException("无法找到当前窗口。");
            var extension = Path.GetExtension(item.Name);
            if (string.IsNullOrWhiteSpace(extension)) extension = ".md";
            var picker = new FileSavePicker
            {
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
                SuggestedFileName = Path.GetFileNameWithoutExtension(item.Name),
            };
            picker.FileTypeChoices.Add("Agent 文档", [extension]);
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker,
                WinRT.Interop.WindowNative.GetWindowHandle(window));
            var file = await picker.PickSaveFileAsync();
            if (file is not null) await Windows.Storage.FileIO.WriteBytesAsync(file, bytes);
        }
        catch (Exception exception)
        {
            await ShowAlertAsync("无法保存本地 Agent 文档", exception.Message);
        }
    }

    private async void OnTeamClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentRoom room }) return;
        var project = Shell.Projects.FirstOrDefault(value =>
            string.Equals(value.Id, room.ProjectId, StringComparison.Ordinal));
        if (project is null)
        {
            await ShowAlertAsync("项目不可用", "这个团队绑定的项目已经移除，无法打开团队工作区。");
            return;
        }
        FeatureRequested?.Invoke(this, new ProjectFeatureRequestedEventArgs(project, "agent-team"));
    }

    private async Task LoadArtifactsAsync(bool reset)
    {
        if (_isLoadingArtifacts || !reset && _artifactNextCursor is null) return;
        _isLoadingArtifacts = true;
        RefreshArtifactState();
        ArtifactNoticeBar.IsOpen = false;
        try
        {
            var owner = ViewModel.OwnerUserId ??
                throw new InvalidOperationException("请先登录后再读取本地 Agent 文档。");
            var page = await _artifactClient.ListAsync(
                owner,
                limit: 50,
                cursor: reset ? null : _artifactNextCursor);
            if (reset) ArtifactItems.Clear();
            var existingIds = ArtifactItems
                .Select(item => item.ArtifactId)
                .ToHashSet(StringComparer.Ordinal);
            foreach (var artifact in page.Artifacts)
            {
                if (existingIds.Add(artifact.ArtifactId))
                    ArtifactItems.Add(new AgentArtifactItemViewModel(artifact));
            }
            _artifactNextCursor = page.NextCursor;
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception exception)
        {
            if (reset)
            {
                ArtifactItems.Clear();
                _artifactNextCursor = null;
            }
            ArtifactNoticeBar.Message = exception.Message.Contains(
                "optional local agent host is not installed",
                StringComparison.OrdinalIgnoreCase)
                ? "未安装可选的 Local Agent Host，本地文档暂不可用。"
                : exception.Message;
            ArtifactNoticeBar.IsOpen = true;
        }
        finally
        {
            _isLoadingArtifacts = false;
            RefreshArtifactState();
        }
    }

    private void RefreshArtifactState()
    {
        var hasItems = ArtifactItems.Count > 0;
        ArtifactRefreshButton.IsEnabled = !_isLoadingArtifacts;
        ArtifactList.IsEnabled = !_isLoadingArtifacts;
        ArtifactList.Visibility = hasItems ? Visibility.Visible : Visibility.Collapsed;
        ArtifactLoadingState.Visibility = _isLoadingArtifacts && !hasItems
            ? Visibility.Visible
            : Visibility.Collapsed;
        ArtifactProgress.IsActive = _isLoadingArtifacts && !hasItems;
        ArtifactEmptyState.Visibility = !_isLoadingArtifacts && !hasItems
            ? Visibility.Visible
            : Visibility.Collapsed;
        ArtifactLoadMoreButton.Visibility = hasItems && _artifactNextCursor is not null
            ? Visibility.Visible
            : Visibility.Collapsed;
        ArtifactLoadMoreButton.IsEnabled = !_isLoadingArtifacts;
        ArtifactLoadMoreButton.Content = _isLoadingArtifacts ? "正在加载…" : "加载更多";
    }

    private async Task<byte[]> LoadArtifactDataAsync(AgentArtifactItemViewModel item)
    {
        var owner = ViewModel.OwnerUserId ??
            throw new InvalidOperationException("请先登录后再读取本地 Agent 文档。");
        var bytes = await _artifactClient.ReadAsync(owner, item.ArtifactId);
        var sha256 = Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
        if ((ulong)bytes.LongLength != item.Size ||
            !string.Equals(sha256, item.Sha256, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidDataException("本地文档的大小或 SHA-256 与元数据不一致。");
        }
        return bytes;
    }

    private async Task ShowAgentDialogAsync(AgentProfile? profile)
    {
        var name = new TextBox { Header = "名称", Text = profile?.Draft.Name ?? "" };
        var description = new TextBox
        {
            Header = "说明",
            Text = profile?.Draft.Description ?? "",
            TextWrapping = TextWrapping.Wrap,
        };
        var prompt = new TextBox
        {
            Header = "角色提示词",
            Text = profile?.Draft.RolePrompt ?? "",
            AcceptsReturn = true,
            MinHeight = 150,
            TextWrapping = TextWrapping.Wrap,
        };
        var model = new ComboBox
        {
            Header = "模型",
            ItemsSource = ViewModel.Models,
            DisplayMemberPath = nameof(ConversationModelOption.DisplayName),
            SelectedItem = ViewModel.Models.FirstOrDefault(value => value.Id == profile?.Draft.ModelConfigId) ??
                ViewModel.Models.FirstOrDefault(),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var thinking = new ComboBox
        {
            Header = "推理强度",
            ItemsSource = new[] { "auto", "none", "low", "medium", "high", "xhigh" },
            SelectedItem = profile?.Draft.ThinkingLevel ?? "auto",
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var profession = new TextBox
        {
            Header = "职业 Key",
            Text = profile?.Draft.ProfessionKey ?? "general",
        };
        var plugins = new TextBox
        {
            Header = "默认插件 ID（逗号分隔）",
            Text = string.Join(", ", profile?.Draft.Plugins ?? []),
        };
        var skills = new TextBox
        {
            Header = "默认 Skill / 权限 ID（逗号分隔）",
            Text = string.Join(", ", profile?.Draft.Skills ?? []),
        };
        var staffManagement = new CheckBox
        {
            Content = "允许发起团队成员新增、入队和移出提案",
            IsChecked = profile is not null && AgentProfilePermissions.CanManageStaff(profile),
        };
        var heartbeat = new CheckBox
        {
            Content = "启用主动巡检",
            IsChecked = profile?.Draft.HeartbeatEnabled ?? false,
        };
        var heartbeatInterval = new NumberBox
        {
            Header = "巡检间隔（秒）",
            Minimum = 60,
            Maximum = 86_400,
            Value = profile?.Draft.HeartbeatIntervalSeconds ?? 900,
        };
        var heartbeatPrompt = new TextBox
        {
            Header = "巡检提示词",
            Text = profile?.Draft.HeartbeatPrompt ?? string.Empty,
            AcceptsReturn = true,
            MinHeight = 90,
            TextWrapping = TextWrapping.Wrap,
        };
        var panel = new StackPanel { Spacing = 10, MinWidth = 440 };
        foreach (var child in new UIElement[]
        {
            name, description, prompt, model, thinking, profession, plugins, skills,
            staffManagement, heartbeat, heartbeatInterval, heartbeatPrompt,
        })
            panel.Children.Add(child);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = profile is null ? "新建 Agent" : "编辑 Agent",
            Content = new ScrollViewer
            {
                Content = panel,
                MaxHeight = 650,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            },
            PrimaryButtonText = "保存",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary ||
            model.SelectedItem is not ConversationModelOption selectedModel) return;
        var normalizedSkills = AgentProfilePermissions.NormalizeStaffPermissions(
            SplitIdentifiers(skills.Text), staffManagement.IsChecked == true);
        var draft = new AgentProfileDraft(
            name.Text.Trim(),
            description.Text.Trim(),
            prompt.Text.Trim(),
            selectedModel.Id,
            thinking.SelectedItem?.ToString(),
            profession.Text.Trim(),
            SplitIdentifiers(plugins.Text),
            normalizedSkills,
            heartbeat.IsChecked == true,
            (int)heartbeatInterval.Value,
            heartbeatPrompt.Text.Trim());
        await IgnoreFailureAsync(() => ViewModel.SaveAgentAsync(profile?.Id, draft));
    }

    private static IReadOnlyList<string> SplitIdentifiers(string value) => value
        .Split([',', ';', '\n', '\r'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
        .Distinct(StringComparer.Ordinal)
        .ToArray();

    private void RefreshState()
    {
        Bindings.Update();
        AgentGrid.Visibility = ViewModel.HasAgents ? Visibility.Visible : Visibility.Collapsed;
        AgentEmptyState.Visibility = ViewModel.HasAgents ? Visibility.Collapsed : Visibility.Visible;
        NoticeBar.IsOpen = !string.IsNullOrWhiteSpace(ViewModel.ErrorMessage) ||
            ViewModel.IsBusy;
        NoticeBar.Severity = ViewModel.ErrorMessage is null
            ? InfoBarSeverity.Informational
            : InfoBarSeverity.Error;
        NoticeBar.Message = ViewModel.ErrorMessage ??
            (ViewModel.IsBusy ? "正在同步 Agent 工作区…" : ViewModel.StatusMessage);
    }

    private void ShowSection(AgentWorkspaceSection section)
    {
        _section = section;
        AgentManagementPanel.Visibility = section == AgentWorkspaceSection.Management
            ? Visibility.Visible
            : Visibility.Collapsed;
        AgentDocumentsPanel.Visibility = section == AgentWorkspaceSection.Documents
            ? Visibility.Visible
            : Visibility.Collapsed;
        DirectChatPanel.Visibility = section == AgentWorkspaceSection.DirectChat
            ? Visibility.Visible
            : Visibility.Collapsed;
        AgentManagementNavigationButton.Opacity = section == AgentWorkspaceSection.Management
            ? 1
            : 0.72;
        AgentDocumentsNavigationButton.Opacity = section == AgentWorkspaceSection.Documents
            ? 1
            : 0.72;
        Bindings.Update();
    }

    private async Task ShowAlertAsync(string title, string message)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap },
            CloseButtonText = "关闭",
        };
        _ = await dialog.ShowAsync();
    }

    private static AgentMessageAttachmentKind AttachmentKind(string mimeType)
    {
        if (mimeType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
            return AgentMessageAttachmentKind.Image;
        if (mimeType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase))
            return AgentMessageAttachmentKind.Audio;
        return AgentMessageAttachmentKind.File;
    }

    private static async Task IgnoreFailureAsync(Func<Task> action)
    {
        try { await action(); }
        catch (OperationCanceledException) { }
        catch { }
    }

    private enum AgentWorkspaceSection
    {
        Management,
        Documents,
        DirectChat,
    }
}

public sealed class AgentArtifactItemViewModel(WindowsLocalAgentArtifactItem artifact)
{
    public string ArtifactId { get; } = artifact.ArtifactId;
    public string Name { get; } = artifact.Name;
    public ulong Size { get; } = artifact.Size;
    public string Sha256 { get; } = artifact.Sha256;
    public string MetadataLabel { get; } =
        $"{FormatSize(artifact.Size)} · {FormatDate(artifact.CreatedAtUnixMs)}";
    public string HashLabel { get; } =
        $"SHA-256 {artifact.Sha256[..Math.Min(12, artifact.Sha256.Length)]}…";

    private static string FormatSize(ulong bytes)
    {
        string[] units = ["B", "KB", "MB", "GB"];
        var value = (double)bytes;
        var unit = 0;
        while (value >= 1024 && unit < units.Length - 1)
        {
            value /= 1024;
            unit++;
        }
        return unit == 0 ? $"{value:0} {units[unit]}" : $"{value:0.#} {units[unit]}";
    }

    private static string FormatDate(long unixMs) =>
        DateTimeOffset.FromUnixTimeMilliseconds(unixMs).ToLocalTime()
            .ToString("yyyy-MM-dd HH:mm");
}
