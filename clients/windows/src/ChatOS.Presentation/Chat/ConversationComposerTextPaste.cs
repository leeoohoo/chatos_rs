using System.Text;

namespace ChatOS.Presentation.Chat;

public readonly record struct ConversationComposerTextEdit(string Text, int CaretIndex);

public static class ConversationComposerTextPaste
{
    public const int AttachmentCharacterThreshold = 4_000;
    public const int AttachmentByteThreshold = 8_000;

    public static bool ShouldCreateAttachment(string text) =>
        text.Length >= AttachmentCharacterThreshold ||
        Encoding.UTF8.GetByteCount(text) >= AttachmentByteThreshold;

    public static ConversationComposerTextEdit Insert(
        string? currentText,
        int selectionStart,
        int selectionLength,
        string pastedText)
    {
        currentText ??= string.Empty;
        var start = Math.Clamp(selectionStart, 0, currentText.Length);
        var length = Math.Clamp(selectionLength, 0, currentText.Length - start);
        var updated = string.Concat(
            currentText.AsSpan(0, start),
            pastedText,
            currentText.AsSpan(start + length));
        return new ConversationComposerTextEdit(updated, start + pastedText.Length);
    }
}
