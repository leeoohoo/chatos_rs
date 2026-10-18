using System.Text.Json;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentProductSkillBundleTests
{
    [Fact]
    public void MainChatBundleUsesCanonicalPolicyCatalogAndBindings()
    {
        var snapshot = WindowsLocalAgentProductSkillBundle.BuildMainChat();
        using var document = JsonDocument.Parse(snapshot.Json);
        var root = document.RootElement;

        Assert.Equal(1, root.GetProperty("schema_version").GetInt32());
        Assert.Equal(snapshot.Revision, root.GetProperty("bundle_revision").GetString());
        Assert.StartsWith("sha256:", snapshot.Revision, StringComparison.Ordinal);
        Assert.Contains(
            "treat that as an execution request rather than a request for another plan summary",
            snapshot.PolicyInstructions,
            StringComparison.Ordinal);
        Assert.Contains("Never end with a future-tense promise", snapshot.PolicyInstructions,
            StringComparison.Ordinal);

        var documents = root.GetProperty("documents").EnumerateArray().ToArray();
        Assert.Equal(
            [
                "product-skill:chatos-async-task-orchestration",
                "product-skill:chatos-main-chat-invariants",
                "product-skill:chatos-requirement-discovery",
                "product-skill:chatos-skill-runtime",
            ],
            documents.Select(value => value.GetProperty("skill_ref").GetString()!)
                .Order(StringComparer.Ordinal).ToArray());
        Assert.All(documents, value =>
            Assert.StartsWith("sha256:", value.GetProperty("instructions_sha256").GetString(),
                StringComparison.Ordinal));
        var orchestration = documents.Single(value => value.GetProperty("name").GetString() ==
            "chatos-async-task-orchestration");
        Assert.Equal(10, orchestration.GetProperty("resources").GetArrayLength());
        var requirement = documents.Single(value => value.GetProperty("name").GetString() ==
            "chatos-requirement-discovery");
        Assert.Equal(2, requirement.GetProperty("resources").GetArrayLength());

        var bindings = root.GetProperty("tool_skill_bindings").EnumerateArray().ToArray();
        Assert.Equal(3, bindings.Length);
        var task = bindings.Single(value => value.GetProperty("binding_id").GetString() ==
            "main-chat.task-orchestration");
        Assert.Equal("on_demand", task.GetProperty("activation_policy").GetString());
        Assert.True(task.GetProperty("tool_names").EnumerateArray()
            .Select(value => value.GetString()!)
            .ToHashSet(StringComparer.Ordinal)
            .SetEquals(WindowsLocalAgentCapabilityCatalog.MainChatTaskToolNames));
        var survey = bindings.Single(value => value.GetProperty("binding_id").GetString() ==
            "project.requirement-discovery");
        Assert.Equal(
            ["requirement_plan_save", "requirement_survey_create"],
            survey.GetProperty("tool_names").EnumerateArray()
                .Select(value => value.GetString()!).ToArray());

        var catalogText = snapshot.CatalogInputItem.GetProperty("content")[0]
            .GetProperty("text").GetString();
        Assert.Contains("[ChatOS Product Skill Catalog]", catalogText, StringComparison.Ordinal);
        Assert.Contains("product-skill:chatos-async-task-orchestration", catalogText,
            StringComparison.Ordinal);
        Assert.Contains("product-skill:chatos-requirement-discovery", catalogText,
            StringComparison.Ordinal);
        Assert.DoesNotContain("references/software-engineering.md", catalogText,
            StringComparison.Ordinal);
    }

    [Fact]
    public void MainChatCatalogIncludesMacOSCompanionToolsWithoutDuplicates()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatTools;
        var names = tools.Select(value => value.GetProperty("name").GetString()!).ToArray();

        Assert.Equal(names.Length, names.Distinct(StringComparer.Ordinal).Count());
        Assert.True(WindowsLocalAgentCapabilityCatalog.MainChatTaskToolNames
            .ToHashSet(StringComparer.Ordinal).IsSubsetOf(names));
        Assert.Contains("requirement_survey_create", names);
        Assert.Contains("requirement_plan_save", names);
        Assert.Contains("local_attachment_read", names);

        var attachment = tools.Single(value =>
            value.GetProperty("name").GetString() == "local_attachment_read");
        Assert.Equal(
            ["model", "content"],
            attachment.GetProperty("parameters").GetProperty("properties")
                .GetProperty("delivery").GetProperty("enum").EnumerateArray()
                .Select(value => value.GetString()!).ToArray());
        var survey = tools.Single(value =>
            value.GetProperty("name").GetString() == "requirement_survey_create");
        Assert.Equal(
            ["question_id", "prompt", "response_kind"],
            survey.GetProperty("parameters").GetProperty("properties")
                .GetProperty("questions").GetProperty("items").GetProperty("required")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
    }

    [Fact]
    public void MainChatBundleIsDeterministic()
    {
        var first = WindowsLocalAgentProductSkillBundle.BuildMainChat();
        var second = WindowsLocalAgentProductSkillBundle.BuildMainChat();

        Assert.Equal(first.Revision, second.Revision);
        Assert.Equal(first.Json, second.Json);
    }
}
