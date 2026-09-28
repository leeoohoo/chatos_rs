using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IMediaGenerationService
{
    Task<IReadOnlyList<MediaGenerationModel>> FetchModelsAsync(
        CancellationToken cancellationToken = default);

    Task<ImageGenerationResult> GenerateImageAsync(
        ImageGenerationRequest request,
        CancellationToken cancellationToken = default);
}
