using System.Text.Json;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class PluginSkillGateTests
{
    [Fact]
    public void SharedFixtureMatchesWindowsRuntime()
    {
        using var fixture = JsonDocument.Parse(File.ReadAllBytes(
            Path.Combine(AppContext.BaseDirectory, "Fixtures", "plugin_skill_gate_v1.json")));
        Assert.Equal(1, fixture.RootElement.GetProperty("schema_version").GetInt32());

        foreach (var fixtureCase in fixture.RootElement.GetProperty("cases").EnumerateArray())
        {
            var identifier = fixtureCase.GetProperty("id").GetString();
            try
            {
                var gate = PluginSkillGate.Parse(fixtureCase.GetProperty("gate"));
                var required = gate.RequiredSkillNames(fixtureCase.GetProperty("arguments"));
                Assert.False(fixtureCase.TryGetProperty("expected_error", out _), identifier);
                Assert.Equal(
                    fixtureCase.GetProperty("expected_catalog_skills").EnumerateArray()
                        .Select(value => value.GetString()!).ToArray(),
                    gate.CatalogSkillNames);
                Assert.Equal(
                    fixtureCase.GetProperty("expected_required_skills").EnumerateArray()
                        .Select(value => value.GetString()!).ToArray(),
                    required);
            }
            catch (PluginSkillGateException exception)
            {
                Assert.Equal(fixtureCase.GetProperty("expected_error").GetString(), exception.Code);
            }
        }
    }
}
