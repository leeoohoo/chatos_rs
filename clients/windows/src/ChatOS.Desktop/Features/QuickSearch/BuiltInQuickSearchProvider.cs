using ChatOS.Core.Domain;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class BuiltInQuickSearchProvider : IQuickSearchProvider
{
    private static readonly (string Id, string Zh, string En, string Glyph, QuickSearchBuiltInAction Action)[] Actions =
    [
        ("screenshot", "截图", "Screenshot", "\uE91B", QuickSearchBuiltInAction.Screenshot),
        ("screen-recording", "屏幕录制", "Screen recording", "\uE714", QuickSearchBuiltInAction.ScreenRecording),
        ("clipboard", "剪贴板历史", "Clipboard history", "\uE8C8", QuickSearchBuiltInAction.ClipboardHistory),
        ("settings", "设置", "Settings", "\uE713", QuickSearchBuiltInAction.OpenSettings),
        ("permissions", "本机运行权限", "Runtime permissions", "\uE72E", QuickSearchBuiltInAction.OpenRuntimePermissions),
    ];

    public QuickSearchResultKind Kind => QuickSearchResultKind.Action;

    public Task<IReadOnlyList<QuickSearchResult>> SearchAsync(
        string query,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var results = Actions.Select(item =>
        {
            var score = QuickSearchRanking.Score(query, item.Zh, item.En, providerWeight: 35);
            return score is null ? null : new QuickSearchResult(
                $"action:{item.Id}", Kind, item.Zh, item.En, item.Glyph, score.Value,
                new(QuickSearchActionKind.BuiltIn, item.Id, item.Action));
        }).OfType<QuickSearchResult>().ToArray();
        return Task.FromResult<IReadOnlyList<QuickSearchResult>>(results);
    }
}
