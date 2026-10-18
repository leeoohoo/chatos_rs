using ChatOS.Api.WeChat;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class WeChatCompanionSettingsViewModelTests
{
    [Fact]
    public async Task Loads_issues_revokes_and_unbinds_using_real_view_state()
    {
        var service = new FakeService();
        using var viewModel = Create(service);

        await viewModel.OpenAsync();

        Assert.False(viewModel.IsBound);
        Assert.True(viewModel.HasSessions);
        Assert.True(viewModel.ShowUnbound);

        await viewModel.IssueTicketAsync();

        Assert.True(viewModel.HasTicket);
        Assert.Equal([1, 2, 3], viewModel.TicketImageData);
        Assert.False(viewModel.ShowUnbound);

        await viewModel.RevokeAsync(Assert.Single(viewModel.Sessions));

        Assert.True(Assert.Single(viewModel.Sessions).IsRevoked);
        Assert.Contains("撤销", viewModel.Notice, StringComparison.Ordinal);

        service.Bound = true;
        await viewModel.OpenAsync();
        Assert.True(viewModel.IsBound);

        await viewModel.UnbindAsync();

        Assert.False(viewModel.IsBound);
        Assert.Null(viewModel.Ticket);
        Assert.True(service.Unbound);
    }

    private static WeChatCompanionSettingsViewModel Create(IWeChatCompanionService service)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var localization = new LocalizationViewModel(
            new AppPreferencesManager(new MemoryPreferencesStore()),
            dispatcher);
        return new WeChatCompanionSettingsViewModel(service, localization, dispatcher);
    }

    private sealed class FakeService : IWeChatCompanionService
    {
        public bool Bound { get; set; }
        public bool Revoked { get; private set; }
        public bool Unbound { get; private set; }

        public Task<WeChatBindingStatus> GetBindingStatusAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new WeChatBindingStatus(Bound, null, null));

        public Task<WeChatBindTicket> IssueBindTicketAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new WeChatBindTicket(
                "ticket",
                "scene",
                DateTimeOffset.UtcNow.AddMinutes(2).ToUnixTimeSeconds(),
                [1, 2, 3]));

        public Task<WeChatBindTicketStatus> GetBindTicketStatusAsync(
            string id,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new WeChatBindTicketStatus(id, "issued", 2_000_000_000, null));

        public Task ConfirmBindTicketAsync(
            string id,
            CancellationToken cancellationToken = default)
        {
            Bound = true;
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<WeChatClientSession>> ListClientSessionsAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<WeChatClientSession>>([
                new WeChatClientSession(
                    "session",
                    "wechat_mini_program",
                    "2026-01-01T00:00:00Z",
                    "2026-01-02T00:00:00Z",
                    2_000_000_000,
                    Revoked ? "2026-01-03T00:00:00Z" : null),
            ]);

        public Task RevokeClientSessionAsync(
            string id,
            CancellationToken cancellationToken = default)
        {
            Revoked = true;
            return Task.CompletedTask;
        }

        public Task UnbindAsync(CancellationToken cancellationToken = default)
        {
            Bound = false;
            Unbound = true;
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(
            AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
