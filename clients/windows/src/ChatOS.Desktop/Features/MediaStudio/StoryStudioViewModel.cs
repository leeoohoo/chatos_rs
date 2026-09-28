using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed partial class StoryStudioViewModel : ObservableObject
{
    private const int MaximumImageBytes = 20 * 1024 * 1024;
    private readonly IMediaGenerationService _media;
    private readonly IStoryPlanningService _planner;
    private readonly MediaStudioHistoryStore _history;
    private readonly StoryProjectStore _store;
    private readonly SemaphoreSlim _projectSaveGate = new(1, 1);
    private string? _ownerUserId;
    private Guid _session = Guid.NewGuid();
    private StoryProjectDocument? _current;
    private CancellationTokenSource? _generationCancellation;
    private string? _lockedPlanningSource;
    private string? _lockedPlanningStyle;
    private string? _lockedPlanningRatio;

    public StoryStudioViewModel(
        IMediaGenerationService media,
        IStoryPlanningService planner,
        MediaStudioHistoryStore history,
        StoryProjectStore store)
    {
        _media = media;
        _planner = planner;
        _history = history;
        _store = store;
        Segments.CollectionChanged += (_, _) =>
        {
            UpdatePlanningInputLock();
            RenumberSegments();
            OnPropertyChanged(nameof(CanQuickSplit));
            OnPropertyChanged(nameof(CanPlan));
            OnPropertyChanged(nameof(WorkspaceSummary));
            NotifyBatchPlanChanged();
            RefreshPromptAudit();
            RefreshStoryRelations();
            RefreshContinuityAudit();
            RefreshProjectMedia();
            NotifySegmentOrderChanged();
            NotifyPlanningRunsChanged();
            NotifyOptimizationChanged();
            NotifySegmentRefinementChanged();
            NotifyVideoJobChanged();
            OnPropertyChanged(nameof(IsPlanningInputsLocked));
            OnPropertyChanged(nameof(CanEditPlanningInputs));
            OnPropertyChanged(nameof(PlanningInputsLockLabel));
        };
        Resources.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(CanGenerateResourceImage));
            NotifyBatchPlanChanged();
            RefreshPromptAudit();
            RefreshStoryRelations();
            RefreshContinuityAudit();
            NotifyPlanningRunsChanged();
            NotifyOptimizationChanged();
            NotifySegmentRefinementChanged();
        };
    }

    public ObservableCollection<StoryProjectCard> Projects { get; } = [];
    public ObservableCollection<StorySegmentEditor> Segments { get; } = [];
    public ObservableCollection<StoryResourceEditor> Resources { get; } = [];
    public ObservableCollection<MediaGenerationModel> Models { get; } = [];
    public ObservableCollection<MediaGenerationModel> ImageModels { get; } = [];
    public ObservableCollection<MediaGenerationModel> VideoModels { get; } = [];
    public IReadOnlyList<string> Ratios => StoryStudioOptions.Ratios;
    public IReadOnlyList<StoryResourceKind> ResourceKinds { get; } = Enum.GetValues<StoryResourceKind>();
    public IReadOnlyList<StorySegmentKindOption> SegmentKinds { get; } =
    [
        new(StorySegmentKind.Story, "剧情"),
        new(StorySegmentKind.Transition, "转场"),
    ];

    public bool IsWorkspaceOpen => _current is not null;
    public bool IsPlanningInputsLocked => Segments.Count > 0;
    public bool CanEditPlanningInputs => !IsBusy && !IsPlanningInputsLocked;
    public string PlanningInputsLockLabel => IsPlanningInputsLocked
        ? "全剧计划已建立，剧情原文、画面风格和画面比例已锁定；先删除全部分段才能重新规划。"
        : string.Empty;
    public bool CanCreate => !IsBusy && !string.IsNullOrWhiteSpace(NewTitle) &&
        NewTextModel is not null && NewImageModel is not null && NewVideoModel is not null;
    public bool CanSave => !IsBusy && _current is not null && !string.IsNullOrWhiteSpace(ProjectTitle) &&
        ProjectTextModel is not null && ProjectImageModel is not null && ProjectVideoModel is not null;
    public bool CanQuickSplit => CanSave && Segments.Count == 0 &&
        !string.IsNullOrWhiteSpace(ProjectSource) && _planningRunsReady && ResumablePlanningRun is null;
    public bool CanPlan => CanQuickSplit && Resources.Count == 0 && ProjectTextModel is not null;
    public bool CanGenerateFrame => CanSave && SelectedSegment is not null &&
        !string.IsNullOrWhiteSpace(SelectedSegment.ImagePrompt);
    public bool CanGenerateVideo => CanSave && SelectedSegment is not null &&
        !SelectedSegment.HasPendingVideoJob && !string.IsNullOrWhiteSpace(SelectedSegment.VideoPrompt);
    public bool CanGenerateResourceImage => CanSave && SelectedResource is not null &&
        !string.IsNullOrWhiteSpace(SelectedResource.ImagePrompt);
    public string WorkspaceSummary => _current is null
        ? "选择或新建剧情项目"
        : $"{Segments.Count} 个分段 · {Segments.Sum(segment => segment.Seconds)} 秒";
    public string VideoProgressLabel => VideoProgress switch
    {
        { Status: "downloading" } => "正在下载视频…",
        { Percent: { } percent } => $"{VideoProgress.Status} · {percent:0}%",
        { } value => value.Status,
        _ => string.Empty,
    };

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private string _newTitle = string.Empty;
    [ObservableProperty] private string _newDescription = string.Empty;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private MediaGenerationModel? _newTextModel;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private MediaGenerationModel? _newImageModel;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    private MediaGenerationModel? _newVideoModel;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    private string _projectTitle = string.Empty;
    [ObservableProperty] private string _projectDescription = string.Empty;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanQuickSplit))]
    [NotifyPropertyChangedFor(nameof(CanPlan))]
    private string _projectSource = string.Empty;
    [ObservableProperty] private string _projectSummary = string.Empty;
    [ObservableProperty] private string _visualStyle = "自然光，电影感，保持角色外观、服装与场景一致";
    [ObservableProperty] private string _projectRatio = "16:9";
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanQuickSplit))]
    [NotifyPropertyChangedFor(nameof(CanPlan))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    private MediaGenerationModel? _projectTextModel;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanQuickSplit))]
    [NotifyPropertyChangedFor(nameof(CanPlan))]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    private MediaGenerationModel? _projectImageModel;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanQuickSplit))]
    [NotifyPropertyChangedFor(nameof(CanPlan))]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    [NotifyPropertyChangedFor(nameof(CanResumeSelectedVideo))]
    [NotifyPropertyChangedFor(nameof(SelectedVideoJobLabel))]
    private MediaGenerationModel? _projectVideoModel;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateFrame))]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    [NotifyPropertyChangedFor(nameof(CanResumeSelectedVideo))]
    [NotifyPropertyChangedFor(nameof(CanAbandonSelectedVideoJob))]
    [NotifyPropertyChangedFor(nameof(SelectedVideoJobLabel))]
    [NotifyPropertyChangedFor(nameof(CanExtractSelectedVideoLastFrame))]
    [NotifyPropertyChangedFor(nameof(CanRestoreArchivedVideo))]
    [NotifyPropertyChangedFor(nameof(CanImportSegmentAsset))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    [NotifyPropertyChangedFor(nameof(CanApplySegmentRefinement))]
    private StorySegmentEditor? _selectedSegment;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanGenerateResourceImage))]
    [NotifyPropertyChangedFor(nameof(CanImportResourceAsset))]
    private StoryResourceEditor? _selectedResource;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanCreate))]
    [NotifyPropertyChangedFor(nameof(CanSave))]
    [NotifyPropertyChangedFor(nameof(CanQuickSplit))]
    [NotifyPropertyChangedFor(nameof(CanPlan))]
    [NotifyPropertyChangedFor(nameof(CanGenerateFrame))]
    [NotifyPropertyChangedFor(nameof(CanGenerateVideo))]
    [NotifyPropertyChangedFor(nameof(CanGenerateResourceImage))]
    [NotifyPropertyChangedFor(nameof(CanStartBatch))]
    [NotifyPropertyChangedFor(nameof(CanImportResourceAsset))]
    [NotifyPropertyChangedFor(nameof(CanImportSegmentAsset))]
    [NotifyPropertyChangedFor(nameof(CanAutoFillContinuity))]
    [NotifyPropertyChangedFor(nameof(CanPlayStoryPlaylist))]
    [NotifyPropertyChangedFor(nameof(CanMoveSegmentUp))]
    [NotifyPropertyChangedFor(nameof(CanMoveSegmentDown))]
    [NotifyPropertyChangedFor(nameof(CanResumePlanning))]
    [NotifyPropertyChangedFor(nameof(CanAbandonPlanning))]
    [NotifyPropertyChangedFor(nameof(CanOptimizeStorySource))]
    [NotifyPropertyChangedFor(nameof(CanOptimizeVisualStyle))]
    [NotifyPropertyChangedFor(nameof(CanApplyOptimization))]
    [NotifyPropertyChangedFor(nameof(CanRefineSelectedSegment))]
    [NotifyPropertyChangedFor(nameof(CanApplySegmentRefinement))]
    [NotifyPropertyChangedFor(nameof(CanEditPlanningInputs))]
    [NotifyPropertyChangedFor(nameof(CanResumeSelectedVideo))]
    [NotifyPropertyChangedFor(nameof(CanAbandonSelectedVideoJob))]
    [NotifyPropertyChangedFor(nameof(CanExtractSelectedVideoLastFrame))]
    [NotifyPropertyChangedFor(nameof(CanRestoreArchivedVideo))]
    private bool _isBusy;
    [ObservableProperty] private string _statusMessage = "剧情项目只保存在本机";
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(VideoProgressLabel))]
    private VideoGenerationProgress? _videoProgress;

    public async Task OpenAsync(string ownerUserId, CancellationToken cancellationToken = default)
    {
        if (string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal) && Models.Count > 0) return;
        Reset(ownerUserId);
        var session = _session;
        IsBusy = true;
        try
        {
            var projectsTask = _store.LoadAsync(ownerUserId, cancellationToken);
            var modelsTask = _media.FetchModelsAsync(cancellationToken);
            await Task.WhenAll(projectsTask, modelsTask);
            if (session != _session) return;
            ApplyModels(modelsTask.Result);
            foreach (var project in projectsTask.Result) Projects.Add(new StoryProjectCard(project));
            StatusMessage = Projects.Count == 0 ? "创建第一个剧情项目" : $"已加载 {Projects.Count} 个剧情项目";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = $"读取剧情工作台失败：{exception.Message}";
        }
        finally
        {
            if (session == _session) IsBusy = false;
        }
    }

    public async Task CreateProjectAsync(CancellationToken cancellationToken = default)
    {
        var owner = _ownerUserId;
        if (!CanCreate || owner is null || NewTextModel is null || NewImageModel is null || NewVideoModel is null) return;
        var now = DateTimeOffset.UtcNow;
        var project = new StoryProjectDocument(
            Guid.NewGuid(), StoryProjectDocument.CurrentVersion, NewTitle.Trim(), NewDescription.Trim(), string.Empty, string.Empty,
            "自然光，电影感，保持角色外观、服装与场景一致", "16:9",
            NewTextModel.Id, NewImageModel.Id, NewVideoModel.Id, [], now, now);
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await _store.SaveAsync(owner, project, cancellationToken);
            Projects.Insert(0, new StoryProjectCard(project));
            NewTitle = string.Empty;
            NewDescription = string.Empty;
            OpenProject(project);
            StatusMessage = "剧情项目已创建";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }

    public void OpenProject(StoryProjectDocument project)
    {
        _current = project;
        ClearOptimizationSuggestion();
        ClearSegmentRefinement();
        _planningRunsReady = false;
        PlanningRuns.Clear();
        ProjectTitle = project.Title;
        ProjectDescription = project.Description;
        ProjectSource = project.Source;
        ProjectSummary = project.Summary ?? string.Empty;
        VisualStyle = project.VisualStyle;
        ProjectRatio = project.Ratio;
        ProjectTextModel = Models.FirstOrDefault(model => model.Id == project.TextModelConfigId);
        ProjectImageModel = ImageModels.FirstOrDefault(model => model.Id == project.ImageModelConfigId);
        ProjectVideoModel = VideoModels.FirstOrDefault(model => model.Id == project.VideoModelConfigId);
        Segments.Clear();
        Resources.Clear();
        foreach (var resource in project.Resources)
        {
            Resources.Add(new StoryResourceEditor(resource, asset =>
                _ownerUserId is null ? null : _store.ResolveAssetPath(_ownerUserId, project.Id, asset)));
        }
        foreach (var segment in project.Segments)
        {
            Segments.Add(new StorySegmentEditor(segment, asset =>
                _ownerUserId is null ? null : _store.ResolveAssetPath(_ownerUserId, project.Id, asset)));
        }
        SelectedSegment = Segments.FirstOrDefault();
        SelectedResource = Resources.FirstOrDefault();
        ErrorMessage = null;
        OnWorkspaceChanged();
        NotifyPlanningRunsChanged();
        if (_ownerUserId is { } owner)
            _ = LoadPlanningRunsAsync(owner, project.Id, _session);
    }

    public void CloseProject()
    {
        CancelPlanningForWorkspaceChange();
        _generationCancellation?.Cancel();
        _current = null;
        ClearOptimizationSuggestion();
        ClearSegmentRefinement();
        _planningRunsReady = false;
        Segments.Clear();
        Resources.Clear();
        PlanningRuns.Clear();
        SelectedSegment = null;
        SelectedResource = null;
        OnWorkspaceChanged();
        NotifyPlanningRunsChanged();
    }

    public async Task SaveCurrentAsync(CancellationToken cancellationToken = default)
    {
        if (!CanSave) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            StatusMessage = "剧情项目已保存";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }

    public void AddSegment()
    {
        if (_current is null || IsBusy || Segments.Count >= 200) return;
        var previous = Segments.LastOrDefault();
        var number = Segments.Count + 1;
        var segment = new StorySegmentDocument(
            $"segment-{Guid.NewGuid():N}", $"分段 {number}", string.Empty, string.Empty, string.Empty,
            4, null, null, null);
        var editor = new StorySegmentEditor(segment, _ => null);
        Segments.Add(editor);
        if (previous is not null) previous.ContinuityOut = string.Empty;
        SelectedSegment = editor;
        RefreshContinuityAudit();
    }

    public void RemoveSelectedSegment()
    {
        if (SelectedSegment is null || IsBusy) return;
        var index = Segments.IndexOf(SelectedSegment);
        Segments.Remove(SelectedSegment);
        SelectedSegment = Segments.Count == 0 ? null : Segments[Math.Clamp(index, 0, Segments.Count - 1)];
        RepairContinuityRange(index - 1, index);
    }

    public void QuickSplit()
    {
        if (!CanQuickSplit) return;
        var chunks = SplitSource(ProjectSource).Take(200).ToArray();
        for (var index = 0; index < chunks.Length; index++)
        {
            var narrative = chunks[index];
            var title = narrative.Length <= 24 ? narrative : $"镜头 {index + 1}";
            var imagePrompt = $"{VisualStyle}。画面内容：{narrative}";
            var videoPrompt = $"{narrative}。保持人物与场景一致，镜头运动自然。";
            Segments.Add(new StorySegmentEditor(new StorySegmentDocument(
                $"segment-{Guid.NewGuid():N}", title, narrative, imagePrompt, videoPrompt,
                4, null, null, null), _ => null));
        }
        SelectedSegment = Segments.FirstOrDefault();
        FillMissingContinuity();
        StatusMessage = chunks.Length == 0 ? "原文中没有可分段内容" : $"已生成 {chunks.Length} 个可编辑分段";
    }

    public Task PlanStoryAsync(CancellationToken cancellationToken = default) =>
        StartPlanningRunAsync(cancellationToken);

    public Task GenerateFirstFrameAsync(CancellationToken cancellationToken = default) =>
        GenerateFrameAsync(false, cancellationToken);

    public Task GenerateLastFrameAsync(CancellationToken cancellationToken = default) =>
        GenerateFrameAsync(true, cancellationToken);

    public void CancelGeneration() => _generationCancellation?.Cancel();

    private async Task GenerateFrameAsync(bool lastFrame, CancellationToken cancellationToken)
    {
        var context = CaptureGenerationContext();
        if (!CanGenerateFrame || context is null || ProjectImageModel is null) return;
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await PersistCurrentAsync(cancellationToken);
            await GenerateFrameCoreAsync(context, ProjectImageModel, lastFrame, cancellationToken);
            var role = lastFrame ? "尾帧" : "首帧";
            StatusMessage = $"{context.Segment.Title} 的{role}已生成";
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            ErrorMessage = exception.Message;
        }
        finally { IsBusy = false; }
    }

    private async Task PersistCurrentAsync(CancellationToken cancellationToken)
    {
        await _projectSaveGate.WaitAsync(cancellationToken);
        try
        {
            var owner = _ownerUserId ?? throw new InvalidOperationException("请先登录。");
            var current = _current ?? throw new InvalidOperationException("请先打开剧情项目。");
            var session = _session;
            if (ProjectTextModel is null || ProjectImageModel is null || ProjectVideoModel is null)
                throw new InvalidOperationException("请选择文本、图片和视频模型。");
            if (IsPlanningInputsLocked &&
                (!string.Equals(ProjectSource.Trim(), _lockedPlanningSource, StringComparison.Ordinal) ||
                 !string.Equals(VisualStyle.Trim(), _lockedPlanningStyle, StringComparison.Ordinal) ||
                 !string.Equals(ProjectRatio, _lockedPlanningRatio, StringComparison.Ordinal)))
            {
                throw new InvalidOperationException("全剧计划建立后不能修改剧情原文、画面风格或画面比例；请先删除全部分段再重新规划。");
            }
            var project = current with
            {
                Title = ProjectTitle.Trim(),
                Description = ProjectDescription.Trim(),
                Source = ProjectSource.Trim(),
                Summary = ProjectSummary.Trim(),
                VisualStyle = VisualStyle.Trim(),
                Ratio = ProjectRatio,
                TextModelConfigId = ProjectTextModel.Id,
                ImageModelConfigId = ProjectImageModel.Id,
                VideoModelConfigId = ProjectVideoModel.Id,
                Segments = Segments.Select(segment => segment.ToDocument()).ToArray(),
                Resources = Resources.Select(resource => resource.ToDocument()).ToArray(),
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await _store.SaveAsync(owner, project, cancellationToken);
            if (_session != session || _ownerUserId != owner || _current?.Id != project.Id) return;
            _current = project;
            var index = Projects.ToList().FindIndex(card => card.Id == project.Id);
            if (index >= 0) Projects.RemoveAt(index);
            Projects.Insert(0, new StoryProjectCard(project));
            OnWorkspaceChanged();
        }
        finally
        {
            _projectSaveGate.Release();
        }
    }

    private void ApplyModels(IReadOnlyList<MediaGenerationModel> models)
    {
        Models.Clear();
        ImageModels.Clear();
        VideoModels.Clear();
        foreach (var model in models)
        {
            Models.Add(model);
            if (model.IsLikelyVideoModel) VideoModels.Add(model);
            else ImageModels.Add(model);
        }
        NewTextModel = Models.FirstOrDefault();
        NewImageModel = ImageModels.FirstOrDefault();
        NewVideoModel = VideoModels.FirstOrDefault();
    }

    private void Reset(string ownerUserId)
    {
        CancelPlanningForWorkspaceChange();
        _generationCancellation?.Cancel();
        _generationCancellation?.Dispose();
        _generationCancellation = null;
        _session = Guid.NewGuid();
        _ownerUserId = ownerUserId;
        _current = null;
        ClearPlanningInputLock();
        ClearOptimizationSuggestion();
        ClearSegmentRefinement();
        _planningRunsReady = false;
        Projects.Clear();
        Segments.Clear();
        Resources.Clear();
        PlanningRuns.Clear();
        SelectedSegment = null;
        SelectedResource = null;
        Models.Clear();
        ImageModels.Clear();
        VideoModels.Clear();
        ErrorMessage = null;
        VideoProgress = null;
        OnWorkspaceChanged();
        NotifyPlanningRunsChanged();
    }

    private GenerationContext? CaptureGenerationContext() =>
        _ownerUserId is { } owner && _current is { } project && SelectedSegment is { } segment
            ? new GenerationContext(owner, project.Id, segment, _session)
            : null;

    private void EnsureContext(GenerationContext context)
    {
        if (context.Session != _session || _ownerUserId != context.Owner ||
            _current?.Id != context.ProjectId || !Segments.Contains(context.Segment))
            throw new OperationCanceledException("剧情项目或登录账户已切换。");
    }

    private static async Task<ImageGenerationInput?> LoadFrameAsync(
        string? path,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(path) || !File.Exists(path)) return null;
        var bytes = await File.ReadAllBytesAsync(path, cancellationToken);
        if (bytes.Length is <= 0 or > MaximumImageBytes) throw new InvalidDataException("剧情帧为空或超过 20 MB。");
        var mime = Path.GetExtension(path).ToLowerInvariant() switch
        {
            ".jpg" or ".jpeg" => "image/jpeg",
            ".webp" => "image/webp",
            _ => "image/png",
        };
        return new ImageGenerationInput(Path.GetFileName(path), mime, Convert.ToBase64String(bytes));
    }

    private static IEnumerable<string> SplitSource(string source)
    {
        var paragraphs = source.Replace("\r\n", "\n").Split('\n', StringSplitOptions.TrimEntries | StringSplitOptions.RemoveEmptyEntries);
        foreach (var paragraph in paragraphs)
        {
            if (paragraph.Length <= 700) { yield return paragraph; continue; }
            for (var offset = 0; offset < paragraph.Length; offset += 700)
                yield return paragraph.Substring(offset, Math.Min(700, paragraph.Length - offset));
        }
    }

    private static string ImageSize(string ratio) => ratio switch
    {
        "9:16" or "3:4" => "1024x1536",
        "16:9" or "4:3" or "21:9" => "1536x1024",
        _ => "1024x1024",
    };

    private void RenumberSegments()
    {
        for (var index = 0; index < Segments.Count; index++)
        {
            Segments[index].SetNumberLabel($"{index + 1:00}");
        }
    }

    partial void OnSelectedSegmentChanged(StorySegmentEditor? oldValue, StorySegmentEditor? newValue)
    {
        if (oldValue is not null) oldValue.PropertyChanged -= OnSelectedSegmentPropertyChanged;
        if (newValue is not null) newValue.PropertyChanged += OnSelectedSegmentPropertyChanged;
        ClearSegmentRefinement();
        NotifySegmentOrderChanged();
        RefreshPromptAudit();
    }

    private void OnSelectedSegmentPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        OnPropertyChanged(nameof(CanGenerateFrame));
        OnPropertyChanged(nameof(CanGenerateVideo));
        OnPropertyChanged(nameof(CanResumeSelectedVideo));
        OnPropertyChanged(nameof(CanAbandonSelectedVideoJob));
        OnPropertyChanged(nameof(SelectedVideoJobLabel));
        OnPropertyChanged(nameof(CanExtractSelectedVideoLastFrame));
        OnPropertyChanged(nameof(CanRestoreArchivedVideo));
        OnPropertyChanged(nameof(CanGenerateResourceImage));
        if (e.PropertyName == nameof(StorySegmentEditor.Seconds))
            OnPropertyChanged(nameof(WorkspaceSummary));
        NotifyBatchPlanChanged();
        RefreshPromptAudit();
        RefreshStoryRelations();
        RefreshContinuityAudit();
        RefreshProjectMedia();
        NotifySegmentOrderChanged();
        NotifySegmentRefinementChanged();
        NotifyVideoJobChanged();
    }

    private void OnWorkspaceChanged()
    {
        OnPropertyChanged(nameof(IsWorkspaceOpen));
        OnPropertyChanged(nameof(CanSave));
        OnPropertyChanged(nameof(CanQuickSplit));
        OnPropertyChanged(nameof(CanPlan));
        OnPropertyChanged(nameof(CanGenerateFrame));
        OnPropertyChanged(nameof(CanGenerateVideo));
        OnPropertyChanged(nameof(WorkspaceSummary));
        OnPropertyChanged(nameof(IsPlanningInputsLocked));
        OnPropertyChanged(nameof(CanEditPlanningInputs));
        OnPropertyChanged(nameof(PlanningInputsLockLabel));
        NotifyBatchPlanChanged();
        RefreshPromptAudit();
        RefreshStoryRelations();
        RefreshContinuityAudit();
        RefreshProjectMedia();
        NotifySegmentOrderChanged();
        NotifySegmentRefinementChanged();
        NotifyVideoJobChanged();
    }

    private void UpdatePlanningInputLock()
    {
        if (Segments.Count == 0)
        {
            ClearPlanningInputLock();
            return;
        }
        _lockedPlanningSource ??= ProjectSource.Trim();
        _lockedPlanningStyle ??= VisualStyle.Trim();
        _lockedPlanningRatio ??= ProjectRatio;
    }

    private void ClearPlanningInputLock()
    {
        _lockedPlanningSource = null;
        _lockedPlanningStyle = null;
        _lockedPlanningRatio = null;
    }

    private async Task GenerateFrameCoreAsync(
        GenerationContext context,
        MediaGenerationModel imageModel,
        bool lastFrame,
        CancellationToken cancellationToken)
    {
        EnsureContext(context);
        var prompt = StoryPromptCatalog.RenderFrame(
            VisualStyle,
            context.Segment.ImagePrompt,
            lastFrame,
            ProjectRatio,
            BuildContinuityContext(context.Segment));
        var references = await LoadSegmentReferencesAsync(context, lastFrame, cancellationToken);
        var result = await _media.GenerateImageAsync(
            new ImageGenerationRequest(imageModel.Id, prompt, ImageSize(ProjectRatio), 1, references),
            cancellationToken);
        var history = await _history.SaveAsync(context.Owner, prompt, result, cancellationToken);
        var source = history.Images.First().FilePath;
        var relative = await _store.ImportAssetAsync(
            context.Owner, context.ProjectId, context.Segment.Id, source, false, cancellationToken);
        EnsureContext(context);
        context.Segment.SetFrame(
            lastFrame, relative, _store.ResolveAssetPath(context.Owner, context.ProjectId, relative)!);
        await PersistCurrentAsync(cancellationToken);
        NotifyBatchPlanChanged();
    }

    private async Task<IReadOnlyList<ImageGenerationInput>> LoadSegmentReferencesAsync(
        GenerationContext context,
        bool includeFirstFrame,
        CancellationToken cancellationToken)
    {
        var resourceIds = context.Segment.ToDocument().ResourceIds.ToHashSet(StringComparer.Ordinal);
        var inputs = new List<ImageGenerationInput>();
        if (includeFirstFrame)
        {
            var firstFrame = await LoadFrameAsync(context.Segment.FirstFramePath, cancellationToken);
            if (firstFrame is not null) inputs.Add(firstFrame);
        }
        else
        {
            var index = Segments.IndexOf(context.Segment);
            var previousTail = index > 0
                ? await LoadFrameAsync(Segments[index - 1].ActualVideoLastFramePath, cancellationToken)
                : null;
            if (previousTail is not null) inputs.Add(previousTail);
        }
        foreach (var resource in Resources.Where(resource => resourceIds.Contains(resource.Id)).Take(8 - inputs.Count))
        {
            var input = await LoadFrameAsync(resource.ImagePath, cancellationToken);
            if (input is not null) inputs.Add(input);
        }
        return inputs;
    }

    private sealed record GenerationContext(
        string Owner,
        Guid ProjectId,
        StorySegmentEditor Segment,
        Guid Session);
}
