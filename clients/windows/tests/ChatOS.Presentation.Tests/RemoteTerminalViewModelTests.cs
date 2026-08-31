using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class RemoteTerminalViewModelTests
{
    [Fact]
    public async Task SubmitAppendsCommandOutputAndUpdatesWorkingDirectory()
    {
        var service = new TerminalDouble();
        using var viewModel = new RemoteTerminalViewModel(service, new ImmediateUiDispatcher());
        viewModel.Open(Connection());
        viewModel.Command = "pwd";

        await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Contains(viewModel.Lines, line => line.IsCommand && line.Text.Contains("pwd"));
        Assert.Contains(viewModel.Lines, line => line.Text == "/srv/app");
        Assert.Equal("/srv/app", viewModel.WorkingDirectory);
        Assert.False(viewModel.IsRunning);
    }

    [Fact]
    public async Task VerificationChallengeRetriesOriginalCommandWithCode()
    {
        var service = new TerminalDouble { RequireVerification = true };
        using var viewModel = new RemoteTerminalViewModel(service, new ImmediateUiDispatcher());
        viewModel.Open(Connection());
        viewModel.Command = "whoami";

        await viewModel.SubmitCommand.ExecuteAsync(null);
        Assert.Equal("OTP:", viewModel.VerificationPrompt);

        service.RequireVerification = false;
        viewModel.VerificationCode = "123456";
        await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Equal("whoami", service.LastCommand);
        Assert.Equal("123456", service.LastCode);
        Assert.Null(viewModel.VerificationPrompt);
    }

    [Fact]
    public async Task CancelStopsActiveCommandAndRestoresSubmitState()
    {
        var service = new TerminalDouble { WaitForCancellation = true };
        using var viewModel = new RemoteTerminalViewModel(service, new ImmediateUiDispatcher());
        viewModel.Open(Connection());
        viewModel.Command = "tail -f log";

        var execution = viewModel.SubmitCommand.ExecuteAsync(null);
        await service.Started.Task.WaitAsync(TimeSpan.FromSeconds(1));
        viewModel.CancelCommand.Execute(null);
        await execution;

        Assert.False(viewModel.IsRunning);
        Assert.Contains(viewModel.Lines, line => line.Text == "命令已取消。");
    }

    private static RemoteConnection Connection() => new(
        "remote-1", "Server", "server.example", 22, "deploy", RemoteAuthenticationType.Password,
        true, false, false, "/srv", RemoteHostKeyPolicy.Strict, "device-1", "workspace-1",
        false, null, null, null, null, false, false, false, null);

    private sealed class TerminalDouble : IRemoteTerminalCommandService
    {
        public bool RequireVerification { get; set; }
        public bool WaitForCancellation { get; set; }
        public string? LastCommand { get; private set; }
        public string? LastCode { get; private set; }
        public TaskCompletionSource Started { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task<RemoteTerminalCommandResult> ExecuteAsync(string connectionId, string command, string workingDirectory, string? verificationCode = null, CancellationToken cancellationToken = default)
        {
            LastCommand = command; LastCode = verificationCode;
            if (RequireVerification) throw new RemoteVerificationRequiredException("OTP:");
            if (WaitForCancellation) { Started.TrySetResult(); await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken); }
            return new RemoteTerminalCommandResult("/srv/app", string.Empty, 0, "/srv/app");
        }
    }
}
