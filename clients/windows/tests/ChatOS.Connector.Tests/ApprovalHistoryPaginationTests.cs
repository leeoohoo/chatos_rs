using ChatOS.Connector.Approval;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed partial class ApprovalSettingsViewModelTests
{
    [Fact]
    public async Task HistoryPagesReachRecordsBeyondPreviousThirtyEntryLimitWithoutReadingAgain()
    {
        var store = HistoryStore(53);
        using var viewModel = Create(store, out _);
        await viewModel.OpenAsync();

        Assert.Equal(53, viewModel.HistoryCount);
        Assert.Equal(6, viewModel.HistoryPageCount);
        Assert.Equal(10, viewModel.History.Count);
        Assert.Equal("1–10 / 53", viewModel.HistoryRangeLabel);
        Assert.False(viewModel.PreviousHistoryPageCommand.CanExecute(null));
        var ids = viewModel.History.Select(row => row.Entry.Id).ToList();
        while (viewModel.NextHistoryPageCommand.CanExecute(null))
        {
            viewModel.NextHistoryPageCommand.Execute(null);
            ids.AddRange(viewModel.History.Select(row => row.Entry.Id));
        }

        Assert.Equal(store.History.Select(entry => entry.Id), ids);
        Assert.Equal(3, viewModel.History.Count);
        Assert.Equal("51–53 / 53", viewModel.HistoryRangeLabel);
        Assert.Equal(1, store.HistoryReadCount);
        Assert.True(viewModel.PreviousHistoryPageCommand.CanExecute(null));
        viewModel.PreviousHistoryPageCommand.Execute(null);
        Assert.Equal("41–50 / 53", viewModel.HistoryRangeLabel);
    }

    [Theory]
    [InlineData(10, 10, 6)]
    [InlineData(20, 20, 3)]
    [InlineData(50, 50, 2)]
    public async Task ChangingPageSizeResetsToFirstPage(int size, int rows, int pages)
    {
        using var viewModel = Create(HistoryStore(53), out _);
        await viewModel.OpenAsync();
        viewModel.HistoryPageSize = size == 10 ? 20 : 10;
        viewModel.NextHistoryPageCommand.Execute(null);

        viewModel.HistoryPageSize = size;

        Assert.Equal(0, viewModel.HistoryPageIndex);
        Assert.Equal(rows, viewModel.History.Count);
        Assert.Equal(pages, viewModel.HistoryPageCount);
        Assert.True(viewModel.HasHistoryPagination);
        Assert.False(viewModel.PreviousHistoryPageCommand.CanExecute(null));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    [InlineData(1)]
    [InlineData(30)]
    [InlineData(int.MaxValue)]
    public async Task UnsupportedPageSizePreservesCurrentPage(int size)
    {
        using var viewModel = Create(HistoryStore(53), out _);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);

        viewModel.HistoryPageSize = size;

        Assert.Equal(10, viewModel.HistoryPageSize);
        Assert.Equal(1, viewModel.HistoryPageIndex);
        Assert.Equal("11–20 / 53", viewModel.HistoryRangeLabel);
    }

    [Theory]
    [InlineData(0, 0, 0)]
    [InlineData(5, 0, 5)]
    [InlineData(13, 1, 3)]
    [InlineData(50, 4, 10)]
    [InlineData(62, 5, 10)]
    public async Task RefreshClampsPageWhenHistoryShrinksAndPreservesItWhenPossible(
        int remaining, int page, int rows)
    {
        var store = HistoryStore(62);
        using var viewModel = Create(store, out _);
        await viewModel.OpenAsync();
        for (var index = 0; index < 5; index++) viewModel.NextHistoryPageCommand.Execute(null);
        store.History.RemoveRange(remaining, store.History.Count - remaining);

        await viewModel.OpenAsync();

        Assert.Equal(page, viewModel.HistoryPageIndex);
        Assert.Equal(rows, viewModel.History.Count);
        Assert.Equal(remaining, viewModel.HistoryCount);
        Assert.Equal(remaining > 0, viewModel.HasHistory);
        Assert.Equal(remaining > 10, viewModel.HasHistoryPagination);
        Assert.Equal(page > 0, viewModel.PreviousHistoryPageCommand.CanExecute(null));
        Assert.Equal(page + 1 < viewModel.HistoryPageCount, viewModel.NextHistoryPageCommand.CanExecute(null));
    }

    [Fact]
    public async Task EmptyHistoryHasNoNavigablePages()
    {
        using var viewModel = Create(HistoryStore(0), out _);
        await viewModel.OpenAsync();

        viewModel.NextHistoryPageCommand.Execute(null);
        viewModel.PreviousHistoryPageCommand.Execute(null);

        Assert.Equal(0, viewModel.HistoryPageIndex);
        Assert.Equal(1, viewModel.HistoryPageCount);
        Assert.Empty(viewModel.History);
        Assert.False(viewModel.HasHistoryPagination);
        Assert.False(viewModel.NextHistoryPageCommand.CanExecute(null));
        Assert.False(viewModel.PreviousHistoryPageCommand.CanExecute(null));
    }

    [Fact]
    public async Task FailedRefreshKeepsEveryCachedPageAndCanBeRetried()
    {
        var store = HistoryStore(53);
        using var viewModel = Create(store, out _);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        store.HistoryReadError = new IOException("history unavailable");

        await viewModel.OpenAsync();

        Assert.Equal("history unavailable", viewModel.ErrorMessage);
        Assert.Equal("11–20 / 53", viewModel.HistoryRangeLabel);
        viewModel.HistoryPageSize = 50;
        viewModel.NextHistoryPageCommand.Execute(null);
        Assert.Equal(3, viewModel.History.Count);
        Assert.Equal("history-50", viewModel.History[0].Entry.Id);

        store.HistoryReadError = null;
        store.History.Add(History("history-53", true));
        await viewModel.OpenAsync();
        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("51–54 / 54", viewModel.HistoryRangeLabel);
    }

    [Fact]
    public async Task LanguageChangeRelabelsCurrentPageWithoutReadingOrDroppingCachedHistory()
    {
        var store = HistoryStore(53);
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        var localization = new LocalizationViewModel(preferences, dispatcher);
        var coordinator = new CommandApprovalCoordinator(store);
        using var viewModel = new ApprovalSettingsViewModel(coordinator, localization, dispatcher);
        await viewModel.OpenAsync();
        viewModel.NextHistoryPageCommand.Execute(null);
        store.HistoryReadError = new IOException("must not reread history");

        await preferences.UpdateAsync(value => value with { Language = InterfaceLanguage.English });
        await WaitUntilAsync(() => viewModel.History[0].DecisionLabel == "Allowed");

        Assert.Equal("Page 2 of 6", viewModel.HistoryPageLabel);
        Assert.Equal("Previous page", viewModel.PreviousHistoryPageLabel);
        Assert.Equal(1, viewModel.HistoryPageIndex);
        Assert.Equal(53, viewModel.HistoryCount);
        Assert.Equal(1, store.HistoryReadCount);
        viewModel.HistoryPageSize = 50;
        viewModel.NextHistoryPageCommand.Execute(null);
        Assert.Equal("history-50", viewModel.History[0].Entry.Id);
    }

    [Fact]
    public async Task ResolvingApprovalRefreshesWholeHistoryAndRetainsCurrentPage()
    {
        var store = HistoryStore(53);
        using var viewModel = Create(store, out var coordinator);
        await viewModel.OpenAsync();
        for (var index = 0; index < 4; index++) viewModel.NextHistoryPageCommand.Execute(null);
        var request = coordinator.RequestAsync(Request(), new ConnectorApprovalRisk(ConnectorApprovalRiskLevel.High));
        await WaitUntilAsync(() => viewModel.Pending.Count == 1);

        await viewModel.ResolveAsync(viewModel.Pending[0], ConnectorApprovalAction.Decline);

        Assert.False((await request).Approved);
        Assert.Equal(54, viewModel.HistoryCount);
        Assert.Equal("41–50 / 54", viewModel.HistoryRangeLabel);
        viewModel.NextHistoryPageCommand.Execute(null);
        Assert.Equal(4, viewModel.History.Count);
        Assert.Empty(viewModel.Pending);
        Assert.NotNull(viewModel.ActionMessage);
    }

    [Fact]
    public async Task HistoryUsesStoreLimitAndPaginationRemainsAvailableAtLargePageSizes()
    {
        using var viewModel = Create(HistoryStore(1_005), out _);
        await viewModel.OpenAsync();
        viewModel.HistoryPageSize = 50;

        Assert.Equal(1_000, viewModel.HistoryCount);
        Assert.Equal(20, viewModel.HistoryPageCount);
        Assert.True(viewModel.HasHistoryPagination);
        for (var index = 0; index < 19; index++) viewModel.NextHistoryPageCommand.Execute(null);
        Assert.Equal("history-999", viewModel.History[^1].Entry.Id);
        Assert.False(viewModel.NextHistoryPageCommand.CanExecute(null));
    }

    [Theory]
    [InlineData(11)]
    [InlineData(31)]
    public async Task PageSizeSelectorStaysAvailableWhenAllRowsFitOnLargerPage(int count)
    {
        using var viewModel = Create(HistoryStore(count), out _);
        await viewModel.OpenAsync();
        viewModel.HistoryPageSize = 50;

        Assert.True(viewModel.HasHistoryPagination);
        Assert.Equal(count, viewModel.History.Count);
        Assert.False(viewModel.NextHistoryPageCommand.CanExecute(null));

        viewModel.HistoryPageSize = 10;
        Assert.True(viewModel.NextHistoryPageCommand.CanExecute(null));
    }

    private static MemoryApprovalStore HistoryStore(int count)
    {
        var store = new MemoryApprovalStore();
        for (var index = 0; index < count; index++) store.History.Add(History($"history-{index}", true));
        return store;
    }
}
