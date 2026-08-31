using System.Collections.ObjectModel;
using ChatOS.Connector.Plugins;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Plugins;

public sealed partial class PluginVisualSessionsViewModel : ObservableObject, IDisposable
{
    private static readonly IReadOnlySet<string> NoFrames = new HashSet<string>(StringComparer.Ordinal);
    private readonly IPluginVisualSessionService _service;
    private readonly IUiDispatcher _dispatcher;
    private readonly object _refreshSync = new();
    private readonly HashSet<string> _dismissed = new(StringComparer.Ordinal);
    private CancellationTokenSource? _refreshCancellation;
    private long _refreshGeneration;
    private string? _preferredAdapterSessionId;

    public PluginVisualSessionsViewModel(
        IPluginVisualSessionService service,
        IUiDispatcher dispatcher)
    {
        _service = service;
        _dispatcher = dispatcher;
    }

    public ObservableCollection<PluginVisualSession> Sessions { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasSessions))]
    [NotifyPropertyChangedFor(nameof(FrameData))]
    [NotifyPropertyChangedFor(nameof(HasFrame))]
    private PluginVisualSession? _selectedSession;

    [ObservableProperty]
    private bool _isRefreshing;

    [ObservableProperty]
    private string? _errorMessage;

    public bool HasSessions => Sessions.Count != 0;

    public byte[]? FrameData => SelectedSession?.FrameData;

    public bool HasFrame => FrameData is { Length: > 0 };

    public async Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        CancellationTokenSource refreshCancellation;
        long generation;
        lock (_refreshSync)
        {
            _refreshCancellation?.Cancel();
            _refreshCancellation?.Dispose();
            _refreshCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            refreshCancellation = _refreshCancellation;
            generation = ++_refreshGeneration;
        }

        var token = refreshCancellation.Token;
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsRefreshing = true;
                ErrorMessage = null;
            }, token).ConfigureAwait(false);

            var metadata = await _service.ReadAsync(NoFrames, token).ConfigureAwait(false);
            var activeIdentities = metadata.Select(SessionIdentity).ToHashSet(StringComparer.Ordinal);
            lock (_dismissed)
            {
                _dismissed.RemoveWhere(identity => !activeIdentities.Contains(identity));
                metadata = metadata.Where(value => !_dismissed.Contains(SessionIdentity(value))).ToArray();
            }

            var selectedAdapterSessionId = SelectAdapterSession(metadata);
            IReadOnlyList<PluginVisualSession> withFrame = metadata;
            if (selectedAdapterSessionId is not null)
            {
                withFrame = await _service.ReadAsync(
                    new HashSet<string>(StringComparer.Ordinal) { selectedAdapterSessionId },
                    token).ConfigureAwait(false);
            }

            if (!IsCurrent(generation, refreshCancellation)) return;

            var selectedWithFrame = selectedAdapterSessionId is null
                ? null
                : withFrame.FirstOrDefault(value =>
                    string.Equals(value.AdapterSessionId, selectedAdapterSessionId, StringComparison.Ordinal));
            var next = metadata.Select(value =>
                    selectedWithFrame is not null &&
                    string.Equals(value.AdapterSessionId, selectedWithFrame.AdapterSessionId, StringComparison.Ordinal)
                        ? selectedWithFrame
                        : value)
                .ToArray();

            await _dispatcher.InvokeAsync(() => Apply(next, selectedAdapterSessionId), token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            // A newer refresh replaced this one.
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (IsCurrent(generation, refreshCancellation))
            {
                await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                    .ConfigureAwait(false);
            }
        }
        finally
        {
            if (IsCurrent(generation, refreshCancellation))
            {
                await _dispatcher.InvokeAsync(() => IsRefreshing = false).ConfigureAwait(false);
            }
        }
    }

    public Task SelectAsync(
        string adapterSessionId,
        CancellationToken cancellationToken = default)
    {
        _preferredAdapterSessionId = adapterSessionId;
        return RefreshAsync(cancellationToken);
    }

    public async Task DismissSelectedAsync(CancellationToken cancellationToken = default)
    {
        if (SelectedSession is not { } selected) return;
        lock (_dismissed)
        {
            _dismissed.Add(SessionIdentity(selected));
        }

        _preferredAdapterSessionId = null;
        await RefreshAsync(cancellationToken).ConfigureAwait(false);
    }

    public void Stop()
    {
        lock (_refreshSync)
        {
            _refreshGeneration++;
            _refreshCancellation?.Cancel();
            _refreshCancellation?.Dispose();
            _refreshCancellation = null;
        }
    }

    public void Dispose() => Stop();

    private string? SelectAdapterSession(IReadOnlyList<PluginVisualSession> sessions)
    {
        var current = _preferredAdapterSessionId ?? SelectedSession?.AdapterSessionId;
        if (current is not null && sessions.Any(value =>
            string.Equals(value.AdapterSessionId, current, StringComparison.Ordinal)))
        {
            return current;
        }

        _preferredAdapterSessionId = sessions.FirstOrDefault()?.AdapterSessionId;
        return _preferredAdapterSessionId;
    }

    private void Apply(
        IReadOnlyList<PluginVisualSession> sessions,
        string? selectedAdapterSessionId)
    {
        for (var index = 0; index < sessions.Count; index++)
        {
            var value = sessions[index];
            if (index < Sessions.Count && SameFrame(Sessions[index], value))
            {
                value = Sessions[index];
            }

            if (index < Sessions.Count)
            {
                if (!ReferenceEquals(Sessions[index], value)) Sessions[index] = value;
            }
            else
            {
                Sessions.Add(value);
            }
        }

        while (Sessions.Count > sessions.Count) Sessions.RemoveAt(Sessions.Count - 1);

        SelectedSession = selectedAdapterSessionId is null
            ? null
            : Sessions.FirstOrDefault(value => string.Equals(
                value.AdapterSessionId,
                selectedAdapterSessionId,
                StringComparison.Ordinal));
        OnPropertyChanged(nameof(HasSessions));
        OnPropertyChanged(nameof(FrameData));
        OnPropertyChanged(nameof(HasFrame));
    }

    private bool IsCurrent(long generation, CancellationTokenSource cancellation) =>
        !cancellation.IsCancellationRequested &&
        Interlocked.Read(ref _refreshGeneration) == generation;

    private static string SessionIdentity(PluginVisualSession session) =>
        $"{session.AdapterSessionId}\n{session.Id}";

    private static bool SameFrame(PluginVisualSession left, PluginVisualSession right) =>
        string.Equals(left.AdapterSessionId, right.AdapterSessionId, StringComparison.Ordinal) &&
        string.Equals(left.Id, right.Id, StringComparison.Ordinal) &&
        left.FrameSequence == right.FrameSequence &&
        left.CapturedAt == right.CapturedAt &&
        string.Equals(left.Title, right.Title, StringComparison.Ordinal) &&
        string.Equals(left.TargetApplication, right.TargetApplication, StringComparison.Ordinal) &&
        left.FrameData is not null == right.FrameData is not null;
}
