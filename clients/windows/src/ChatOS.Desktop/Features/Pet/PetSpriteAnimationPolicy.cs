using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.Pet;

internal static class PetSpriteAtlasPolicy
{
    public const int Columns = 8;
    public const int Rows = 11;
    public const int CellWidth = 192;
    public const int CellHeight = 208;
    public const int RenderWidth = 180;
    public const int RenderHeight = 195;
    public const int FrameCount = Columns * Rows;
    public const long MaximumFrameBytes = 1024L * 1024L;

    public static string FrameFileName(int row, int column) =>
        $"frame-{row:D2}-{column:D2}.png";
}

internal readonly record struct PetSpriteSequence(
    int Row,
    int FrameCount,
    TimeSpan FrameDuration)
{
    public static PetSpriteSequence For(
        PetAnimationState state,
        bool isDragging,
        bool isDraggingRight) =>
        isDragging
            ? new PetSpriteSequence(isDraggingRight ? 1 : 2, 8, TimeSpan.FromMilliseconds(100))
            : state switch
            {
                PetAnimationState.Succeeded =>
                    new PetSpriteSequence(4, 5, TimeSpan.FromMilliseconds(500)),
                PetAnimationState.Failed =>
                    new PetSpriteSequence(5, 8, TimeSpan.FromMilliseconds(500)),
                PetAnimationState.Waiting =>
                    new PetSpriteSequence(6, 6, TimeSpan.FromMilliseconds(500)),
                PetAnimationState.Running =>
                    new PetSpriteSequence(7, 6, TimeSpan.FromMilliseconds(200)),
                PetAnimationState.Review =>
                    new PetSpriteSequence(8, 6, TimeSpan.FromMilliseconds(250)),
                _ => new PetSpriteSequence(0, 7, TimeSpan.FromSeconds(1)),
            };
}
