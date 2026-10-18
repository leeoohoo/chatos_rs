using ChatOS.Presentation.Terminal;

namespace ChatOS.Presentation.Tests;

public sealed class TerminalPlainTextDecoderTests
{
    [Fact]
    public void RemovesPowerShellStartupControlSequences()
    {
        var decoder = new TerminalPlainTextDecoder();
        const string output = "\u001b[?9001h\u001b[?1004h\u001b[?25l\u001b[2J\u001b[m\u001b[H \u001b[H" +
                              "\u001b]0;C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe \a" +
                              "\u001b[?25hPS C:\\>";

        Assert.Equal(" PS C:\\>", decoder.Decode(output));
    }

    [Fact]
    public void HandlesControlSequencesSplitAcrossOutputChunks()
    {
        var decoder = new TerminalPlainTextDecoder();

        Assert.Equal(string.Empty, decoder.Decode("\u001b[?25"));
        Assert.Equal("Hello", decoder.Decode("lHello\u001b]0;Title\u001b"));
        Assert.Equal("World", decoder.Decode("\\World"));
    }

    [Fact]
    public void KeepsReadableWhitespaceAndDropsOtherControlCharacters()
    {
        var decoder = new TerminalPlainTextDecoder();

        Assert.Equal("first\r\n\tsecond", decoder.Decode("first\r\n\tsec\0\b\u007fond"));
    }
}
