using System.Text;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class RemoteSftpViewModelTests
{
    [Fact]
    public async Task OpenUsesDefaultPathAndLoadsSortedServiceEntries()
    {
        var service = new SftpDouble();
        var viewModel = new RemoteSftpViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Connection());

        Assert.Equal("/srv/app", viewModel.CurrentPath);
        Assert.Equal("/srv/app", service.LastListedPath);
        Assert.Equal(2, viewModel.Entries.Count);
    }

    [Fact]
    public async Task OpeningDirectoryNavigatesWhileOpeningFilePreviewsText()
    {
        var service = new SftpDouble();
        var viewModel = new RemoteSftpViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Connection());

        await viewModel.OpenEntryCommand.ExecuteAsync(service.Directory);
        Assert.Equal("/srv/app/logs", viewModel.CurrentPath);

        await viewModel.OpenEntryCommand.ExecuteAsync(service.File);
        Assert.Equal("preview", viewModel.PreviewText);
    }

    [Fact]
    public async Task VerificationChallengeIsShownAndCodeIsUsedOnRetry()
    {
        var service = new SftpDouble { RequireVerification = true };
        var viewModel = new RemoteSftpViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Connection());
        Assert.Equal("OTP:", viewModel.VerificationPrompt);

        service.RequireVerification = false;
        viewModel.VerificationCode = "654321";
        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("654321", service.LastCode);
        Assert.Null(viewModel.VerificationPrompt);
    }

    [Fact]
    public async Task UploadResolvesFileNameInsideCurrentDirectory()
    {
        var service = new SftpDouble();
        var viewModel = new RemoteSftpViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Connection());
        await using var input = new MemoryStream(Encoding.UTF8.GetBytes("data"));

        await viewModel.UploadAsync(input, "new.txt", true);

        Assert.Equal("/srv/app/new.txt", service.LastUploadedPath);
        Assert.True(service.LastOverwrite);
    }

    private static RemoteConnection Connection() => new(
        "remote-1", "Server", "server.example", 22, "deploy", RemoteAuthenticationType.Password,
        true, false, false, "/srv/app", RemoteHostKeyPolicy.Strict, "device-1", "workspace-1",
        false, null, null, null, null, false, false, false, null);

    private sealed class SftpDouble : IRemoteSftpService
    {
        public RemoteFileEntry Directory { get; } = new("logs", "/srv/app/logs", true, false, 0, DateTimeOffset.UtcNow);
        public RemoteFileEntry File { get; } = new("readme.txt", "/srv/app/readme.txt", false, false, 7, DateTimeOffset.UtcNow);
        public bool RequireVerification { get; set; }
        public string? LastListedPath { get; private set; }
        public string? LastCode { get; private set; }
        public string? LastUploadedPath { get; private set; }
        public bool LastOverwrite { get; private set; }

        public Task<IReadOnlyList<RemoteFileEntry>> ListAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default)
        {
            LastListedPath = path; LastCode = verificationCode;
            if (RequireVerification) throw new RemoteVerificationRequiredException("OTP:");
            return Task.FromResult<IReadOnlyList<RemoteFileEntry>>([Directory, File]);
        }
        public Task<string> ReadTextAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default) => Task.FromResult("preview");
        public Task DownloadAsync(string connectionId, string remotePath, Stream destination, string? verificationCode = null, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task UploadAsync(string connectionId, Stream source, string remotePath, bool overwrite, string? verificationCode = null, CancellationToken cancellationToken = default) { LastUploadedPath = remotePath; LastOverwrite = overwrite; return Task.CompletedTask; }
        public Task CreateDirectoryAsync(string connectionId, string path, string? verificationCode = null, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task RenameAsync(string connectionId, string sourcePath, string destinationPath, string? verificationCode = null, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task DeleteAsync(string connectionId, string path, bool recursive, string? verificationCode = null, CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
