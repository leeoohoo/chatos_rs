using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.Pet;

namespace ChatOS.Connector.Tests;

public sealed class PetSpriteAnimationPolicyTests
{
    [Fact]
    public void PngAtlasContainsEveryProductionFrame()
    {
        Assert.Equal(88, PetSpriteAtlasPolicy.FrameCount);
        Assert.Equal("frame-00-00.png", PetSpriteAtlasPolicy.FrameFileName(0, 0));
        Assert.Equal("frame-10-07.png", PetSpriteAtlasPolicy.FrameFileName(10, 7));

        var framesDirectory = Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "Assets",
            "Pets",
            "fengtuan",
            "frames");
        var frames = Directory.GetFiles(framesDirectory, "frame-??-??.png");

        Assert.Equal(PetSpriteAtlasPolicy.FrameCount, frames.Length);
        Assert.All(frames, path => Assert.InRange(
            new FileInfo(path).Length,
            1,
            PetSpriteAtlasPolicy.MaximumFrameBytes));
    }

    [Theory]
    [InlineData(PetAnimationState.Idle, 0, 7, 1000)]
    [InlineData(PetAnimationState.Succeeded, 4, 5, 500)]
    [InlineData(PetAnimationState.Failed, 5, 8, 500)]
    [InlineData(PetAnimationState.Waiting, 6, 6, 500)]
    [InlineData(PetAnimationState.Running, 7, 6, 200)]
    [InlineData(PetAnimationState.Review, 8, 6, 250)]
    public void ActivityStateMatchesMacOsAnimation(
        PetAnimationState state,
        int expectedRow,
        int expectedFrames,
        int expectedDurationMilliseconds)
    {
        var sequence = PetSpriteSequence.For(state, isDragging: false, isDraggingRight: true);

        Assert.Equal(expectedRow, sequence.Row);
        Assert.Equal(expectedFrames, sequence.FrameCount);
        Assert.Equal(TimeSpan.FromMilliseconds(expectedDurationMilliseconds), sequence.FrameDuration);
    }

    [Theory]
    [InlineData(true, 1)]
    [InlineData(false, 2)]
    public void DragDirectionSelectsDirectionalGait(bool isDraggingRight, int expectedRow)
    {
        var sequence = PetSpriteSequence.For(
            PetAnimationState.Idle,
            isDragging: true,
            isDraggingRight);

        Assert.Equal(expectedRow, sequence.Row);
        Assert.Equal(8, sequence.FrameCount);
        Assert.Equal(TimeSpan.FromMilliseconds(100), sequence.FrameDuration);
    }

    private static string FindWindowsRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "ChatOS.Win.sln")))
                return directory.FullName;
            directory = directory.Parent;
        }

        throw new DirectoryNotFoundException("Could not locate the ChatOS Windows repository root.");
    }
}
