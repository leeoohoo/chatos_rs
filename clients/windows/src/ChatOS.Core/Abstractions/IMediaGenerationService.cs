using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IMediaGenerationService
{
    Task<IReadOnlyList<MediaGenerationModel>> FetchModelsAsync(
        CancellationToken cancellationToken = default);

    Task<ImageGenerationResult> GenerateImageAsync(
        ImageGenerationRequest request,
        CancellationToken cancellationToken = default);

    Task<VideoGenerationResult> GenerateVideoAsync(
        VideoGenerationRequest request,
        IProgress<VideoGenerationProgress>? progress = null,
        CancellationToken cancellationToken = default);

    Task<VideoGenerationResult> ResumeVideoAsync(
        VideoGenerationRequest request,
        string jobId,
        IProgress<VideoGenerationProgress>? progress = null,
        CancellationToken cancellationToken = default);
}
