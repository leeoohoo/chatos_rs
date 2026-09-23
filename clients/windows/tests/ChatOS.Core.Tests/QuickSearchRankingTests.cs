using ChatOS.Core.Domain;

namespace ChatOS.Core.Tests;

public sealed class QuickSearchRankingTests
{
    [Fact]
    public void ExactAndPrefixMatchesOutrankFuzzyMatches()
    {
        var exact = QuickSearchRanking.Score("Edge", "Edge");
        var prefix = QuickSearchRanking.Score("Ed", "Edge");
        var fuzzy = QuickSearchRanking.Score("eg", "Edge");

        Assert.NotNull(exact);
        Assert.NotNull(prefix);
        Assert.NotNull(fuzzy);
        Assert.True(exact > prefix);
        Assert.True(prefix > fuzzy);
    }

    [Fact]
    public void ChineseSubstringMatchesWithoutTokenization() =>
        Assert.True(QuickSearchRanking.Score("项目", "打开项目设置") > 200);

    [Fact]
    public void UnrelatedTextDoesNotMatch() =>
        Assert.Null(QuickSearchRanking.Score("terminal", "Edge"));

    [Fact]
    public void BoostsCannotOvertakeHighlyAccurateMatch()
    {
        var exact = QuickSearchRanking.Score("Notes", "Notes");
        var fuzzy = QuickSearchRanking.Score("nts", "Network Tools", recencyBoost: 1_000, frequencyBoost: 1_000);
        Assert.True(exact > fuzzy);
    }
}
