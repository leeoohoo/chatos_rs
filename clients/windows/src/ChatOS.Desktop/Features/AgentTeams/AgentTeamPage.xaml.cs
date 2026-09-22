using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class AgentTeamPage : UserControl
{
    public AgentTeamPage(AgentTeamWorkspaceViewModel viewModel)
    {
        ViewModel = viewModel;
        ViewModel.PropertyChanged += (_, _) => Bindings.Update();
        InitializeComponent();
    }

    public AgentTeamWorkspaceViewModel ViewModel { get; }
    public string SelectedRoomName => ViewModel.SelectedRoom?.Draft.Name ?? "请选择团队或私聊";
    public string SelectedRoomGoal => ViewModel.SelectedRoom?.Draft.Goal ?? string.Empty;
    public string Notice => ViewModel.ErrorMessage ?? ViewModel.StatusMessage;
    public bool HasNotice => !string.IsNullOrWhiteSpace(Notice);
    public InfoBarSeverity NoticeSeverity => ViewModel.ErrorMessage is null
        ? InfoBarSeverity.Informational
        : InfoBarSeverity.Error;

    private async void OnRefreshClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(() => ViewModel.RefreshAsync());

    private async void OnRunAgentsClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(ViewModel.DrainAsync);

    private async void OnRoomSelectionChanged(object sender, SelectionChangedEventArgs e) =>
        await IgnoreFailureAsync(() => ViewModel.SelectRoomAsync(ViewModel.SelectedRoom));

    private async void OnNewAgentClick(object sender, RoutedEventArgs e) =>
        await ShowAgentDialogAsync(null);

    private async void OnEditAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentProfile profile })
            await ShowAgentDialogAsync(profile);
    }

    private async void OnArchiveAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentProfile profile }) return;
        if (await ConfirmAsync("归档 Agent", $"归档“{profile.Draft.Name}”？团队成员关系会同时移除。"))
            await IgnoreFailureAsync(() => ViewModel.ArchiveAgentAsync(profile));
    }

    private async void OnNewTeamClick(object sender, RoutedEventArgs e)
    {
        if (!RequireAgents()) return;
        var name = new TextBox { Header = "团队名称" };
        var goal = new TextBox { Header = "团队目标", AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        var manager = AgentPicker("项目经理", ViewModel.Agents.FirstOrDefault());
        var panel = Form(name, goal, manager);
        if (await ShowDialogAsync("新建 Agent 团队", panel, "创建") != ContentDialogResult.Primary ||
            manager.SelectedItem is not AgentProfile selected) return;
        await IgnoreFailureAsync(() => ViewModel.CreateTeamAsync(name.Text, goal.Text, selected.Id));
    }

    private async void OnOpenDirectClick(object sender, RoutedEventArgs e)
    {
        if (!RequireAgents()) return;
        var picker = AgentPicker("选择 Agent", ViewModel.SelectedAgent);
        if (await ShowDialogAsync("打开私聊", picker, "打开") == ContentDialogResult.Primary &&
            picker.SelectedItem is AgentProfile profile)
            await IgnoreFailureAsync(() => ViewModel.OpenDirectAsync(profile.Id));
    }

    private async void OnDirectAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentProfile profile })
            await IgnoreFailureAsync(() => ViewModel.OpenDirectAsync(profile.Id));
    }

    private async void OnConfigureTeamClick(object sender, RoutedEventArgs e)
    {
        var room = ViewModel.SelectedRoom;
        if (room is null || room.IsDirect) return;
        var name = new TextBox { Header = "团队名称", Text = room.Draft.Name };
        var goal = new TextBox { Header = "团队目标", Text = room.Draft.Goal, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        var manager = AgentPicker("项目经理", ViewModel.ProfileFor(room.ProjectManagerAgentId ?? string.Empty));
        var defaultAgent = AgentPicker("默认 Agent（消息未 @ 时接收）", ViewModel.ProfileFor(room.DefaultAgentId ?? string.Empty));
        if (await ShowDialogAsync("团队设置", Form(name, goal, manager, defaultAgent), "保存") != ContentDialogResult.Primary ||
            manager.SelectedItem is not AgentProfile managerProfile) return;
        await IgnoreFailureAsync(() => ViewModel.ConfigureTeamAsync(name.Text, goal.Text,
            (defaultAgent.SelectedItem as AgentProfile)?.Id, managerProfile.Id));
    }

    private async void OnAddMemberClick(object sender, RoutedEventArgs e)
    {
        var candidates = ViewModel.Agents.Where(profile =>
            ViewModel.Members.All(member => member.AgentId != profile.Id)).ToArray();
        if (candidates.Length == 0)
        {
            await AlertAsync("添加成员", "没有可添加的 Agent。请先创建 Agent，或成员已经全部加入。");
            return;
        }
        var picker = AgentPicker("Agent", candidates[0], candidates);
        var role = new TextBox { Header = "团队角色", Text = "member" };
        var responsibility = new TextBox { Header = "职责", AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        if (await ShowDialogAsync("添加团队成员", Form(picker, role, responsibility), "添加") != ContentDialogResult.Primary ||
            picker.SelectedItem is not AgentProfile profile) return;
        await IgnoreFailureAsync(() => ViewModel.AddMemberAsync(profile.Id, role.Text, responsibility.Text));
    }

    private async void OnMemberClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: AgentRoomMember member }) return;
        var profile = ViewModel.ProfileFor(member.AgentId);
        if (profile is null) return;
        var result = await ShowDialogAsync(profile.Draft.Name,
            new TextBlock { Text = $"角色：{member.Draft.Role}\n\n{member.Draft.Responsibility}", TextWrapping = TextWrapping.Wrap },
            ViewModel.SelectedRoom?.ProjectManagerAgentId == member.AgentId ? null : "移除");
        if (result == ContentDialogResult.Primary)
            await IgnoreFailureAsync(() => ViewModel.RemoveMemberAsync(member));
    }

    private async void OnSendClick(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(ViewModel.MessageText) && !ViewModel.HasPendingAttachments) return;
        await IgnoreFailureAsync(() => ViewModel.SendMessageAsync());
    }

    private async void OnMentionSendClick(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(ViewModel.MessageText) && !ViewModel.HasPendingAttachments) return;
        var list = new ListView
        {
            ItemsSource = ViewModel.MemberProfiles,
            DisplayMemberPath = "Draft.Name",
            SelectionMode = ListViewSelectionMode.Multiple,
            MinWidth = 360,
            MaxHeight = 360,
        };
        if (await ShowDialogAsync("选择要 @ 的 Agent", list, "发送") != ContentDialogResult.Primary) return;
        var ids = list.SelectedItems.Cast<AgentProfile>().Select(value => value.Id).ToArray();
        await IgnoreFailureAsync(() => ViewModel.SendMessageAsync(ids));
    }

    private async void OnAddAttachmentClick(object sender, RoutedEventArgs e)
    {
        try
        {
            var window = (Application.Current as App)?.MainWindow;
            if (window is null) throw new InvalidOperationException("无法找到当前窗口。");
            var picker = new FileOpenPicker
            {
                ViewMode = PickerViewMode.List,
                SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            };
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
            var attachments = new List<AgentMessageAttachment>();
            foreach (var file in await picker.PickMultipleFilesAsync())
            {
                using var stream = await file.OpenReadAsync();
                if (stream.Size is 0 or > 20 * 1024 * 1024)
                    throw new InvalidOperationException($"附件“{file.Name}”为空或超过 20 MB。");
                var bytes = await ReadAllBytesAsync(stream);
                var mime = string.IsNullOrWhiteSpace(file.ContentType)
                    ? "application/octet-stream" : file.ContentType;
                attachments.Add(new AgentMessageAttachment(
                    Guid.NewGuid().ToString("D").ToLowerInvariant(), file.Name, mime,
                    AttachmentKind(mime), bytes.LongLength, bytes));
            }
            ViewModel.AddAttachments(attachments);
        }
        catch (Exception exception)
        {
            await AlertAsync("无法添加附件", exception.Message);
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
            var attachment = await ViewModel.LoadAttachmentAsync(metadata.Id)
                ?? throw new InvalidOperationException("附件已经不存在。");
            var window = (Application.Current as App)?.MainWindow
                ?? throw new InvalidOperationException("无法找到当前窗口。");
            var extension = Path.GetExtension(attachment.Name);
            if (string.IsNullOrWhiteSpace(extension)) extension = ".bin";
            var picker = new FileSavePicker { SuggestedFileName = attachment.Name };
            picker.FileTypeChoices.Add("附件", [extension]);
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
            var file = await picker.PickSaveFileAsync();
            if (file is not null) await FileIO.WriteBytesAsync(file, attachment.Data);
        }
        catch (Exception exception)
        {
            await AlertAsync("无法保存附件", exception.Message);
        }
    }

    private async void OnNewTodoClick(object sender, RoutedEventArgs e) =>
        await ShowTodoDialogAsync(null);

    private async void OnEditTodoClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentTodo todo }) await ShowTodoDialogAsync(todo);
    }

    private async void OnTodoUpClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentTodo todo })
            await IgnoreFailureAsync(() => ViewModel.MoveTodoAsync(todo, -1));
    }

    private async void OnTodoDownClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentTodo todo })
            await IgnoreFailureAsync(() => ViewModel.MoveTodoAsync(todo, 1));
    }

    private async void OnNewAssetClick(object sender, RoutedEventArgs e) =>
        await ShowAssetDialogAsync(null);

    private async void OnEditAssetClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentTeamAsset asset }) await ShowAssetDialogAsync(asset);
    }

    private async void OnArchiveAssetClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentTeamAsset asset } &&
            await ConfirmAsync("归档资产", $"归档“{asset.Title}”？历史版本仍保留在本机。"))
            await IgnoreFailureAsync(() => ViewModel.ArchiveAssetAsync(asset));
    }

    private async void OnApproveStaffingProposalClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentStaffingProposal proposal } &&
            proposal.Status == AgentStaffingProposalStatus.Pending &&
            await ConfirmAsync("批准成员提案", $"批准 {proposal.Draft.Kind} 提案？"))
            await IgnoreFailureAsync(() => ViewModel.ResolveStaffingProposalAsync(proposal, true));
    }

    private async void OnRejectStaffingProposalClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentStaffingProposal proposal } &&
            proposal.Status == AgentStaffingProposalStatus.Pending &&
            await ConfirmAsync("拒绝成员提案", $"拒绝 {proposal.Draft.Kind} 提案？"))
            await IgnoreFailureAsync(() => ViewModel.ResolveStaffingProposalAsync(proposal, false));
    }

    private async Task ShowAgentDialogAsync(AgentProfile? profile)
    {
        var name = new TextBox { Header = "名称", Text = profile?.Draft.Name ?? string.Empty };
        var description = new TextBox { Header = "说明", Text = profile?.Draft.Description ?? string.Empty };
        var prompt = new TextBox { Header = "角色提示词", Text = profile?.Draft.RolePrompt ?? string.Empty, AcceptsReturn = true, MinHeight = 120, TextWrapping = TextWrapping.Wrap };
        var model = ModelPicker(profile?.Draft.ModelConfigId);
        var thinking = new ComboBox { Header = "推理强度", ItemsSource = new[] { "auto", "none", "low", "medium", "high", "xhigh" }, SelectedItem = profile?.Draft.ThinkingLevel ?? "auto" };
        var profession = new TextBox { Header = "职业 Key", Text = profile?.Draft.ProfessionKey ?? "general" };
        var plugins = new TextBox { Header = "默认插件 ID（逗号分隔）", Text = string.Join(", ", profile?.Draft.Plugins ?? []) };
        var skills = new TextBox { Header = "默认 Skill / 权限 ID（逗号分隔）", Text = string.Join(", ", profile?.Draft.Skills ?? []) };
        var requirementSurveys = new CheckBox
        {
            Content = "允许在所属团队发起并解决需求调研",
            IsChecked = profile?.Draft.ProfessionKey == "project_manager" ||
                profile?.Draft.Skills.Contains("requirement.survey.manage",
                    StringComparer.Ordinal) == true,
        };
        var staffManagement = new CheckBox
        {
            Content = "允许发起团队成员新增、入队和移出提案",
            IsChecked = profile is not null && AgentProfilePermissions.CanManageStaff(profile),
        };
        var heartbeat = new CheckBox { Content = "启用心跳", IsChecked = profile?.Draft.HeartbeatEnabled ?? false };
        var heartbeatInterval = new NumberBox
        {
            Header = "心跳间隔（秒）",
            Minimum = 60,
            Maximum = 86_400,
            Value = profile?.Draft.HeartbeatIntervalSeconds ?? 900,
        };
        var heartbeatPrompt = new TextBox
        {
            Header = "心跳提示词",
            Text = profile?.Draft.HeartbeatPrompt ?? string.Empty,
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
        };
        if (await ShowDialogAsync(profile is null ? "新建 Agent" : "编辑 Agent",
            new ScrollViewer
            {
                Content = Form(name, description, prompt, model, thinking, profession, plugins,
                    skills, requirementSurveys, staffManagement, heartbeat, heartbeatInterval,
                    heartbeatPrompt),
                MaxHeight = 650,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            }, "保存") != ContentDialogResult.Primary ||
            model.SelectedItem is not ConversationModelOption selectedModel) return;
        var skillIds = SplitIdentifiers(skills.Text)
            .Where(value => value != "requirement.survey.manage").ToList();
        if (requirementSurveys.IsChecked == true)
            skillIds.Add("requirement.survey.manage");
        var normalizedSkillIds = AgentProfilePermissions.NormalizeStaffPermissions(
            skillIds, staffManagement.IsChecked == true);
        var draft = new AgentProfileDraft(name.Text.Trim(), description.Text.Trim(), prompt.Text.Trim(),
            selectedModel.Id, thinking.SelectedItem?.ToString(), profession.Text.Trim(),
            SplitIdentifiers(plugins.Text), normalizedSkillIds,
            heartbeat.IsChecked == true, (int)heartbeatInterval.Value, heartbeatPrompt.Text.Trim());
        await IgnoreFailureAsync(() => ViewModel.SaveAgentAsync(profile?.Id, draft));
    }

    private async Task ShowTodoDialogAsync(AgentTodo? todo)
    {
        if (!RequireAgents() || ViewModel.SelectedRoom is null) return;
        if (todo is not null) await IgnoreFailureAsync(() => ViewModel.LoadTodoProgressAsync(todo));
        var assignee = AgentPicker("负责人", ViewModel.ProfileFor(todo?.Draft.AgentId ?? string.Empty), ViewModel.MemberProfiles);
        var title = new TextBox { Header = "标题", Text = todo?.Draft.Title ?? string.Empty };
        var detail = new TextBox { Header = "详情", Text = todo?.Draft.Detail ?? string.Empty, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        var priority = EnumPicker("优先级", todo?.Draft.Priority ?? AgentTodoPriority.Normal);
        if (todo is null)
        {
            if (await ShowDialogAsync("新建 Todo", Form(assignee, title, detail, priority), "创建") != ContentDialogResult.Primary ||
                assignee.SelectedItem is not AgentProfile profile) return;
            await IgnoreFailureAsync(() => ViewModel.CreateTodoAsync(profile.Id, title.Text, detail.Text,
                (AgentTodoPriority)priority.SelectedItem));
            return;
        }
        var status = EnumPicker("状态", todo.Status);
        var result = new TextBox { Header = "结果 / 阻塞原因", Text = todo.Result, AcceptsReturn = true, TextWrapping = TextWrapping.Wrap };
        var progress = new TextBox
        {
            Header = "执行进展与共享资产建议",
            Text = string.Join("\n\n", ViewModel.SelectedTodoProgress.Select(value =>
                $"[{value.Kind}] {value.Stage}\n{value.Detail}" +
                string.Concat(value.Suggestions.Select(suggestion =>
                    $"\n  建议更新 {suggestion.Category} · {suggestion.Title}\n  {suggestion.Rationale}")))),
            IsReadOnly = true,
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MaxHeight = 180,
        };
        if (await ShowDialogAsync("更新 Todo", Form(assignee, title, detail, priority, status, result, progress), "更新") != ContentDialogResult.Primary) return;
        await IgnoreFailureAsync(() => ViewModel.UpdateTodoAsync(todo, (AgentTodoStatus)status.SelectedItem,
            result.Text, (assignee.SelectedItem as AgentProfile)?.Id));
    }

    private async Task ShowAssetDialogAsync(AgentTeamAsset? asset)
    {
        var category = EnumPicker("类型", asset?.Category ?? AgentTeamAssetCategory.Note);
        var title = new TextBox { Header = "标题", Text = asset?.Title ?? string.Empty };
        var markdown = new TextBox { Header = "Markdown 内容", Text = asset?.Markdown ?? string.Empty, AcceptsReturn = true, MinHeight = 260, TextWrapping = TextWrapping.Wrap };
        if (await ShowDialogAsync(asset is null ? "新建团队资产" : $"编辑资产 · revision {asset.Revision}",
            Form(category, title, markdown), "保存") != ContentDialogResult.Primary) return;
        await IgnoreFailureAsync(() => ViewModel.SaveAssetAsync(asset,
            (AgentTeamAssetCategory)category.SelectedItem, title.Text, markdown.Text));
    }

    private ComboBox AgentPicker(string header, AgentProfile? selected, IEnumerable<AgentProfile>? source = null) =>
        new()
        {
            Header = header,
            ItemsSource = source ?? ViewModel.Agents,
            DisplayMemberPath = "Draft.Name",
            SelectedItem = selected ?? (source ?? ViewModel.Agents).FirstOrDefault(),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };

    private ComboBox ModelPicker(string? id) => new()
    {
        Header = "模型",
        ItemsSource = ViewModel.Models,
        DisplayMemberPath = nameof(ConversationModelOption.DisplayName),
        SelectedItem = ViewModel.Models.FirstOrDefault(value => value.Id == id) ?? ViewModel.Models.FirstOrDefault(),
        HorizontalAlignment = HorizontalAlignment.Stretch,
    };

    private static ComboBox EnumPicker<T>(string header, T selected) where T : struct, Enum => new()
    {
        Header = header,
        ItemsSource = Enum.GetValues<T>(),
        SelectedItem = selected,
        HorizontalAlignment = HorizontalAlignment.Stretch,
    };

    private static IReadOnlyList<string> SplitIdentifiers(string value) => value
        .Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
        .Distinct(StringComparer.Ordinal)
        .ToArray();

    private static StackPanel Form(params UIElement[] children)
    {
        var panel = new StackPanel { Spacing = 10, MinWidth = 440 };
        foreach (var child in children) panel.Children.Add(child);
        return panel;
    }

    private static async Task<byte[]> ReadAllBytesAsync(IRandomAccessStream stream)
    {
        var bytes = new byte[(int)stream.Size];
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        await reader.LoadAsync((uint)stream.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    private static AgentMessageAttachmentKind AttachmentKind(string mimeType)
    {
        if (mimeType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
            return AgentMessageAttachmentKind.Image;
        if (mimeType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase))
            return AgentMessageAttachmentKind.Audio;
        return AgentMessageAttachmentKind.File;
    }

    private async Task<ContentDialogResult> ShowDialogAsync(string title, object content, string? primary)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = title,
            Content = content,
            PrimaryButtonText = primary,
            CloseButtonText = "取消",
            DefaultButton = primary is null ? ContentDialogButton.Close : ContentDialogButton.Primary,
        };
        return await dialog.ShowAsync();
    }

    private async Task AlertAsync(string title, string message) =>
        _ = await ShowDialogAsync(title,
            new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap }, null);

    private async Task<bool> ConfirmAsync(string title, string message) =>
        await ShowDialogAsync(title, new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap }, "确认") == ContentDialogResult.Primary;

    private bool RequireAgents()
    {
        if (ViewModel.Agents.Count > 0) return true;
        _ = AlertAsync("需要 Agent", "请先创建至少一个 Agent。");
        return false;
    }

    private static async Task IgnoreFailureAsync(Func<Task> action)
    {
        try { await action(); }
        catch (OperationCanceledException) { }
        catch { }
    }
}
