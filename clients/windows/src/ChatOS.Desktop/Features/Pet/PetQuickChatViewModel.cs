using System.Collections.ObjectModel;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Pet;

public sealed partial class PetQuickChatViewModel : ObservableObject, IDisposable
{
    private readonly MainWindowViewModel _mainWindow;
    private readonly PetFavoriteProjectsManager _favorites;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;

    public PetQuickChatViewModel(
        MainWindowViewModel mainWindow,
        PetFavoriteProjectsManager favorites,
        ConversationSessionFactory conversationFactory,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher)
    {
        _mainWindow = mainWindow;
        _favorites = favorites;
        _localization = localization;
        _dispatcher = dispatcher;
        Conversation = conversationFactory.Create();
        _mainWindow.Contacts.CollectionChanged += OnWorkspaceResourcesChanged;
        _mainWindow.Projects.CollectionChanged += OnWorkspaceResourcesChanged;
        _favorites.Changed += OnFavoritesChanged;
        _localization.PropertyChanged += OnLocalizationChanged;
        RebuildResources();
    }

    public ConversationSessionViewModel Conversation { get; }

    public ObservableCollection<PetQuickChatResourceViewModel> Resources { get; } = [];

    public bool HasResources => Resources.Count > 0;

    public bool HasSelectedResource => SelectedResource is not null;

    public string Title => _localization.Text("快捷聊天", "Quick chat");

    public string Description => _localization.Text("选择叽咕狸或常用项目", "Choose Jiguli or a favorite project");

    public string EmptyHint => _localization.Text(
        "可在项目运行页开启“设为常用项目”。",
        "Enable “Favorite project” on the project Run page.");

    public string PreparingHint => _localization.Text(
        "正在准备项目会话…",
        "Preparing the project conversation…");

    public string SendLabel => _localization.Text("发送", "Send");

    public string BackLabel => _localization.Text("返回", "Back");

    public string TaskMessagesLabel => _localization.Text("任务消息", "Task messages");

    public string ReplyingLabel => _localization.Text("正在回复…", "Replying…");

    public string ComposerPlaceholder => _localization.Text("发送消息…", "Send a message…");

    public string CancelLabel => _localization.Cancel;

    [ObservableProperty]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSelectedResource))]
    private PetQuickChatResourceViewModel? _selectedResource;

    [ObservableProperty]
    private bool _isPreparing;

    [ObservableProperty]
    private string? _errorMessage;

    public void Open()
    {
        RebuildResources();
        IsOpen = true;
        ErrorMessage = null;
    }

    public async Task CloseAsync()
    {
        IsOpen = false;
        SelectedResource = null;
        await Conversation.OpenAsync(null, "ChatOS");
    }

    public async Task SelectAsync(
        PetQuickChatResourceViewModel resource,
        CancellationToken cancellationToken = default)
    {
        IsPreparing = true;
        ErrorMessage = null;
        try
        {
            var conversationId = resource.ConversationId;
            if (resource.Kind == WorkspaceResourceKind.Project && string.IsNullOrWhiteSpace(conversationId))
            {
                conversationId = await _mainWindow.EnsureProjectConversationAsync(
                    resource.SourceId,
                    cancellationToken);
                RebuildResources();
                resource = Resources.First(value => value.Id == resource.Id);
            }

            if (string.IsNullOrWhiteSpace(conversationId))
            {
                throw new InvalidOperationException(_localization.Text(
                    "叽咕狸当前没有可用会话，请先在主界面刷新工作区。",
                    "Jiguli does not have an available conversation. Refresh the workspace first."));
            }

            SelectedResource = resource;
            await Conversation.OpenAsync(
                conversationId,
                resource.Title,
                cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsPreparing = false;
        }
    }

    public async Task BackAsync()
    {
        SelectedResource = null;
        ErrorMessage = null;
        await Conversation.OpenAsync(null, "ChatOS");
    }

    public void Dispose()
    {
        _mainWindow.Contacts.CollectionChanged -= OnWorkspaceResourcesChanged;
        _mainWindow.Projects.CollectionChanged -= OnWorkspaceResourcesChanged;
        _favorites.Changed -= OnFavoritesChanged;
        _localization.PropertyChanged -= OnLocalizationChanged;
        Conversation.Dispose();
    }

    private void RebuildResources()
    {
        var selectedId = SelectedResource?.Id;
        var next = new List<PetQuickChatResourceViewModel>();
        var jiguli = _mainWindow.Contacts.FirstOrDefault(value =>
                string.Equals(value.Id, "jiguli", StringComparison.OrdinalIgnoreCase) ||
                string.Equals(value.Title.Trim(), "叽咕狸", StringComparison.Ordinal))
            ?? _mainWindow.Contacts.FirstOrDefault();
        if (jiguli is not null)
        {
            next.Add(new PetQuickChatResourceViewModel(
                $"contact:{jiguli.Id}",
                jiguli.Id,
                WorkspaceResourceKind.Contact,
                jiguli.Title,
                jiguli.Subtitle,
                jiguli.ConversationId,
                "\uE77B"));
        }

        next.AddRange(_mainWindow.Projects
            .Where(value => _favorites.IsFavorite(value.Id))
            .Select(value => new PetQuickChatResourceViewModel(
                $"project:{value.Id}",
                value.Id,
                WorkspaceResourceKind.Project,
                value.Title,
                value.Subtitle,
                value.ConversationId,
                "\uE8B7")));
        Resources.Clear();
        foreach (var item in next) Resources.Add(item);
        SelectedResource = selectedId is null
            ? null
            : Resources.FirstOrDefault(value => value.Id == selectedId);
        OnPropertyChanged(nameof(HasResources));
    }

    private void OnWorkspaceResourcesChanged(object? sender, System.Collections.Specialized.NotifyCollectionChangedEventArgs e) =>
        _ = _dispatcher.InvokeAsync(RebuildResources);

    private void OnFavoritesChanged(object? sender, EventArgs e) =>
        _ = _dispatcher.InvokeAsync(RebuildResources);

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) =>
        OnPropertyChanged(string.Empty);
}

public sealed record PetQuickChatResourceViewModel(
    string Id,
    string SourceId,
    WorkspaceResourceKind Kind,
    string Title,
    string Subtitle,
    string? ConversationId,
    string Glyph);
