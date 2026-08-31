using System.Collections.ObjectModel;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Plugins;
using ChatOS.Presentation.Threading;
using ChatOS.Presentation.Settings;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Plugins;

public interface IPluginArtifactUserInteraction
{
    string CacheDirectory { get; }

    Task<string?> PickSavePathAsync(
        string suggestedFileName,
        CancellationToken cancellationToken = default);

    Task OpenFileAsync(string path, CancellationToken cancellationToken = default);
}

public sealed partial class PluginArtifactsViewModel : ObservableObject, IDisposable
{
    private readonly IPluginArtifactService _service;
    private readonly IPluginArtifactUserInteraction _interaction;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private readonly object _loadSync = new();
    private CancellationTokenSource? _loadCancellation;
    private long _loadGeneration;
    private string? _adapterSessionId;

    public PluginArtifactsViewModel(
        IPluginArtifactService service,
        IPluginArtifactUserInteraction interaction,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _service = service;
        _interaction = interaction;
        _dispatcher = dispatcher;
        _localization = localization;
    }

    public ObservableCollection<PluginArtifactItemViewModel> Artifacts { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasArtifacts))]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isTransferring;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public bool HasArtifacts => Artifacts.Count != 0;

    public async Task LoadAsync(
        string? adapterSessionId = null,
        CancellationToken cancellationToken = default)
    {
        CancellationTokenSource loadCancellation;
        long generation;
        lock (_loadSync)
        {
            _adapterSessionId = adapterSessionId;
            _loadCancellation?.Cancel();
            _loadCancellation?.Dispose();
            _loadCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            loadCancellation = _loadCancellation;
            generation = ++_loadGeneration;
        }

        var token = loadCancellation.Token;
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsLoading = true;
                ErrorMessage = null;
            }, token).ConfigureAwait(false);
            var descriptors = await _service.ListAsync(adapterSessionId, token).ConfigureAwait(false);
            if (!IsCurrent(generation, loadCancellation)) return;
            var items = descriptors.Select(value => new PluginArtifactItemViewModel(value, _localization)).ToArray();
            await _dispatcher.InvokeAsync(() =>
            {
                Artifacts.Clear();
                foreach (var item in items) Artifacts.Add(item);
                OnPropertyChanged(nameof(HasArtifacts));
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            // A newer load replaced this one.
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (IsCurrent(generation, loadCancellation))
            {
                await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                    .ConfigureAwait(false);
            }
        }
        finally
        {
            if (IsCurrent(generation, loadCancellation))
            {
                await _dispatcher.InvokeAsync(() => IsLoading = false).ConfigureAwait(false);
            }
        }
    }

    public Task RefreshAsync(CancellationToken cancellationToken = default) =>
        LoadAsync(_adapterSessionId, cancellationToken);

    public async Task OpenAsync(
        PluginArtifactItemViewModel item,
        CancellationToken cancellationToken = default)
    {
        await RunTransferAsync(async token =>
        {
            Directory.CreateDirectory(_interaction.CacheDirectory);
            var fileName = BuildCacheFileName(item.Descriptor);
            var target = Path.Combine(_interaction.CacheDirectory, fileName);
            await CopyAtomicallyAsync(item.Descriptor, target, token).ConfigureAwait(false);
            await _interaction.OpenFileAsync(target, token).ConfigureAwait(false);
            return Text($"已打开 {item.DisplayName}", $"Opened {item.DisplayName}");
        }, cancellationToken).ConfigureAwait(false);
    }

    public async Task SaveAsAsync(
        PluginArtifactItemViewModel item,
        CancellationToken cancellationToken = default)
    {
        var target = await _interaction.PickSavePathAsync(item.SafeFileName, cancellationToken)
            .ConfigureAwait(false);
        if (string.IsNullOrWhiteSpace(target)) return;

        await RunTransferAsync(async token =>
        {
            await CopyAtomicallyAsync(item.Descriptor, target, token).ConfigureAwait(false);
            return Text($"已保存到 {target}", $"Saved to {target}");
        }, cancellationToken).ConfigureAwait(false);
    }

    public void Stop()
    {
        lock (_loadSync)
        {
            _loadGeneration++;
            _loadCancellation?.Cancel();
            _loadCancellation?.Dispose();
            _loadCancellation = null;
        }
    }

    public void Dispose() => Stop();

    public static string SanitizeFileName(string value)
    {
        var invalid = Path.GetInvalidFileNameChars().Concat("<>:\"/\\|?*").ToHashSet();
        var builder = new StringBuilder(value.Length);
        foreach (var character in value.Trim())
        {
            builder.Append(invalid.Contains(character) || char.IsControl(character) ? '_' : character);
        }

        var result = builder.ToString().Trim().TrimEnd('.');
        if (string.IsNullOrWhiteSpace(result)) result = "artifact.bin";
        if (result.Length > 120)
        {
            var extension = Path.GetExtension(result);
            var stemLength = Math.Max(1, 120 - extension.Length);
            result = result[..stemLength] + extension;
        }

        return result;
    }

    private async Task RunTransferAsync(
        Func<CancellationToken, Task<string>> operation,
        CancellationToken cancellationToken)
    {
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsTransferring = true;
                ErrorMessage = null;
                ActionMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            var message = await operation(cancellationToken).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ActionMessage = message, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsTransferring = false).ConfigureAwait(false);
        }
    }

    private async Task CopyAtomicallyAsync(
        PluginArtifactDescriptor descriptor,
        string target,
        CancellationToken cancellationToken)
    {
        var absoluteTarget = Path.GetFullPath(target);
        var directory = Path.GetDirectoryName(absoluteTarget)
            ?? throw new InvalidOperationException("Artifact destination has no parent directory.");
        Directory.CreateDirectory(directory);
        var temporary = Path.Combine(
            directory,
            $".{Path.GetFileName(absoluteTarget)}.{Guid.NewGuid():N}.chatos.tmp");
        try
        {
            await using (var stream = new FileStream(
                temporary,
                FileMode.CreateNew,
                FileAccess.Write,
                FileShare.None,
                64 * 1024,
                FileOptions.Asynchronous | FileOptions.SequentialScan))
            {
                await _service.CopyToAsync(descriptor.ArtifactId, stream, cancellationToken)
                    .ConfigureAwait(false);
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
            }

            File.Move(temporary, absoluteTarget, true);
        }
        finally
        {
            if (File.Exists(temporary)) File.Delete(temporary);
        }
    }

    private static string BuildCacheFileName(PluginArtifactDescriptor descriptor)
    {
        var safeName = SanitizeFileName(descriptor.DisplayName);
        var extension = Path.GetExtension(safeName);
        var stem = Path.GetFileNameWithoutExtension(safeName);
        var identity = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(descriptor.ArtifactId)))
            .ToLowerInvariant()[..12];
        return $"{stem}-{identity}{extension}";
    }

    private bool IsCurrent(long generation, CancellationTokenSource cancellation) =>
        !cancellation.IsCancellationRequested && Interlocked.Read(ref _loadGeneration) == generation;

    private string Text(string chinese, string english) =>
        _localization?.Text(chinese, english) ?? chinese;
}

public sealed class PluginArtifactItemViewModel
{
    public PluginArtifactItemViewModel(
        PluginArtifactDescriptor descriptor,
        LocalizationViewModel? localization = null)
    {
        Descriptor = descriptor;
        SafeFileName = PluginArtifactsViewModel.SanitizeFileName(descriptor.DisplayName);
        OpenLabel = localization?.Open ?? "打开";
        SaveAsLabel = localization?.SaveAs ?? "另存为";
    }

    public PluginArtifactDescriptor Descriptor { get; }

    public string DisplayName => Descriptor.DisplayName;

    public string SafeFileName { get; }

    public string OpenLabel { get; }

    public string SaveAsLabel { get; }

    public string MediaType => Descriptor.MediaType;

    public string Producer => Descriptor.ProducerToolName;

    public string CreatedLabel => Descriptor.CreatedAt.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss");

    public string SizeLabel => Descriptor.SizeBytes switch
    {
        >= 1024L * 1024L => $"{Descriptor.SizeBytes / (1024d * 1024d):0.##} MB",
        >= 1024L => $"{Descriptor.SizeBytes / 1024d:0.##} KB",
        _ => $"{Descriptor.SizeBytes} B",
    };
}
