using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.AgentTeams;

public sealed partial class AgentTeamWorkspaceViewModel : ObservableObject, IDisposable
{
    private readonly IAgentTeamService _service;
    private readonly IConversationRuntimeSettingsService _runtimeSettings;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _refreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;
    private string? _ownerUserId;
    private string? _loadedRoomId;
    private int _localMutationCount;
    private bool _modelsLoaded;

    public AgentTeamWorkspaceViewModel(
        IAgentTeamService service,
        IConversationRuntimeSettingsService runtimeSettings,
        IUiDispatcher dispatcher)
    {
        _service = service;
        _runtimeSettings = runtimeSettings;
        _dispatcher = dispatcher;
        _service.Changed += OnServiceChanged;
    }

    public ObservableCollection<AgentProfile> Agents { get; } = [];
    public ObservableCollection<ConversationModelOption> Models { get; } = [];
    public ObservableCollection<AgentRoom> Rooms { get; } = [];
    public ObservableCollection<AgentRoomMember> Members { get; } = [];
    public ObservableCollection<AgentProfile> MemberProfiles { get; } = [];
    public ObservableCollection<AgentMessage> Messages { get; } = [];
    public ObservableCollection<AgentTodo> Todos { get; } = [];
    public ObservableCollection<AgentTodoProgress> SelectedTodoProgress { get; } = [];
    public ObservableCollection<AgentTeamAsset> Assets { get; } = [];
    public ObservableCollection<AgentRequirementSurvey> RequirementSurveys { get; } = [];
    public ObservableCollection<AgentStaffingProposal> StaffingProposals { get; } = [];
    public ObservableCollection<AgentRunSummary> Runs { get; } = [];
    public ObservableCollection<AgentMessageAttachment> PendingAttachments { get; } = [];

    public bool IsOpen => _ownerUserId is not null && ProjectId is not null;
    public bool HasRoom => SelectedRoom is not null;
    public bool HasAgents => Agents.Count > 0;
    public bool CanConfigureTeam => SelectedRoom is { Kind: AgentConversationKind.ProjectTeam };
    public bool HasPendingAttachments => PendingAttachments.Count > 0;

    [ObservableProperty]
    private string? _projectId;

