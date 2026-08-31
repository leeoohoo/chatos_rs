using System.Diagnostics;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.Plugins;

public sealed class WindowsPluginArtifactUserInteraction : IPluginArtifactUserInteraction
{
    public string CacheDirectory { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "ChatOS",
        "ArtifactCache");

    public async Task<string?> PickSavePathAsync(
        string suggestedFileName,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var window = (Microsoft.UI.Xaml.Application.Current as App)?.MainWindow
            ?? throw new InvalidOperationException("The ChatOS main window is unavailable.");
        var picker = new FileSavePicker
        {
            SuggestedStartLocation = PickerLocationId.DocumentsLibrary,
            SuggestedFileName = Path.GetFileNameWithoutExtension(suggestedFileName),
        };
        var extension = Path.GetExtension(suggestedFileName);
        if (string.IsNullOrWhiteSpace(extension) || extension.Length > 16 ||
            extension.Any(character => !char.IsLetterOrDigit(character) && character != '.'))
        {
            extension = ".bin";
        }

        picker.FileTypeChoices.Add("Artifact", [extension]);
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSaveFileAsync();
        cancellationToken.ThrowIfCancellationRequested();
        return file?.Path;
    }

    public Task OpenFileAsync(string path, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Process.Start(new ProcessStartInfo(Path.GetFullPath(path))
        {
            UseShellExecute = true,
        });
        return Task.CompletedTask;
    }
}
