using ChatOS.Presentation.Chat;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationComposerTextPasteTests
{
    [Fact]
    public void ShortPasteReplacesSelectionAndPlacesCaretAfterInsertedText()
    {
        var result = ConversationComposerTextPaste.Insert("before OLD after", 7, 3, "新文本");

        Assert.Equal("before 新文本 after", result.Text);
        Assert.Equal(10, result.CaretIndex);
        Assert.False(ConversationComposerTextPaste.ShouldCreateAttachment("新文本"));
    }

    [Fact]
    public void CharacterThresholdCreatesAttachment()
    {
        var text = new string('x', ConversationComposerTextPaste.AttachmentCharacterThreshold);

        Assert.True(ConversationComposerTextPaste.ShouldCreateAttachment(text));
    }

    [Fact]
    public void Utf8ByteThresholdCreatesAttachmentBeforeCharacterThreshold()
    {
        var text = new string('界', 2_667);

        Assert.True(text.Length < ConversationComposerTextPaste.AttachmentCharacterThreshold);
        Assert.True(ConversationComposerTextPaste.ShouldCreateAttachment(text));
    }
}
