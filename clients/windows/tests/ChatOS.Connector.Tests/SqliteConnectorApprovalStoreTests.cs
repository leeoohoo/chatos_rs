using ChatOS.Connector.Approval;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Tests;

public sealed class SqliteConnectorApprovalStoreTests
{
    [Fact]
    public async Task ModeAndHistoryRoundTrip()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-approval-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorApprovalStore(database);
            await store.SaveModeAsync(ConnectorApprovalMode.AutoApproval);
            var createdAt = DateTimeOffset.UtcNow;
            await store.AppendAsync(new ConnectorApprovalHistoryEntry(
                "history-1",
                "approval-1",
                "request-1",
                "workspace-1",
                "git status",
                directory,
                "test",
                ConnectorApprovalMode.AutoApproval,
                true,
                ConnectorApprovalReviewer.User,
                ConnectorApprovalRiskLevel.Low,
                null,
                "approved",
                createdAt));

            Assert.Equal(ConnectorApprovalMode.AutoApproval, await store.ReadModeAsync());
            var entry = Assert.Single(await store.ReadHistoryAsync());
            Assert.Equal("approval-1", entry.ApprovalId);
            Assert.True(entry.Approved);
            Assert.Equal(ConnectorApprovalReviewer.User, entry.Reviewer);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task CommandAuditRoundTripsWithoutLosingTruncationMetadata()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-command-audit-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteTerminalCommandHistoryStore(database);
            await store.AppendAsync(new TerminalCommandHistoryEntry(
                "history-1",
                "request-1",
                "workspace-1",
                "test",
                "cmd.exe /c echo hello",
                directory,
                true,
                0,
                false,
                30_000,
                "hello",
                string.Empty,
                700_000,
                0,
                true,
                false,
                "approved",
                "approved by user",
                null,
                DateTimeOffset.UtcNow));

            var entry = Assert.Single(await store.ReadAsync());
            Assert.Equal(700_000, entry.StandardOutputBytes);
            Assert.True(entry.StandardOutputTruncated);
            Assert.Equal("approved", entry.ApprovalDecision);
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }
}
