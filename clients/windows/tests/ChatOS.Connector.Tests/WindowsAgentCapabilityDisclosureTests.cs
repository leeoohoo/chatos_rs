using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsAgentCapabilityDisclosureTests
{
    [Fact]
    public void CatalogPagesAtToolBoundariesAndKeepsStableOptions()
    {
        var descriptor = Descriptor(Enumerable.Range(1, 24).Select(index => Tool(
            $"tool_{index}", $"browser_{index:00}", $"Browser tool {index}")).ToArray());

        var first = WindowsAgentCapabilityDisclosure.Response(descriptor, limit: 8);
        var filtered = WindowsAgentCapabilityDisclosure.Response(
            descriptor, query: "browser_17", limit: 8);
        var second = WindowsAgentCapabilityDisclosure.Response(
            descriptor, offset: first.GetProperty("next_offset").GetInt32(), limit: 8);

        Assert.Equal(2, first.GetProperty("disclosure_version").GetInt32());
        Assert.Equal("catalog", first.GetProperty("view").GetString());
        Assert.Equal(8, first.GetProperty("tools").GetArrayLength());
        Assert.DoesNotContain("input_schema", first.GetRawText(), StringComparison.Ordinal);
        Assert.Equal("tool_17",
            filtered.GetProperty("tools").EnumerateArray().First()
                .GetProperty("tool_option").GetString());
        Assert.Equal("tool_9",
            second.GetProperty("tools").EnumerateArray().First()
                .GetProperty("tool_option").GetString());
        Assert.True(first.GetRawText().EnumerateRunes().Count() <=
            WindowsAgentCapabilityDisclosure.MaximumResponseCharacters);
    }

    [Fact]
    public void ToolDetailDisclosesOnlyItsExactRunScopedSkillRefs()
    {
        var descriptor = Descriptor([
            Tool("tool_1", "read", "Read", "navigation"),
            Tool("tool_2", "write", "Write", "editing"),
        ]);

        var response = WindowsAgentCapabilityDisclosure.Response(
            descriptor, toolOption: "tool_2");

        Assert.Equal("tool", response.GetProperty("view").GetString());
        Assert.Single(response.GetProperty("tools").EnumerateArray());
        Assert.Equal("plugin-skill:plugin_1:editing",
            response.GetProperty("tools").EnumerateArray().First()
                .GetProperty("required_skill_refs")[0].GetString());
        Assert.Equal("plugin-skill:plugin_1:editing",
            response.GetProperty("skills").EnumerateArray().First()
                .GetProperty("skill_ref").GetString());
        Assert.True(response.GetProperty("tools").EnumerateArray().First().TryGetProperty(
            "input_schema", out _));
    }

    [Fact]
    public void OversizedUnicodeSchemaRoundTripsThroughExplicitPages()
    {
        var marker = string.Concat(Enumerable.Repeat("😀字段", 3_000));
        var schema = Element(JsonSerializer.Serialize(new
        {
            type = "object",
            description = marker,
            properties = new { value = new { type = "string" } },
        }));
        var descriptor = Descriptor([new WindowsAgentCapabilityToolDescriptor(
            "tool_1", "large", "Large schema", schema, "none", [])]);

        var initial = WindowsAgentCapabilityDisclosure.Response(
            descriptor, toolOption: "tool_1");
        var metadata = initial.GetProperty("tools").EnumerateArray().First();
        Assert.Equal("paged", metadata.GetProperty("schema_delivery").GetString());
        Assert.False(metadata.TryGetProperty("schema_chunk", out _));

        var builder = new StringBuilder();
        int? offset = metadata.GetProperty("next_schema_offset").GetInt32();
        while (offset is not null)
        {
            var page = WindowsAgentCapabilityDisclosure.Response(
                descriptor, toolOption: "tool_1", schemaOffset: offset);
            Assert.True(page.GetRawText().EnumerateRunes().Count() <=
                WindowsAgentCapabilityDisclosure.MaximumResponseCharacters);
            var tool = page.GetProperty("tools").EnumerateArray().First();
            builder.Append(tool.GetProperty("schema_chunk").GetString());
            offset = tool.GetProperty("next_schema_offset").ValueKind == JsonValueKind.Null
                ? null
                : tool.GetProperty("next_schema_offset").GetInt32();
        }

        var reconstructed = builder.ToString();
        Assert.Equal(schema.GetRawText(), reconstructed);
        Assert.Equal(
            Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(reconstructed)))
                .ToLowerInvariant(),
            metadata.GetProperty("schema_sha256").GetString());
    }

    [Theory]
    [InlineData("tool_1", "query", 0, null)]
    [InlineData("tool_1", null, 1, null)]
    [InlineData(null, null, 0, 0)]
    public void InvalidRequestModesFailClosed(
        string? toolOption,
        string? query,
        int offset,
        int? schemaOffset)
    {
        Assert.Throws<WindowsAgentCapabilityDisclosureException>(() =>
            WindowsAgentCapabilityDisclosure.Response(
                Descriptor([Tool("tool_1", "read", "Read")]),
                toolOption,
                query,
                offset,
                schemaOffset: schemaOffset));
    }

    [Fact]
    public void OversizedMetadataReturnsCompleteErrorInsteadOfPartialJson()
    {
        var response = WindowsAgentCapabilityDisclosure.Response(Descriptor([
            Tool("tool_1", "read", new string('x', 9_000)),
        ]));

        Assert.Equal("error", response.GetProperty("view").GetString());
        Assert.Equal("capability_description_entry_too_large",
            response.GetProperty("error").GetString());
        Assert.True(response.GetRawText().EnumerateRunes().Count() <=
            WindowsAgentCapabilityDisclosure.MaximumResponseCharacters);
    }

    private static WindowsAgentCapabilityDescriptor Descriptor(
        IReadOnlyList<WindowsAgentCapabilityToolDescriptor> tools) => new(
            "plugin_1", "Browser", "browser", "main", tools);

    private static WindowsAgentCapabilityToolDescriptor Tool(
        string option,
        string name,
        string description,
        params string[] skills) => new(
            option,
            name,
            description,
            Element("{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}"),
            "none",
            skills.Select(skill => $"plugin-skill:plugin_1:{skill}").ToArray());

    private static JsonElement Element(string json)
    {
        using var document = JsonDocument.Parse(json);
        return document.RootElement.Clone();
    }
}
