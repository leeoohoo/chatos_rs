using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentControlPlaneTests
{
    [Fact]
    public async Task LatestSnapshotsUseOwnerScopedProtocolCommands()
    {
        var host = new ControlPlaneHost();
        var client = new WindowsLocalAgentControlPlaneClient(host);

        var models = await client.LatestModelsAsync("user-1");
        var capabilities = await client.LatestCapabilitiesAsync("user-1", "main_chat");

        Assert.Single(models);
        Assert.Equal("model-1", models[0].ModelConfigRef);
        Assert.Equal("user-1", capabilities.OwnerUserId);
        Assert.Equal("main_chat", capabilities.ProfileKey);
        Assert.Equal(
            ["list_latest_model_config_snapshots", "get_latest_capability_policy_snapshot"],
            host.Commands.Select(command => command.GetProperty("type").GetString()!).ToArray());
        Assert.All(host.Commands, command =>
            Assert.Equal("user-1", command.GetProperty("owner_user_id").GetString()));
        Assert.Equal("main_chat", host.Commands[1].GetProperty("profile_key").GetString());
    }

    [Fact]
    public void ModelSnapshotProtocolContainsReferenceButNoCredentialValue()
    {
        var snapshot = new WindowsLocalAgentModelSnapshot(
            "user-1",
            "model-1",
            "sha256-revision",
            "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            "https://example.invalid/v1",
            "model-1",
            "openai",
            true,
            false,
            null,
            0.7,
            4096,
            "medium",
            false,
            null,
            null,
            null);

        var json = JsonSerializer.Serialize(snapshot, new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        });

        Assert.Contains("credential_ref", json);
        Assert.DoesNotContain("api_key", json);
        Assert.DoesNotContain("test-secret", json);
    }

    [Fact]
    public void ModelRevisionHashesTheNormalizedPublishedThinkingLevel()
    {
        var legacy = ModelSnapshot(thinkingLevel: null);
        var normalized = ModelSnapshot(
            WindowsLocalAgentBootstrapService.ModelThinkingLevel(
                null,
                "gpt-5.6-sol"));

        Assert.Equal("medium", normalized.ThinkingLevel);
        Assert.NotEqual(
            WindowsLocalAgentBootstrapService.ModelRevision(legacy, supportsReasoning: true),
            WindowsLocalAgentBootstrapService.ModelRevision(normalized, supportsReasoning: true));
        Assert.Equal(
            "sha256-83226954b7259a367c88146c78274da3a5af575ad7f48b848fe625b47c77b7aa",
            WindowsLocalAgentBootstrapService.ModelRevision(normalized, supportsReasoning: true));
    }

    [Fact]
    public void ModelRevisionDistinguishesUnknownAndFalseImageSupport()
    {
        var unknown = ModelSnapshot(thinkingLevel: "medium") with
        {
            SupportsImages = null,
        };
        var unsupported = unknown with { SupportsImages = false };

        Assert.NotEqual(
            WindowsLocalAgentBootstrapService.ModelRevision(unknown, supportsReasoning: true),
            WindowsLocalAgentBootstrapService.ModelRevision(unsupported, supportsReasoning: true));
    }

    [Fact]
    public void SolDisabledAliasesFreezeAsLowInsteadOfAFalseOffState()
    {
        foreach (var alias in new[] { "none", "minimal", "off", "disabled" })
        {
            Assert.Equal(
                "low",
                WindowsLocalAgentBootstrapService.ModelThinkingLevel(
                    alias,
                    "gpt-6.1-sol-2026-10-09"));
        }
    }

    [Fact]
    public void MainChatCatalogPublishesTheCompleteReservedTaskSurface()
    {
        var names = WindowsLocalAgentCapabilityCatalog.MainChatTools
            .Select(tool => tool.GetProperty("name").GetString()!)
            .ToArray();

        Assert.Equal(
            [
                "list_tasks", "get_task", "create_task", "create_tasks_with_prerequisites",
                "cancel_task", "wait_for_task_completion", "get_task_dependency_graph",
                "resolve_task_followup", "requirement_survey_create",
                "requirement_plan_save", "local_attachment_read",
            ],
            names);
        Assert.All(WindowsLocalAgentCapabilityCatalog.MainChatTools, tool =>
            Assert.Equal("function", tool.GetProperty("type").GetString()));
    }

    private static WindowsLocalAgentModelSnapshot ModelSnapshot(string? thinkingLevel) => new(
        "user-1",
        "provider_model_1",
        string.Empty,
        "env:CHATOS_LOCAL_AGENT_MODEL_PROVIDER_MODEL_1",
        "https://example.invalid/v1",
        "gpt-5.6-sol",
        "gpt",
        true,
        true,
        null,
        null,
        null,
        thinkingLevel,
        false,
        null,
        null,
        null);

    [Fact]
    public void EmptyMainChatSelectionsDoNotPublishInvalidEmptyEnums()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor([]);
        foreach (var tool in tools)
        {
            AssertNoEmptyEnum(tool);
            var parameters = tool.GetProperty("parameters");
            Assert.False(parameters.TryGetProperty("required", out var required) &&
                required.ValueKind == JsonValueKind.Array && required.GetArrayLength() == 0);
        }

        Assert.Equal(
            [
                "list_tasks", "get_task", "create_task", "create_tasks_with_prerequisites",
                "cancel_task", "wait_for_task_completion", "get_task_dependency_graph",
                "resolve_task_followup", "requirement_survey_create",
                "requirement_plan_save", "local_attachment_read",
            ],
            tools.Select(tool => tool.GetProperty("name").GetString()!).ToArray());

        var create = tools
            .Single(tool => tool.GetProperty("name").GetString() == "create_task");
        var properties = create.GetProperty("parameters").GetProperty("properties");
        Assert.Equal(
            [
                "title", "objective", "expected_outputs", "acceptance_criteria",
                "requires_execution", "enabled_builtin_kinds",
            ],
            create.GetProperty("parameters").GetProperty("required")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
        Assert.False(create.GetProperty("parameters")
            .GetProperty("additionalProperties").GetBoolean());
        Assert.Equal(0, properties.GetProperty("enabled_builtin_kinds")
            .GetProperty("maxItems").GetInt32());
        Assert.Equal(0, properties.GetProperty("external_mcp_config_ids")
            .GetProperty("maxItems").GetInt32());
        Assert.Equal(0, properties.GetProperty("plugin_hints")
            .GetProperty("maxItems").GetInt32());

        Assert.Equal(
            ["graph_goal", "decomposition_rationale", "tasks"],
            tools.Single(tool => tool.GetProperty("name").GetString() ==
                    "create_tasks_with_prerequisites")
                .GetProperty("parameters").GetProperty("required")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
        var graphItems = tools
            .Single(tool => tool.GetProperty("name").GetString() ==
                "create_tasks_with_prerequisites")
            .GetProperty("parameters").GetProperty("properties")
            .GetProperty("tasks").GetProperty("items");
        Assert.Equal(
            [
                "client_ref", "title", "objective", "expected_outputs",
                "acceptance_criteria", "requires_execution", "enabled_builtin_kinds",
            ],
            graphItems.GetProperty("required")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
        Assert.Equal(2, tools.Single(tool => tool.GetProperty("name").GetString() ==
                "create_tasks_with_prerequisites")
            .GetProperty("parameters").GetProperty("properties")
            .GetProperty("tasks").GetProperty("minItems").GetInt32());
        Assert.False(graphItems.GetProperty("additionalProperties").GetBoolean());
    }

    [Fact]
    public void MainChatCreateTaskPublishesTheFrozenAcceptanceContract()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor(
            [],
            builtinChoices:
            [
                new("CodeMaintainerRead", "Read project"),
                new("CodeMaintainerWrite", "Modify project"),
            ]);
        var properties = tools.Single(tool =>
                tool.GetProperty("name").GetString() == "create_task")
            .GetProperty("parameters").GetProperty("properties");

        Assert.Equal(1, properties.GetProperty("expected_outputs")
            .GetProperty("minItems").GetInt32());
        Assert.Equal(1, properties.GetProperty("acceptance_criteria")
            .GetProperty("minItems").GetInt32());
        Assert.Equal(32_000, properties.GetProperty("final_response_max_chars")
            .GetProperty("maximum").GetInt32());
        Assert.Equal(200, properties.GetProperty("owned_paths")
            .GetProperty("maxItems").GetInt32());
        Assert.Equal(
            ["CodeMaintainerRead", "CodeMaintainerWrite"],
            properties.GetProperty("enabled_builtin_kinds").GetProperty("items")
                .GetProperty("enum").EnumerateArray()
                .Select(value => value.GetString()!).ToArray());
    }

    [Fact]
    public void MainChatTaskGraphSchemaMatchesSharedNotificationAndFinalDeliveryContract()
    {
        var graph = WindowsLocalAgentCapabilityCatalog.MainChatTools
            .Single(tool => tool.GetProperty("name").GetString() ==
                "create_tasks_with_prerequisites")
            .GetProperty("parameters").GetProperty("properties");
        var notifications = graph.GetProperty("notification_policy");

        Assert.Equal(
            ["progress_summary", "all_tasks", "milestones", "graph_summary"],
            notifications.GetProperty("enum").EnumerateArray()
                .Select(value => value.GetString()!).ToArray());
        Assert.Equal("progress_summary", notifications.GetProperty("default").GetString());
        Assert.Equal("string", graph.GetProperty("final_delivery_ref")
            .GetProperty("type").GetString());
        Assert.Equal(1, graph.GetProperty("final_delivery_ref")
            .GetProperty("minLength").GetInt32());
    }

    [Fact]
    public void MainChatTaskSchemaFreezesOnlyEligibleModelChoices()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor(
            [],
            taskModelChoices:
            [
                new("model-complex", "Sol (openai/gpt-6.1-sol) - complex work"),
                new("model-simple", "Luna (openai/gpt-6-luna) - simple work"),
                new("model-complex", "duplicate ignored"),
            ]);

        foreach (var schema in new[]
        {
            tools.Single(tool => tool.GetProperty("name").GetString() == "create_task")
                .GetProperty("parameters").GetProperty("properties")
                .GetProperty("default_model_config_id"),
            tools.Single(tool => tool.GetProperty("name").GetString() ==
                    "create_tasks_with_prerequisites")
                .GetProperty("parameters").GetProperty("properties")
                .GetProperty("tasks").GetProperty("items").GetProperty("properties")
                .GetProperty("default_model_config_id"),
        })
        {
            Assert.Equal(["model-complex", "model-simple"], schema.GetProperty("enum")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
            Assert.Equal(
                ["Sol (openai/gpt-6.1-sol) - complex work",
                    "Luna (openai/gpt-6-luna) - simple work"],
                schema.GetProperty("x-enum-labels").EnumerateArray()
                    .Select(value => value.GetString()!).ToArray());
            Assert.Equal("model-complex", schema.GetProperty("oneOf")[0]
                .GetProperty("const").GetString());
        }
    }

    [Fact]
    public void MainChatTaskSchemaFreezesInstalledPluginChoices()
    {
        var tools = WindowsLocalAgentCapabilityCatalog.MainChatToolsFor(
        [
            Choice("open-computer-use@chatos-marketplace", "computer-use-mcp",
                "Computer Use", "Desktop control."),
            Choice("chatos-browser-cdp@chatos-marketplace", "browser-cdp-mcp",
                "Browser CDP", "Browser pages."),
        ]);
        var create = tools.Single(tool =>
            tool.GetProperty("name").GetString() == "create_task");
        var choices = create
            .GetProperty("parameters")
            .GetProperty("properties")
            .GetProperty("plugin_hints")
            .GetProperty("items");
        var options = choices.GetProperty("oneOf").EnumerateArray().ToArray();

        Assert.Equal(
            ["chatos-browser-cdp@chatos-marketplace", "open-computer-use@chatos-marketplace"],
            options.Select(value => value.GetProperty("properties")
                .GetProperty("plugin_key").GetProperty("const").GetString()!)
            .ToArray());
        Assert.Equal(
            ["browser-cdp-mcp", "computer-use-mcp"],
            options.Select(value => value.GetProperty("properties")
                .GetProperty("component_key").GetProperty("const").GetString()!).ToArray());
        Assert.All(options, value => Assert.Equal(
            ["plugin_key", "component_key"],
            value.GetProperty("required").EnumerateArray()
                .Select(item => item.GetString()!).ToArray()));
        var titles = options
            .Select(value => value.GetProperty("title").GetString()!)
            .ToArray();
        Assert.Contains(titles, title =>
            title.Contains("Browser pages.", StringComparison.Ordinal) &&
            title.Contains("only for websites", StringComparison.Ordinal));
        Assert.Contains(titles, title =>
            title.Contains("Desktop control.", StringComparison.Ordinal) &&
            title.Contains("native desktop applications", StringComparison.Ordinal));
    }

    [Fact]
    public void TaskExecutionCatalogPublishesProjectAndTerminalTools()
    {
        var names = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools
            .Select(tool => tool.GetProperty("name").GetString()!)
            .ToArray();

        Assert.Equal(
            new[] {
                "local_attachment_read", "project_list", "project_read", "project_search",
                "project_write", "project_copy", "project_move", "terminal_exec",
                "capability_search", "capability_describe", "agent_skill_activate",
                "agent_skill_read_resource", "capability_invoke",
                "remote_connection_controller_test_connection",
                "remote_connection_controller_run_command",
                "remote_connection_controller_list_directory",
                "remote_connection_controller_read_file",
                "remote_connection_controller_download_file",
                "remote_connection_controller_upload_file",
            },
            names);
        Assert.True(names.ToHashSet().SetEquals(
            WindowsLocalAgentCapabilityCatalog.TaskExecutionToolNames));
        Assert.All(WindowsLocalAgentCapabilityCatalog.TaskExecutionTools, tool =>
        {
            Assert.Equal("function", tool.GetProperty("type").GetString());
            Assert.Equal(JsonValueKind.Object, tool.GetProperty("parameters").ValueKind);
        });
        foreach (var name in new[] { "project_copy", "project_move" })
        {
            var mutation = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
                tool.GetProperty("name").GetString() == name);
            Assert.Equal(
                ["source_path", "target_path"],
                mutation.GetProperty("parameters").GetProperty("required")
                    .EnumerateArray().Select(value => value.GetString()!).ToArray());
            Assert.False(mutation.GetProperty("parameters")
                .GetProperty("additionalProperties").GetBoolean());
        }
        var projectList = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
            tool.GetProperty("name").GetString() == "project_list");
        Assert.Contains("incomplete", projectList.GetProperty("description").GetString(),
            StringComparison.Ordinal);
        Assert.Contains("empty", projectList.GetProperty("description").GetString(),
            StringComparison.Ordinal);
        var projectWrite = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
            tool.GetProperty("name").GetString() == "project_write");
        Assert.Contains("unified diff", projectWrite.GetProperty("description").GetString(),
            StringComparison.Ordinal);
        Assert.Contains("truncated", projectWrite.GetProperty("description").GetString(),
            StringComparison.Ordinal);
        Assert.Equal(
            "native-windows-main-chat-v17-project-write-empty-content",
            WindowsLocalAgentCapabilityCatalog.Revision);
        var projectMove = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
            tool.GetProperty("name").GetString() == "project_move");
        Assert.Contains("case-only rename", projectMove.GetProperty("description").GetString(),
            StringComparison.Ordinal);
        Assert.True(new[] {
            "project_list", "project_read", "project_search", "project_write", "project_copy",
            "project_move",
            "terminal_exec",
            "remote_connection_controller_test_connection",
            "remote_connection_controller_run_command",
            "remote_connection_controller_list_directory",
            "remote_connection_controller_read_file",
            "remote_connection_controller_download_file",
            "remote_connection_controller_upload_file",
        }.ToHashSet().SetEquals(WindowsLocalAgentCapabilityCatalog.ProjectToolNames));
        Assert.True(new[] {
            "capability_search", "capability_describe", "agent_skill_activate",
            "agent_skill_read_resource", "capability_invoke",
        }.ToHashSet().SetEquals(WindowsLocalAgentCapabilityCatalog.PluginToolNames));

        var describe = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
            tool.GetProperty("name").GetString() == "capability_describe");
        Assert.False(describe.GetProperty("strict").GetBoolean());
        var describeProperties = describe.GetProperty("parameters").GetProperty("properties");
        Assert.Equal(new[] { "string", "null" }, describeProperties.GetProperty("tool_option")
            .GetProperty("type").EnumerateArray().Select(value => value.GetString()!).ToArray());
        Assert.Equal(new[] { "integer", "null" }, describeProperties.GetProperty("schema_offset")
            .GetProperty("type").EnumerateArray().Select(value => value.GetString()!).ToArray());

        foreach (var name in new[] { "agent_skill_activate", "agent_skill_read_resource" })
        {
            var skillTool = WindowsLocalAgentCapabilityCatalog.TaskExecutionTools.Single(tool =>
                tool.GetProperty("name").GetString() == name);
            var parameters = skillTool.GetProperty("parameters");
            Assert.Contains("skill_ref", parameters.GetProperty("required")
                .EnumerateArray().Select(value => value.GetString()));
            Assert.False(parameters.GetProperty("properties").TryGetProperty(
                "plugin_option", out _));
            Assert.False(parameters.GetProperty("properties").TryGetProperty(
                "skill_name", out _));
        }
    }

    private static void AssertNoEmptyEnum(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            foreach (var property in value.EnumerateObject())
            {
                if (property.Name is "enum" or "oneOf")
                {
                    Assert.True(property.Value.ValueKind != JsonValueKind.Array ||
                        property.Value.GetArrayLength() > 0,
                        $"Schema property '{property.Name}' must not be an empty array.");
                }
                AssertNoEmptyEnum(property.Value);
            }
        }
        else if (value.ValueKind == JsonValueKind.Array)
        {
            foreach (var item in value.EnumerateArray()) AssertNoEmptyEnum(item);
        }
    }

    [Fact]
    public void TaskToolAuthorizationPreservesTheSelectedTaskScope()
    {
        using var document = JsonDocument.Parse("""
        {
          "tool_options": {
            "requires_execution": true,
            "enabled_builtin_kinds": ["CodeMaintainerRead", "CodeMaintainerWrite"],
            "plugin_hints": [{"plugin_key": "plugin-1", "component_key": "main"}],
            "plugin_release_snapshots": [{
              "plugin_key": "plugin-1",
              "component_key": "main",
              "plugin_id": "plugin-id-1",
              "release_id": "release-1",
              "version": "1.0.0",
              "artifact_sha256": "sha256-1"
            }]
          }
        }
        """);

        var authorization = WindowsLocalAgentTaskToolAuthorization.Resolve(
            document.RootElement);

        Assert.True(authorization.Allows("project_read"));
        Assert.True(authorization.Allows("project_write"));
        Assert.True(authorization.Allows("project_copy"));
        Assert.True(authorization.Allows("project_move"));
        Assert.False(authorization.Allows("terminal_exec"));
        Assert.True(authorization.Allows("capability_search"));
        Assert.True(authorization.PluginKeys.SetEquals(["plugin-1"]));
        Assert.True(authorization.PluginSelectionIds.SetEquals([
            WindowsLocalAgentPluginChoice.SelectionIdFor("plugin-1", "main")
        ]));
    }

    [Fact]
    public void ProjectWorkspaceValidationFailureIsSafeAndRetryable()
    {
        var invocation = ToolInvocation("project_write", sideEffecting: true);

        var outcome = WindowsLocalAgentPlatformToolWorker.FailureOutcome(
            invocation,
            new WorkspaceRequestException(
                400,
                "workspace_path_unsafe",
                "Workspace path is unsafe."));

        Assert.Equal("failed", outcome.GetProperty("type").GetString());
        Assert.Equal("Workspace path is unsafe.", outcome.GetProperty("error").GetString());
        var detail = outcome.GetProperty("detail");
        Assert.Equal("native_project_workspace_validation",
            detail.GetProperty("phase").GetString());
        Assert.Equal("workspace_path_unsafe", detail.GetProperty("code").GetString());
        Assert.Equal(400, detail.GetProperty("status_code").GetInt32());
    }

    [Fact]
    public void UnknownProjectMutationFailureStillRequiresReviewAndHidesDetails()
    {
        var outcome = WindowsLocalAgentPlatformToolWorker.FailureOutcome(
            ToolInvocation("project_write", sideEffecting: true),
            new IOException(@"failed at C:\sensitive\secret.txt"));

        Assert.Equal("needs_review", outcome.GetProperty("type").GetString());
        Assert.Equal("Local platform tool failed.", outcome.GetProperty("error").GetString());
        Assert.DoesNotContain("sensitive", outcome.GetRawText(), StringComparison.OrdinalIgnoreCase);
    }

    private static WindowsLocalToolInvocation ToolInvocation(
        string toolName,
        bool sideEffecting) => new(
            "invocation-1",
            "run-1",
            "batch-1",
            "call-1",
            toolName,
            JsonSerializer.SerializeToElement(new { path = "file.txt" }),
            sideEffecting,
            sideEffecting,
            sideEffecting ? "approved" : "not_required",
            null,
            null,
            null,
            "claimed",
            null,
            null,
            1,
            "claim-1",
            null,
            1_000,
            1_000);

    [Theory]
    [InlineData("""{"tool_options":{"requires_execution":true,"enabled_builtin_kinds":[],"plugin_hints":[{"plugin_key":"plugin-1"}],"plugin_release_snapshots":[]}}""")]
    [InlineData("""{"tool_options":{"requires_execution":true,"enabled_builtin_kinds":[],"plugin_hints":[{"plugin_key":"plugin-1","component_key":"main"}],"plugin_release_snapshots":[]}}""")]
    public void TaskPluginAuthorizationFailsClosedWithoutExactComponentSnapshot(string json)
    {
        using var document = JsonDocument.Parse(json);

        Assert.Throws<InvalidOperationException>(() =>
            WindowsLocalAgentTaskToolAuthorization.Resolve(document.RootElement));
    }

    [Theory]
    [InlineData("release-2", "1.0.0", "sha256-main")]
    [InlineData("release-main", "2.0.0", "sha256-main")]
    [InlineData("release-main", "1.0.0", "sha256-changed")]
    public void FrozenPluginChoiceRejectsReleaseDrift(
        string releaseId,
        string version,
        string artifactSha256)
    {
        var snapshot = new WindowsLocalAgentPluginReleaseSnapshot(
            "plugin-1", "main", "id-main", "release-main", "1.0.0", "sha256-main");
        var enabled = new Dictionary<string, LocalConnectorPlugin>(StringComparer.Ordinal)
        {
            ["id-main"] = new(
                "id-main", "plugin-1", "Plugin", "Description", "Tools", "ChatOS",
                version, true, false, true, true, ["process.spawn"]),
        };
        var records = new Dictionary<string, InstalledPluginRecord>(StringComparer.Ordinal)
        {
            ["id-main"] = new(
                "id-main", releaseId, version, artifactSha256, "C:\\plugin",
                DateTimeOffset.UtcNow, ["process.spawn"], PluginKey: "plugin-1"),
        };

        var error = Assert.Throws<InvalidOperationException>(() =>
            WindowsLocalAgentPluginToolExecutor.ValidateFrozenChoice(
                snapshot, enabled, records));

        Assert.Contains("frozen", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void FrozenPluginChoiceKeepsTheExactSelectedComponent()
    {
        var snapshot = new WindowsLocalAgentPluginReleaseSnapshot(
            "plugin-1", "task-execution-mcp", "id-main", "release-main", "1.0.0",
            "sha256-main");
        var enabled = new Dictionary<string, LocalConnectorPlugin>(StringComparer.Ordinal)
        {
            ["id-main"] = new(
                "id-main", "plugin-1", "Plugin", "Description", "Tools", "ChatOS",
                "1.0.0", true, false, true, true, ["process.spawn"]),
        };
        var records = new Dictionary<string, InstalledPluginRecord>(StringComparer.Ordinal)
        {
            ["id-main"] = new(
                "id-main", "release-main", "1.0.0", "sha256-main", "C:\\plugin",
                DateTimeOffset.UtcNow, ["process.spawn"], PluginKey: "plugin-1"),
        };

        var choice = WindowsLocalAgentPluginToolExecutor.ValidateFrozenChoice(
            snapshot, enabled, records);

        Assert.Equal("task-execution-mcp", choice.ComponentKey);
        Assert.Equal(WindowsLocalAgentPluginChoice.SelectionIdFor(
                snapshot.PluginKey, snapshot.ComponentKey),
            choice.SelectionId);
    }

    private static WindowsLocalAgentPluginChoice Choice(
        string pluginKey,
        string componentKey,
        string displayName,
        string description) => new(
            pluginKey,
            componentKey,
            $"id-{componentKey}",
            $"release-{componentKey}",
            "1.0.0",
            $"sha256-{componentKey}",
            displayName,
            description);

    private sealed class ControlPlaneHost : ILocalAgentHostClient
    {
        private static readonly JsonSerializerOptions JsonOptions = new()
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        };

        public List<JsonElement> Commands { get; } = [];

        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            var encoded = JsonSerializer.SerializeToElement(command, JsonOptions);
            Commands.Add(encoded);
            var type = encoded.GetProperty("type").GetString();
            object result = type switch
            {
                "list_latest_model_config_snapshots" => new
                {
                    type = "model_config_snapshots",
                    snapshots = new[] { ModelSnapshot() },
                },
                "get_latest_capability_policy_snapshot" => new
                {
                    type = "capability_policy_snapshot",
                    snapshot = CapabilitySnapshot(),
                },
                _ => throw new InvalidOperationException($"Unexpected command: {type}"),
            };
            var json = JsonSerializer.Serialize(result, JsonOptions);
            return Task.FromResult(JsonSerializer.Deserialize<TResponse>(json, JsonOptions)!);
        }

        private static WindowsLocalAgentModelSnapshot ModelSnapshot() => new(
            "user-1", "model-1", "revision-1",
            "env:CHATOS_LOCAL_AGENT_MODEL_MODEL_1",
            "https://example.invalid/v1", "gpt-test", "openai", true, false,
            null, null, null, "medium", false, null, null, null);

        private static WindowsLocalAgentCapabilitySnapshot CapabilitySnapshot() => new(
            "user-1", "main_chat", "policy-1", "Use task tools.", [], []);
    }
}
