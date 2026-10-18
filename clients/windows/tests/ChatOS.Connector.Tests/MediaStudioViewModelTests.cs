using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.MediaStudio;

namespace ChatOS.Connector.Tests;

public sealed class MediaStudioViewModelTests
{
    [Fact]
    public async Task ResetDiscardsAnImageResultThatCompletesAfterTheAccountSessionEnds()
    {
        using var folder = new TemporaryFolder();
        var service = new MediaService();
        var store = new MediaStudioHistoryStore(new NoNetworkHttpClientFactory(), folder.Path);
        var viewModel = new MediaStudioViewModel(service, store);
        await viewModel.OpenAsync("owner-a");
        viewModel.Prompt = "draw a fox";
        var generation = viewModel.GenerateAsync();
        await service.GenerationEntered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        viewModel.Reset();
        service.Generation.SetResult(new ImageGenerationResult(
            "late-result",
            "image-model",
            "gpt-image-1.5",
            DateTimeOffset.UtcNow,
            [new GeneratedMediaAsset("image", "image/png", "AQID", null, null)]));
        await generation;

        Assert.Empty(viewModel.History);
        Assert.Empty(viewModel.LatestImages);
        Assert.Null(viewModel.LatestImageBatch);
        Assert.False(viewModel.IsGeneratingImage);
        Assert.False(viewModel.IsBusy);
        Assert.Empty(Directory.EnumerateFileSystemEntries(folder.Path));
    }

    [Fact]
    public async Task ResetDiscardsAVideoResultThatCompletesAfterTheAccountSessionEnds()
    {
        using var folder = new TemporaryFolder();
        var service = new MediaService();
        var store = new MediaStudioHistoryStore(new NoNetworkHttpClientFactory(), folder.Path);
        var viewModel = new MediaStudioViewModel(service, store);
        await viewModel.OpenAsync("owner-a");
        viewModel.VideoPrompt = "camera tracks a fox";
        var generation = viewModel.GenerateVideoAsync();
        await service.VideoGenerationEntered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        viewModel.Reset();
        service.VideoGeneration.SetResult(VideoResult());
        await generation;

        Assert.Empty(viewModel.VideoHistory);
        Assert.Null(viewModel.LatestVideo);
        Assert.Null(viewModel.VideoProgress);
        Assert.False(viewModel.IsGeneratingVideo);
        Assert.Empty(Directory.EnumerateFileSystemEntries(folder.Path));
    }

    [Fact]
    public async Task ResetIgnoresLateVideoProgressFromThePreviousAccountSession()
    {
        using var folder = new TemporaryFolder();
        var service = new MediaService();
        var store = new MediaStudioHistoryStore(new NoNetworkHttpClientFactory(), folder.Path);
        var viewModel = new MediaStudioViewModel(service, store);
        await viewModel.OpenAsync("owner-a");
        viewModel.VideoPrompt = "camera tracks a fox";

        var generation = viewModel.GenerateVideoAsync();
        await service.VideoGenerationEntered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        viewModel.Reset();
        service.VideoProgress!.Report(new VideoGenerationProgress("in_progress", 62));
        await Task.Delay(100);

        Assert.Null(viewModel.VideoProgress);
        service.VideoGeneration.SetResult(VideoResult());
        await generation;
        Assert.Null(viewModel.VideoProgress);
    }

    [Fact]
    public async Task ImageGenerationReportsSaveFailureSeparatelyFromGenerationFailure()
    {
        using var folder = new TemporaryFolder();
        var service = new MediaService { ReturnInvalidImageData = true };
        var store = new MediaStudioHistoryStore(new NoNetworkHttpClientFactory(), folder.Path);
        var viewModel = new MediaStudioViewModel(service, store);
        await viewModel.OpenAsync("owner-a");
        viewModel.Prompt = "draw a fox";
        service.Generation.SetResult(new ImageGenerationResult(
            "generated", "image-model", "gpt-image-1.5", DateTimeOffset.UtcNow,
            [new GeneratedMediaAsset("image", "image/png", "AQID", null, null)]));

        await viewModel.GenerateAsync();

        Assert.Contains("图片已生成，但未能保存到本机", viewModel.ErrorMessage);
        Assert.Equal("图片已生成，但保存失败", viewModel.StatusMessage);
        Assert.False(viewModel.IsGeneratingImage);
    }

    [Fact]
    public async Task VideoGenerationReportsSaveFailureSeparatelyFromGenerationFailure()
    {
        using var folder = new TemporaryFolder();
        var service = new MediaService { ReturnEmptyVideoData = true };
        var store = new MediaStudioHistoryStore(new NoNetworkHttpClientFactory(), folder.Path);
        var viewModel = new MediaStudioViewModel(service, store);
        await viewModel.OpenAsync("owner-a");
        viewModel.VideoPrompt = "camera tracks a fox";
        service.VideoGeneration.SetResult(VideoResult());

        await viewModel.GenerateVideoAsync();

        Assert.Contains("视频已生成，但未能保存到本机", viewModel.ErrorMessage);
        Assert.False(viewModel.IsGeneratingVideo);
    }

    private static VideoGenerationResult VideoResult() => new(
        "late-video",
        "video-model",
        "minimax-h3",
        DateTimeOffset.UtcNow,
        "video/mp4",
        [1, 2, 3]);

    private sealed class MediaService : IMediaGenerationService
    {
        public TaskCompletionSource GenerationEntered { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource<ImageGenerationResult> Generation { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource VideoGenerationEntered { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource<VideoGenerationResult> VideoGeneration { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        public IProgress<VideoGenerationProgress>? VideoProgress { get; private set; }
        public bool ReturnInvalidImageData { get; init; }
        public bool ReturnEmptyVideoData { get; init; }

        public Task<IReadOnlyList<MediaGenerationModel>> FetchModelsAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<MediaGenerationModel>>(
            [
                new("image-model", "Image", "openai", "gpt-image-1.5", true, true, true),
                new("video-model", "Video", "minimax", "minimax-h3", true, true, true),
            ]);

        public Task<ImageGenerationResult> GenerateImageAsync(
            ImageGenerationRequest request,
            CancellationToken cancellationToken = default)
        {
            GenerationEntered.TrySetResult();
            return Generation.Task.ContinueWith(task => ReturnInvalidImageData
                ? task.Result with { Images = [new GeneratedMediaAsset("broken", "image/png", "not-base64", null, null)] }
                : task.Result, TaskScheduler.Default); // Ignore cancellation to verify the session guard.
        }

        public Task<VideoGenerationResult> GenerateVideoAsync(
            VideoGenerationRequest request,
            IProgress<VideoGenerationProgress>? progress = null,
            CancellationToken cancellationToken = default)
        {
            VideoProgress = progress;
            VideoGenerationEntered.TrySetResult();
            return VideoGeneration.Task.ContinueWith(task => ReturnEmptyVideoData
                ? task.Result with { VideoData = [] }
                : task.Result, TaskScheduler.Default); // Ignore cancellation to verify the session guard.
        }

        public Task<VideoGenerationResult> ResumeVideoAsync(
            VideoGenerationRequest request,
            string jobId,
            IProgress<VideoGenerationProgress>? progress = null,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class NoNetworkHttpClientFactory : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) =>
            throw new InvalidOperationException("The test should not access the network.");
    }

    private sealed class TemporaryFolder : IDisposable
    {
        public TemporaryFolder()
        {
            Path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                "chatos-media-view-model-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(Path);
        }

        public string Path { get; }

        public void Dispose()
        {
            if (Directory.Exists(Path)) Directory.Delete(Path, true);
        }
    }
}
