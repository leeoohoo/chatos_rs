namespace ChatOS.Core.Abstractions;

public sealed record PetWindowPlacement(int AnchorX, int AnchorY);

public interface IPetWindowPlacementStore
{
    Task<PetWindowPlacement?> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(PetWindowPlacement placement, CancellationToken cancellationToken = default);
}
