using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryResourceEditor : ObservableObject
{
    public StoryResourceEditor(StoryResourceDocument document, Func<string?, string?> resolvePath)
    {
        Id = document.Id;
        _kind = document.Kind;
        _name = document.Name;
        _description = document.Description;
        _imagePrompt = document.ImagePrompt;
        _imageAsset = document.ImageAsset;
        ImagePath = resolvePath(document.ImageAsset);
    }

    public string Id { get; }
    public string? ImagePath { get; private set; }
    public string KindLabel => Kind switch
    {
        StoryResourceKind.Character => "角色",
        StoryResourceKind.Scene => "场景",
        _ => "道具",
    };

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(KindLabel))]
    private StoryResourceKind _kind;
    [ObservableProperty] private string _name;
    [ObservableProperty] private string _description;
    [ObservableProperty] private string _imagePrompt;
    private string? _imageAsset;

    public StoryResourceDocument ToDocument() => new(
        Id,
        Kind,
        Name.Trim(),
        Description.Trim(),
        ImagePrompt.Trim(),
        _imageAsset);

    internal void SetImage(string relativePath, string fullPath)
    {
        _imageAsset = relativePath;
        ImagePath = fullPath;
        OnPropertyChanged(nameof(ImagePath));
    }
}

public sealed partial class StoryStudioViewModel
{
    partial void OnSelectedResourceChanged(StoryResourceEditor? oldValue, StoryResourceEditor? newValue)
    {
        if (oldValue is not null) oldValue.PropertyChanged -= OnSelectedResourcePropertyChanged;
        if (newValue is not null) newValue.PropertyChanged += OnSelectedResourcePropertyChanged;
    }

    private void OnSelectedResourcePropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e) =>
        OnPropertyChanged(nameof(CanGenerateResourceImage));

    public void AddResource()
    {
        if (_current is null || IsBusy || Resources.Count >= 100) return;
        var resource = new StoryResourceEditor(new StoryResourceDocument(
            $"resource-{Guid.NewGuid():N}", StoryResourceKind.Character,
            $"素材 {Resources.Count + 1}", string.Empty, string.Empty, null), _ => null);
        Resources.Add(resource);
        SelectedResource = resource;
    }

    public void RemoveSelectedResource()
    {
        if (SelectedResource is null || IsBusy) return;
        var resourceId = SelectedResource.Id;
        var index = Resources.IndexOf(SelectedResource);
        Resources.Remove(SelectedResource);
        foreach (var segment in Segments) segment.RemoveResource(resourceId);
        SelectedResource = Resources.Count == 0 ? null : Resources[Math.Clamp(index, 0, Resources.Count - 1)];
    }

    public async Task GenerateResourceImageAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        var project = _current;
        var resource = SelectedResource;
        var session = _session;
        if (!CanGenerateResourceImage || owner is null || project is null || resource is null || ProjectImageModel is null) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            var prompt = $"{VisualStyle}\n{resource.ImagePrompt.Trim()}\n生成{resource.KindLabel}“{resource.Name}”的一致性参考图，画面比例 {ProjectRatio}。";
            var result = await _media.GenerateImageAsync(
                new ImageGenerationRequest(ProjectImageModel.Id, prompt, ImageSize(ProjectRatio), 1, []),
                cancellationToken);
            var history = await _history.SaveAsync(owner, prompt, result, cancellationToken);
            var relative = await _store.ImportAssetAsync(
                owner, project.Id, $"resource-{resource.Id}", history.Images.First().FilePath,
                false, cancellationToken);
            if (session != _session || _current?.Id != project.Id || !Resources.Contains(resource))
                throw new OperationCanceledException("剧情项目或登录账户已切换。");
            resource.SetImage(relative, _store.ResolveAssetPath(owner, project.Id, relative)!);
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = $"{resource.KindLabel}“{resource.Name}”的参考图已生成";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }
}
