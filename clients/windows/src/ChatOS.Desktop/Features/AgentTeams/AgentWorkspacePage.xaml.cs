using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.AgentTeams;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class AgentWorkspacePage : Page
{
    public AgentWorkspacePage(
        AgentTeamWorkspaceViewModel viewModel,
        MainWindowViewModel shell)
    {
        ViewModel = viewModel;
        Shell = shell;
        InitializeComponent();
        ViewModel.PropertyChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        ViewModel.Rooms.CollectionChanged += (_, _) => DispatcherQueue.TryEnqueue(RefreshState);
        Shell.Projects.CollectionChanged += (_, _) => DispatcherQueue.TryEnqueue(() => Bindings.Update());
        RefreshState();
    }

    public event EventHandler<ProjectFeatureRequestedEventArgs>? FeatureRequested;

    public AgentTeamWorkspaceViewModel ViewModel { get; }
    public MainWindowViewModel Shell { get; }
    public string SelectedRoomName => ViewModel.SelectedRoom?.Draft.Name ?? "选择一个 Agent 开始私聊";
    public string SelectedRoomGoal => ViewModel.SelectedRoom?.Draft.Goal ?? "";

    private async void OnRefreshClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(() => ViewModel.RefreshAsync());

    private async void OnLoadEarlierMessagesClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(ViewModel.LoadEarlierMessagesAsync);

    private async void OnRunAgentsClick(object sender, RoutedEventArgs e) =>
        await IgnoreFailureAsync(ViewModel.DrainAsync);

    private async void OnAgentClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is AgentProfile profile)
            await IgnoreFailureAsync(() => ViewModel.OpenDirectAsync(profile.Id));
    }

    private async void OnRoomSelectionChanged(object sender, SelectionChangedEventArgs e) =>
        await IgnoreFailureAsync(() => ViewModel.SelectRoomAsync(ViewModel.SelectedRoom));

    private async void OnSendClick(object sender, RoutedEventArgs e)
    {
        if (string.IsNullOrWhiteSpace(ViewModel.MessageText) && !ViewModel.HasPendingAttachments) return;
        await IgnoreFailureAsync(() => ViewModel.SendMessageAsync());
    }

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

    private void OnProjectClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: ShellResourceViewModel project }) return;
        FeatureRequested?.Invoke(this, new ProjectFeatureRequestedEventArgs(project, "agent-team"));
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
        var panel = new StackPanel { Spacing = 10, MinWidth = 440 };
        foreach (var child in new UIElement[] { name, description, prompt, model, thinking })
            panel.Children.Add(child);
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = profile is null ? "新建 Agent" : "编辑 Agent",
            Content = panel,
            PrimaryButtonText = "保存",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary ||
            model.SelectedItem is not ConversationModelOption selectedModel) return;
        AgentProfileDraft draft;
        if (profile is null)
        {
            draft = new AgentProfileDraft(
                name.Text.Trim(),
                description.Text.Trim(),
                prompt.Text.Trim(),
                selectedModel.Id,
                thinking.SelectedItem?.ToString());
        }
        else
        {
            draft = profile.Draft with
            {
                Name = name.Text.Trim(),
                Description = description.Text.Trim(),
                RolePrompt = prompt.Text.Trim(),
                ModelConfigId = selectedModel.Id,
                ThinkingLevel = thinking.SelectedItem?.ToString(),
            };
        }
        await IgnoreFailureAsync(() => ViewModel.SaveAgentAsync(profile?.Id, draft));
    }

    private void RefreshState()
    {
        Bindings.Update();
        NoticeBar.IsOpen = !string.IsNullOrWhiteSpace(ViewModel.ErrorMessage) ||
            ViewModel.IsBusy;
        NoticeBar.Severity = ViewModel.ErrorMessage is null
            ? InfoBarSeverity.Informational
            : InfoBarSeverity.Error;
        NoticeBar.Message = ViewModel.ErrorMessage ??
            (ViewModel.IsBusy ? "正在同步 Agent 工作区…" : ViewModel.StatusMessage);
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
}
