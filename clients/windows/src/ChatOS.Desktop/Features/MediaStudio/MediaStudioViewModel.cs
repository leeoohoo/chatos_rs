using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class MediaStudioViewModel : ObservableObject
{
    public static readonly IReadOnlyList<string> AvailableSizes =
        ["auto", "1024x1024", "1536x1024", "1024x1536"];
    public static readonly IReadOnlyList<int> AvailableCounts = [1, 2, 3, 4];
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private readonly IMediaGenerationService _service;
    private readonly MediaStudioHistoryStore _historyStore;
    private string? _ownerUserId;
    private bool _initialized;

    public MediaStudioViewModel(
        IMediaGenerationService service,
        MediaStudioHistoryStore historyStore)
    {
        _service = service;
        _historyStore = historyStore;
    }

    public ObservableCollection<MediaGenerationModel> Models { get; } = [];

    public ObservableCollection<MediaStudioHistoryItem> History { get; } = [];

    public ObservableCollection<MediaStudioImageItem> LatestImages { get; } = [];

    public ObservableCollection<ImageGenerationInput> ReferenceImages { get; } = [];

    public IReadOnlyList<string> Sizes => AvailableSizes;

    public IReadOnlyList<int> Counts => AvailableCounts;

    public bool CanGenerate => !IsBusy && SelectedModel is not null &&
        !string.IsNullOrWhiteSpace(Prompt) && _ownerUserId is not null;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private MediaGenerationModel? _selectedModel;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private string _prompt = string.Empty;

    [ObservableProperty]
    private string _selectedSize = "1024x1024";

    [ObservableProperty]
    private int _selectedCount = 1;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerate))]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _statusMessage = "选择模型并描述想要生成的画面";

    public async Task OpenAsync(string ownerUserId, CancellationToken cancellationToken = default)
    {
        if (!string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal))
        {
            Reset();
            _ownerUserId = ownerUserId;
        }
        if (_initialized) return;
        _initialized = true;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var historyTask = _historyStore.LoadAsync(ownerUserId, cancellationToken);
            var modelsTask = _service.FetchModelsAsync(cancellationToken);
            await Task.WhenAll(historyTask, modelsTask);
            foreach (var item in historyTask.Result) History.Add(item);
            ShowLatest(History.FirstOrDefault());
            ApplyModels(modelsTask.Result);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            _initialized = false;
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task ReloadModelsAsync(CancellationToken cancellationToken = default)
    {
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            ApplyModels(await _service.FetchModelsAsync(cancellationToken));
            StatusMessage = Models.Count == 0
                ? "没有找到已配置的图片模型"
                : $"已加载 {Models.Count} 个可用模型";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task GenerateAsync(CancellationToken cancellationToken = default)
    {
        if (!CanGenerate || SelectedModel is null || _ownerUserId is null) return;
        var submittedPrompt = Prompt.Trim();
        IsBusy = true;
        ErrorMessage = null;
        StatusMessage = "正在生成图片…";
        try
        {
            var result = await _service.GenerateImageAsync(
                new ImageGenerationRequest(
                    SelectedModel.Id,
                    submittedPrompt,
                    SelectedSize == "auto" ? null : SelectedSize,
                    SelectedCount,
                    ReferenceImages.ToArray()),
                cancellationToken);
            var item = await _historyStore.SaveAsync(
                _ownerUserId,
                submittedPrompt,
                result,
                cancellationToken);
            History.Insert(0, item);
            ShowLatest(item);
            StatusMessage = $"已生成并保存 {item.Images.Count} 张图片";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
            StatusMessage = "生成失败";
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task AddReferenceImagesAsync(
        IReadOnlyList<string> paths,
        CancellationToken cancellationToken = default)
    {
        ErrorMessage = null;
        if (ReferenceImages.Count + paths.Count > 8)
        {
            ErrorMessage = "最多可以添加 8 张参考图。";
            return;
        }
        try
        {
            foreach (var path in paths)
            {
                var mimeType = Path.GetExtension(path).ToLowerInvariant() switch
                {
                    ".png" => "image/png",
                    ".jpg" or ".jpeg" => "image/jpeg",
                    ".webp" => "image/webp",
                    _ => throw new InvalidDataException("参考图必须是 PNG、JPEG 或 WebP。"),
                };
                var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
                if (bytes.Length == 0 || bytes.Length > MaximumImageBytes)
                    throw new InvalidDataException("参考图为空或超过 20 MB。");
                ReferenceImages.Add(new ImageGenerationInput(
                    Path.GetFileName(path),
                    mimeType,
                    Convert.ToBase64String(bytes)));
            }
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
    }

    public void RemoveReferenceImage(ImageGenerationInput image) => ReferenceImages.Remove(image);

    public void SelectHistoryItem(MediaStudioHistoryItem item) => ShowLatest(item);

    public void ReportError(string message) => ErrorMessage = message;

    public void Reset()
    {
        _ownerUserId = null;
        _initialized = false;
        Models.Clear();
        History.Clear();
        LatestImages.Clear();
        ReferenceImages.Clear();
        SelectedModel = null;
        Prompt = string.Empty;
        ErrorMessage = null;
        StatusMessage = "选择模型并描述想要生成的画面";
    }

    private void ApplyModels(IReadOnlyList<MediaGenerationModel> values)
    {
        var selectedId = SelectedModel?.Id;
        Models.Clear();
        foreach (var model in values.Where(model => !model.IsLikelyVideoModel)) Models.Add(model);
        SelectedModel = Models.FirstOrDefault(model => model.Id == selectedId) ?? Models.FirstOrDefault();
        if (Models.Count == 0) StatusMessage = "没有找到已配置的图片模型";
    }

    private void ShowLatest(MediaStudioHistoryItem? item)
    {
        LatestImages.Clear();
        if (item is null) return;
        foreach (var image in item.Images) LatestImages.Add(image);
    }
}
