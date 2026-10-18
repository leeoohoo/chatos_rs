using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Core.Tests;

public sealed class MessageTaskRequirementPlanTests
{
    [Fact]
    public void DecodesTaskOwnConfirmedSnapshotHashAndScope()
    {
        const string markdown = "# Original proposal\n\nFrozen body.";
        var hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(markdown)))
            .ToLowerInvariant();
        var input = JsonSerializer.SerializeToElement(new
        {
            requirement_plan = new
            {
                snapshot = new
                {
                    plan_id = "plan-1",
                    revision = 3UL,
                    title = "Original proposal",
                    markdown,
                    confirmed_at_unix_ms = 1234L,
                },
                markdown_sha256 = hash,
                task_scope = new
                {
                    title = "Implement Windows parity",
                    prompt = "Use the frozen proposal",
                    expected_outputs = new[] { "Implementation", "Evidence" },
                    acceptance_criteria = new[] { "History remains on revision 3" },
                },
            },
        });

        var plan = Assert.IsType<MessageTaskRequirementPlan>(
            MessageTaskRequirementPlan.Decode(input, "Implement Windows parity"));

        Assert.Equal(3UL, plan.Snapshot.Revision);
        Assert.Equal(markdown, plan.Snapshot.Markdown);
        Assert.Equal(hash, plan.MarkdownSha256);
        Assert.Contains("Implementation", plan.TaskScope.ExpectedOutputs);
        Assert.Contains("revision 3", plan.TaskScope.AcceptanceCriteria);
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("unconfirmed")]
    [InlineData("zero-revision")]
    [InlineData("wrong-hash")]
    [InlineData("wrong-scope")]
    public void RejectsMalformedOrNonAuthoritativeSnapshots(string variant)
    {
        const string markdown = "Frozen body";
        var hash = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(markdown)))
            .ToLowerInvariant();
        JsonElement input = variant switch
        {
            "missing" => JsonSerializer.SerializeToElement(new { }),
            "unconfirmed" => Input(1, 0, hash, "Task"),
            "zero-revision" => Input(0, 10, hash, "Task"),
            "wrong-hash" => Input(1, 10, new string('a', 64), "Task"),
            _ => Input(1, 10, hash, "Another task"),
        };

        Assert.Null(MessageTaskRequirementPlan.Decode(input, "Task"));

        static JsonElement Input(ulong revision, long confirmedAt, string hash, string scopeTitle) =>
            JsonSerializer.SerializeToElement(new
            {
                requirement_plan = new
                {
                    snapshot = new
                    {
                        plan_id = "plan-1",
                        revision,
                        title = "Plan",
                        markdown,
                        confirmed_at_unix_ms = confirmedAt,
                    },
                    markdown_sha256 = hash,
                    task_scope = new { title = scopeTitle, prompt = "Do it" },
                },
            });
    }
}
