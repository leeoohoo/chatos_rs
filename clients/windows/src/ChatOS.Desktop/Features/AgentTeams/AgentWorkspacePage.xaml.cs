using ChatOS.Core.Domain;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.AgentTeams;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

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
        if (string.IsNullOrWhiteSpace(ViewModel.MessageText)) return;
        await IgnoreFailureAsync(() => ViewModel.SendMessageAsync());
    }

    private async void OnNewAgentClick(object sender, RoutedEventArgs e) =>
        await ShowAgentDialogAsync(null);

    private async void OnEditAgentClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: AgentProfile profile })
            await ShowAgentDialogAsync(profile);
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
        var draft = profile?.Draft with
        {
            Name = name.Text.Trim(),
            Description = description.Text.Trim(),
            RolePrompt = prompt.Text.Trim(),
            ModelConfigId = selectedModel.Id,
            ThinkingLevel = thinking.SelectedItem?.ToString(),
        } ?? new AgentProfileDraft(
            name.Text.Trim(),
            description.Text.Trim(),
            prompt.Text.Trim(),
            selectedModel.Id,
            thinking.SelectedItem?.ToString());
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

    private static async Task IgnoreFailureAsync(Func<Task> action)
    {
        try { await action(); }
        catch (OperationCanceledException) { }
        catch { }
    }
}
