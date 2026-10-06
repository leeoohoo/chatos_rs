using ChatOS.Core.Domain;
using ChatOS.Desktop.Features.Pet;

namespace ChatOS.Connector.Tests;

public sealed class PetSpriteAnimationPolicyTests
{
    [Fact]
    public void AtlasPolicyAcceptsOnlyProductionDimensions()
    {
        Assert.True(PetSpriteAtlasPolicy.HasExpectedDimensions(1536, 2288));
        Assert.False(PetSpriteAtlasPolicy.HasExpectedDimensions(3072, 4576));
        Assert.Equal(88, PetSpriteAtlasPolicy.FrameCount);
        Assert.Equal(20L * 1024 * 1024, PetSpriteAtlasPolicy.MaximumBytes);
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
}
