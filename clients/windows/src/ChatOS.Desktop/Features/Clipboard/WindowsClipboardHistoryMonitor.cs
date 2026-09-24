using System.Diagnostics;
using System.Runtime.InteropServices;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using Windows.Storage.Streams;
using WindowsClipboard = Windows.ApplicationModel.DataTransfer.Clipboard;

namespace ChatOS.Desktop.Features.Clipboard;

public sealed class WindowsClipboardHistoryMonitor : IDisposable
{
    private static readonly string[] SensitiveFormatFragments =
    [
        "password", "credential", "secret", "keepass", "1password", "bitwarden",
    ];

    private static readonly string[] SensitiveProcessFragments =
    [
        "1password", "bitwarden", "keepass", "dashlane", "lastpass", "protonpass",
    ];

    private readonly IClipboardHistoryStore _store;
    private readonly SemaphoreSlim _captureGate = new(1, 1);
    private bool _started;
    private bool _suppressNextChange;

    public WindowsClipboardHistoryMonitor(IClipboardHistoryStore store) => _store = store;

    public event EventHandler? HistoryChanged;

    public async Task StartAsync(CancellationToken cancellationToken = default)
    {
        if (_started) return;
        await _store.PruneAsync(DateTimeOffset.UtcNow.AddDays(-30), cancellationToken: cancellationToken);
        WindowsClipboard.ContentChanged += OnClipboardContentChanged;
        _started = true;
    }

    public void Stop()
    {
        if (!_started) return;
        WindowsClipboard.ContentChanged -= OnClipboardContentChanged;
        _started = false;
    }

    public async Task RestoreAsync(
        ClipboardHistoryPayload payload,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var package = new DataPackage { RequestedOperation = DataPackageOperation.Copy };
        InMemoryRandomAccessStream? imageStream = null;
        switch (payload.Kind)
        {
            case ClipboardHistoryKind.Text:
                package.SetText(payload.Text ?? string.Empty);
                break;
            case ClipboardHistoryKind.Url:
                var uri = new Uri(payload.Text ?? throw new InvalidOperationException("URL payload is empty."));
                package.SetWebLink(uri);
                package.SetText(uri.AbsoluteUri);
                break;
            case ClipboardHistoryKind.Files:
                var files = new List<IStorageItem>();
                foreach (var path in payload.FilePaths ?? [])
                {
                    cancellationToken.ThrowIfCancellationRequested();
                    files.Add(await StorageFile.GetFileFromPathAsync(path));
                }
                if (files.Count == 0) throw new InvalidOperationException("File payload is empty.");
                package.SetStorageItems(files);
                break;
            case ClipboardHistoryKind.Image:
                var bytes = payload.ImageBytes ?? throw new InvalidOperationException("Image payload is empty.");
                imageStream = new InMemoryRandomAccessStream();
                using (var writer = new DataWriter(imageStream))
                {
                    writer.WriteBytes(bytes);
                    await writer.StoreAsync();
                    await writer.FlushAsync();
                    writer.DetachStream();
                }
                imageStream.Seek(0);
                package.SetBitmap(RandomAccessStreamReference.CreateFromStream(imageStream));
                break;
            default:
                throw new ArgumentOutOfRangeException(nameof(payload));
        }

        _suppressNextChange = true;
        try
        {
            WindowsClipboard.SetContent(package);
            WindowsClipboard.Flush();
        }
        catch
        {
            _suppressNextChange = false;
            throw;
        }
        finally
        {
            imageStream?.Dispose();
        }
    }

    internal static bool IsSensitive(IEnumerable<string> formats, string? sourceApplication) =>
        formats.Any(format => SensitiveFormatFragments.Any(fragment =>
            format.Contains(fragment, StringComparison.OrdinalIgnoreCase)))
        || (!string.IsNullOrWhiteSpace(sourceApplication) && SensitiveProcessFragments.Any(fragment =>
            sourceApplication.Contains(fragment, StringComparison.OrdinalIgnoreCase)));

    private async void OnClipboardContentChanged(object? sender, object e)
    {
        if (_suppressNextChange)
        {
            _suppressNextChange = false;
            return;
        }

        if (!await _captureGate.WaitAsync(0)) return;
        try
        {
            var content = WindowsClipboard.GetContent();
            var source = TryGetClipboardOwnerProcessName();
            if (IsSensitive(content.AvailableFormats, source)) return;
            var payload = await ReadPayloadAsync(content);
            if (payload is null || payload.ByteCount <= 0 || payload.ByteCount > 20 * 1024 * 1024) return;
            await _store.StoreAsync(payload, source);
            await _store.PruneAsync(DateTimeOffset.UtcNow.AddDays(-30));
            HistoryChanged?.Invoke(this, EventArgs.Empty);
        }
        catch (Exception exception) when (exception is COMException or IOException or UnauthorizedAccessException)
        {
            // Clipboard ownership may change while delayed content is materializing.
        }
        finally
        {
            _captureGate.Release();
        }
    }

    private static async Task<ClipboardHistoryPayload?> ReadPayloadAsync(DataPackageView content)
    {
        if (content.Contains(StandardDataFormats.Bitmap))
        {
            var reference = await content.GetBitmapAsync();
            await using var source = (await reference.OpenReadAsync()).AsStreamForRead();
            await using var target = new MemoryStream();
            await source.CopyToAsync(target);
            return new(ClipboardHistoryKind.Image, ImageBytes: target.ToArray());
        }

        if (content.Contains(StandardDataFormats.StorageItems))
        {
            var items = await content.GetStorageItemsAsync();
            var paths = items.Select(item => item.Path).Where(path => !string.IsNullOrWhiteSpace(path)).ToArray();
            return paths.Length == 0 ? null : new(ClipboardHistoryKind.Files, FilePaths: paths);
        }

        if (content.Contains(StandardDataFormats.WebLink))
        {
            var uri = await content.GetWebLinkAsync();
            return new(ClipboardHistoryKind.Url, Text: uri.AbsoluteUri);
        }

        if (content.Contains(StandardDataFormats.Text))
        {
            var text = await content.GetTextAsync();
            if (string.IsNullOrWhiteSpace(text)) return null;
            return Uri.TryCreate(text.Trim(), UriKind.Absolute, out var uri)
                && uri.Scheme is "http" or "https"
                ? new(ClipboardHistoryKind.Url, Text: uri.AbsoluteUri)
                : new(ClipboardHistoryKind.Text, Text: text);
        }

        return null;
    }

    private static string? TryGetClipboardOwnerProcessName()
    {
        try
        {
            var window = GetClipboardOwner();
            if (window == IntPtr.Zero) return null;
            _ = GetWindowThreadProcessId(window, out var processId);
            return processId == 0 ? null : Process.GetProcessById((int)processId).ProcessName;
        }
        catch
        {
            return null;
        }
    }

    public void Dispose()
    {
        Stop();
        _captureGate.Dispose();
    }

    [DllImport("user32.dll")]
    private static extern IntPtr GetClipboardOwner();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
}