    [ObservableProperty]
    private string _projectName = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasRoom))]
    [NotifyPropertyChangedFor(nameof(CanConfigureTeam))]
    private AgentRoom? _selectedRoom;

    [ObservableProperty]
    private AgentProfile? _selectedAgent;

    [ObservableProperty]
    private AgentTodo? _selectedTodo;

    [ObservableProperty]
    private AgentTeamAsset? _selectedAsset;

    [ObservableProperty]
    private string _messageText = string.Empty;

    [ObservableProperty]
    private string _statusMessage = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _isBusy;

    public async Task OpenAsync(
        string ownerUserId,
        WorkspaceProject project,
        CancellationToken cancellationToken = default)
    {
        CancelSession();
        _ownerUserId = ownerUserId;
        ProjectId = project.Id;
        ProjectName = project.Name;
        _modelsLoaded = false;
        await _dispatcher.InvokeAsync(() => Models.Clear(), cancellationToken).ConfigureAwait(false);
        _sessionCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        Interlocked.Increment(ref _generation);
        await RefreshAsync(_sessionCancellation.Token).ConfigureAwait(false);
    }

    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        using var context = RequireContext(cancellationToken);
        await _refreshGate.WaitAsync(context.Token).ConfigureAwait(false);
        try
        {
            await SetBusyAsync(true, null, context.Token).ConfigureAwait(false);
            var agentsTask = _service.ListAgentsAsync(context.Owner, false, context.Token);
            var roomsTask = _service.ListRoomsAsync(context.Owner, context.Project, context.Token);
            var directsTask = _service.ListRoomsAsync(context.Owner, "direct", context.Token);
            var shouldLoadModels = !_modelsLoaded;
            var modelsTask = shouldLoadModels
                ? FetchModelsSafelyAsync(context.Token)
                : Task.FromResult<IReadOnlyList<ConversationModelOption>>([]);
            await Task.WhenAll(agentsTask, roomsTask, directsTask, modelsTask).ConfigureAwait(false);
            EnsureCurrent(context.Generation, context.Token);
            await _dispatcher.InvokeAsync(() =>
            {
                Replace(Agents, agentsTask.Result);
                if (shouldLoadModels)
                {
                    Replace(Models, modelsTask.Result.Where(value => value.HasApiKey));
                    _modelsLoaded = true;
                }
                var roomId = SelectedRoom?.Id;
                Replace(Rooms, roomsTask.Result.Concat(directsTask.Result)
                    .OrderBy(value => value.IsDirect)
                    .ThenByDescending(value => value.UpdatedAtUnixMs));
                SelectedRoom = Rooms.FirstOrDefault(value => value.Id == roomId) ?? Rooms.FirstOrDefault();
                SelectedAgent = Agents.FirstOrDefault(value => value.Id == SelectedAgent?.Id)
                    ?? Agents.FirstOrDefault();
                OnPropertyChanged(nameof(HasAgents));
            }, context.Token).ConfigureAwait(false);
            if (SelectedRoom is not null)
            {
                await LoadSelectedRoomAsync(context).ConfigureAwait(false);
            }
            else
            {
                await _dispatcher.InvokeAsync(ClearRoomState, context.Token).ConfigureAwait(false);
            }

            await SetBusyAsync(false, null, context.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (context.Token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await SetBusyAsync(false, exception.Message, CancellationToken.None).ConfigureAwait(false);
        }
        finally
        {
            _refreshGate.Release();
        }
    }

    public async Task SelectRoomAsync(AgentRoom? room)
    {
        if (room is null || room.Id == _loadedRoomId) return;
        SelectedRoom = room;
        using var context = RequireContext(CancellationToken.None);
        await LoadSelectedRoomAsync(context).ConfigureAwait(false);
    }

    public string AgentName(string? agentId) =>
        Agents.FirstOrDefault(value => value.Id == agentId)?.Draft.Name ??
        (agentId is null ? "你" : agentId);

    public string MessageSender(AgentMessage message) => message.SenderKind switch
    {
        AgentMessageSenderKind.Human => "你",
        AgentMessageSenderKind.System => "系统",
        _ => AgentName(message.SenderAgentId),
    };

    public AgentProfile? ProfileFor(string agentId) =>
        Agents.FirstOrDefault(value => value.Id == agentId);

    public void AddAttachments(IEnumerable<AgentMessageAttachment> attachments)
    {
        foreach (var attachment in attachments)
        {
            attachment.Validate();
            if (PendingAttachments.Count >= 20)
                throw new AgentTeamException(AgentTeamError.InvalidField,
                    "每条团队消息最多添加 20 个附件。");
            PendingAttachments.Add(attachment);
        }
        OnPropertyChanged(nameof(HasPendingAttachments));
    }

    public void RemoveAttachment(AgentMessageAttachment attachment)
    {
        PendingAttachments.Remove(attachment);
        OnPropertyChanged(nameof(HasPendingAttachments));
    }

    public async Task DrainAsync()
    {
        using var context = RequireContext(CancellationToken.None);
        await ExecuteAsync("正在运行 Agent…", async () =>
        {
            await _service.DrainAsync(context.Owner, context.Token).ConfigureAwait(false);
            await RefreshAsync(context.Token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    public async Task LoadTodoProgressAsync(AgentTodo todo)
    {
        using var context = RequireContext(CancellationToken.None);
        var progress = await _service.ListTodoProgressAsync(context.Owner, todo.Id, context.Token)
            .ConfigureAwait(false);
        EnsureCurrent(context.Generation, context.Token);
        await _dispatcher.InvokeAsync(() => Replace(SelectedTodoProgress, progress), context.Token)
            .ConfigureAwait(false);
    }

    public async Task<AgentMessageAttachment?> LoadAttachmentAsync(string attachmentId)
    {
        using var context = RequireContext(CancellationToken.None);
        var roomId = SelectedRoom?.Id ?? throw new InvalidOperationException("请先选择一个对话。");
        return await _service.GetMessageAttachmentAsync(context.Owner, roomId, attachmentId,
            context.Token).ConfigureAwait(false);
    }

    private async Task LoadSelectedRoomAsync(SessionContext context)
    {
        var room = SelectedRoom;
        if (room is null) return;
        var snapshot = await _service.LoadSnapshotAsync(context.Owner, room.Id, context.Token)
            .ConfigureAwait(false);
        EnsureCurrent(context.Generation, context.Token);
        await _dispatcher.InvokeAsync(() =>
        {
            SelectedRoom = snapshot.Room;
            Replace(Members, snapshot.Members);
            Replace(MemberProfiles, snapshot.Profiles);
            Replace(Messages, snapshot.Messages);
            Replace(Todos, snapshot.Todos);
            Replace(Assets, snapshot.Assets);
            Replace(RequirementSurveys, snapshot.RequirementSurveys);
            Replace(StaffingProposals, snapshot.StaffingProposals);
            Replace(Runs, snapshot.Runs);
            _loadedRoomId = snapshot.Room.Id;
            SelectedTodo = Todos.FirstOrDefault(value => value.Id == SelectedTodo?.Id);
            SelectedAsset = Assets.FirstOrDefault(value => value.Id == SelectedAsset?.Id);
            OnPropertyChanged(nameof(CanConfigureTeam));
        }, context.Token).ConfigureAwait(false);
    }

    private void ClearRoomState()
    {
        Members.Clear();
        MemberProfiles.Clear();
        Messages.Clear();
        Todos.Clear();
        SelectedTodoProgress.Clear();
        Assets.Clear();
        RequirementSurveys.Clear();
        StaffingProposals.Clear();
        Runs.Clear();
        PendingAttachments.Clear();
        OnPropertyChanged(nameof(HasPendingAttachments));
        _loadedRoomId = null;
        SelectedTodo = null;
        SelectedAsset = null;
    }

    private async void OnServiceChanged(object? sender, AgentTeamChangedEventArgs args)
    {
        if (Volatile.Read(ref _localMutationCount) > 0 ||
            !string.Equals(args.OwnerUserId, _ownerUserId, StringComparison.Ordinal) ||
            args.ProjectId is not null && args.ProjectId != ProjectId && args.ProjectId != "direct") return;
        try
        {
            await RefreshAsync(_sessionCancellation?.Token ?? CancellationToken.None).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
    }

    private async Task ExecuteAsync(string status, Func<Task> action)
    {
        await SetBusyAsync(true, null, CancellationToken.None).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() => StatusMessage = status).ConfigureAwait(false);
        try
        {
            await action().ConfigureAwait(false);
            await SetBusyAsync(false, null, CancellationToken.None).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (_sessionCancellation?.IsCancellationRequested == true)
        {
        }
        catch (Exception exception)
        {
            await SetBusyAsync(false, exception.Message, CancellationToken.None).ConfigureAwait(false);
            throw;
        }
    }

    private async Task<IReadOnlyList<ConversationModelOption>> FetchModelsSafelyAsync(
        CancellationToken cancellationToken)
    {
        try
        {
            return await _runtimeSettings.FetchAvailableModelsAsync(cancellationToken)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            return [];
        }
    }

    private Task SetBusyAsync(bool busy, string? error, CancellationToken token) =>
        _dispatcher.InvokeAsync(() =>
        {
            IsBusy = busy;
            ErrorMessage = error;
            if (!busy) StatusMessage = error is null ? "已同步" : string.Empty;
        }, token);

    private SessionContext RequireContext(CancellationToken cancellationToken)
    {
        if (_ownerUserId is null || ProjectId is null || _sessionCancellation is null)
            throw new InvalidOperationException("Agent team workspace is not open.");
        var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken, _sessionCancellation.Token);
        return new(_ownerUserId, ProjectId, _generation, linked.Token, linked);
    }

    private void EnsureCurrent(long generation, CancellationToken token)
    {
        token.ThrowIfCancellationRequested();
        if (generation != _generation) throw new OperationCanceledException(token);
    }

    private static void Replace<T>(ObservableCollection<T> target, IEnumerable<T> values)
    {
        target.Clear();
        foreach (var value in values) target.Add(value);
    }

    private void CancelSession()
    {
        Interlocked.Increment(ref _generation);
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
    }

    public void Dispose()
    {
        _service.Changed -= OnServiceChanged;
        CancelSession();
        _refreshGate.Dispose();
    }

    private sealed record SessionContext(
        string Owner,
        string Project,
        long Generation,
        CancellationToken Token,
        CancellationTokenSource Linked) : IDisposable
    {
        public void Dispose() => Linked.Dispose();
    }
}
