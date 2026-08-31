using ChatOS.Connector.Remote;

namespace ChatOS.Connector.Tests;

public sealed class SshNetRemoteTerminalCommandServiceTests
{
    [Fact]
    public void ParseSeparatesOutputErrorDirectoryAndExitStatus()
    {
        const string marker = "__MARK__";
        var payload = """
            __MARK__OUT
            line one
            line two
            __MARK__ERR
            warning
            __MARK__CWD
            /srv/app
            __MARK__STATUS
            7
            """;

        var result = SshNetRemoteTerminalCommandService.Parse(payload, string.Empty, marker, "~");

        Assert.Equal("line one\nline two", result.Output.Replace("\r\n", "\n"));
        Assert.Equal("warning", result.Error);
        Assert.Equal(7, result.ExitCode);
        Assert.Equal("/srv/app", result.WorkingDirectory);
    }

    [Fact]
    public void ParseUsesFallbacksWhenRemoteWrapperIsInterrupted()
    {
        var result = SshNetRemoteTerminalCommandService.Parse(
            "partial output without markers",
            "connection closed",
            "__MISSING__",
            "/home/deploy");

        Assert.Empty(result.Output);
        Assert.Equal("connection closed", result.Error);
        Assert.Equal(-1, result.ExitCode);
        Assert.Equal("/home/deploy", result.WorkingDirectory);
    }
}
