using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Core.Tests;

public sealed class MessageTaskAcceptanceTests
{
    [Fact]
    public void ValidReportBindsEveryCriterionAndCountsUnicodeScalars()
    {
        var contract = MessageTaskAcceptanceContract.Decode(JsonSerializer.SerializeToElement(new
        {
            acceptance_report_version = 1,
            acceptance_criteria = new[] { "Inspect the artifact", "Run the focused test" },
            final_response_max_chars = 4,
        }));
        var outcome = JsonSerializer.SerializeToElement(new
        {
            task_acceptance = new
            {
                status = "succeeded",
                reason = "Both checks passed",
                final_response = "中😀\n!",
                final_response_chars = 999,
                acceptance_checks = new object[]
                {
                    new { criterion_index = 1, status = "passed", evidence = "Focused test passed" },
                    new { criterion_index = 0, status = "passed", evidence = "Artifact opened and inspected" },
                },
            },
        });

        var report = MessageTaskAcceptanceReport.Decode(outcome, contract);

        Assert.NotNull(report);
        Assert.Equal(4, report.FinalResponseChars);
        Assert.Equal(MessageTaskResultDisposition.AcceptancePassed, report.Disposition);
        Assert.Collection(report.Checks,
            check =>
            {
                Assert.Equal(0, check.CriterionIndex);
                Assert.Equal("Inspect the artifact", check.Criterion);
            },
            check =>
            {
                Assert.Equal(1, check.CriterionIndex);
                Assert.Equal("Run the focused test", check.Criterion);
            });
    }

    [Theory]
    [InlineData("not_verified")]
    [InlineData("failed")]
    public void SucceededReportCannotContainAnUnpassedCheck(string checkStatus)
    {
        var contract = Contract();
        var outcome = Outcome("succeeded", checkStatus, "ok");

        Assert.Null(MessageTaskAcceptanceReport.Decode(outcome, contract));
    }

    [Fact]
    public void FailedNotVerifiedReportIsEvidenceIncompleteNotPassed()
    {
        var report = MessageTaskAcceptanceReport.Decode(
            Outcome("failed", "not_verified", "ok"), Contract());

        Assert.NotNull(report);
        Assert.Equal(MessageTaskResultDisposition.EvidenceIncomplete, report.Disposition);
        var run = new MessageTaskRun(
            "run-1", "task-1", "succeeded", null, null, null, null, null, null, report);
        Assert.Equal(MessageTaskResultDisposition.EvidenceIncomplete, run.ResultDisposition);
    }

    [Fact]
    public void OverLimitFinalResponseAndOversizedEvidenceAreRejected()
    {
        var contract = Contract(maximum: 2);
        Assert.Null(MessageTaskAcceptanceReport.Decode(
            Outcome("failed", "not_verified", "abc"), contract));
        Assert.Null(MessageTaskAcceptanceReport.Decode(
            Outcome("failed", "not_verified", "ok", new string('e', 2_001)), contract));
    }

    [Fact]
    public void LegacyOrMalformedContractDoesNotInventAcceptance()
    {
        var legacy = JsonSerializer.SerializeToElement(new
        {
            acceptance_criteria = new[] { "Inspect the artifact" },
        });
        var invalid = JsonSerializer.SerializeToElement(new
        {
            acceptance_report_version = 1,
            acceptance_criteria = Array.Empty<string>(),
        });

        Assert.Null(MessageTaskAcceptanceContract.Decode(legacy));
        Assert.Null(MessageTaskAcceptanceContract.Decode(invalid));
        Assert.Null(MessageTaskAcceptanceReport.Decode(
            Outcome("succeeded", "passed", "ok"), null));
    }

    private static MessageTaskAcceptanceContract Contract(int? maximum = null) =>
        new(1, ["Inspect the artifact"], maximum);

    private static JsonElement Outcome(
        string status,
        string checkStatus,
        string finalResponse,
        string evidence = "Concrete check result") => JsonSerializer.SerializeToElement(new
        {
            task_acceptance = new
            {
                status,
                reason = "reported",
                final_response = finalResponse,
                acceptance_checks = new[]
                {
                    new { criterion_index = 0, status = checkStatus, evidence },
                },
            },
        });
}
