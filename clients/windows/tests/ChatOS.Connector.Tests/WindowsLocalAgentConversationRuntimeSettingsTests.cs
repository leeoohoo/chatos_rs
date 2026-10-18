using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentConversationRuntimeSettingsTests
{
    [Fact]
    public async Task MissingSettingsAreInitializedInTheLocalHost()
    {
        var host = new RuntimeSettingsHost
        {
            GetError = new LocalAgentHostRequestException(
                "not_found",
                "missing",
                false),
        };
        var service = CreateService(host, Model("model-1", "revision-1", "medium"));

        var settings = await service.FetchAsync("conversation-1");

        Assert.Equal("model-1", settings.SelectedModelId);
        Assert.Equal("medium", settings.SelectedThinkingLevel);
        Assert.True(settings.ReasoningEnabled);
        var put = Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut);
        Assert.Equal("conversation-1", put.ConversationId);
        Assert.Null(put.ExpectedVersion);
        Assert.Null(put.RemoteConnectionId);
    }

    [Fact]
    public async Task NewConversationPrefersTaskCapableModelOverMediaOnlyCatalogEntry()
    {
        var host = new RuntimeSettingsHost
        {
            GetError = new LocalAgentHostRequestException(
                "not_found",
                "missing",
                false),
        };
        var image = Model("image", "image-revision", null, "gpt-image-1.5");
        var text = Model("text", "text-revision", "medium", "gpt-5.6-sol");
        var service = new WindowsLocalAgentConversationRuntimeSettingsService(
            new WindowsLocalAgentConversationRuntimeSettingsClient(host));
        service.Configure("user-1", new WindowsLocalAgentBootstrapSnapshot(
            "user-1",
            [image, text],
            [
                new ConversationModelOption(
                    "image", "Image", "gpt-image-1.5", null, TaskEnabled: false),
                new ConversationModelOption(
                    "text", "Text", "gpt-5.6-sol", "medium", TaskEnabled: true),
            ],
            [],
            Capability("main"),
            Capability("installer", WindowsLocalAgentCapabilityCatalog.PluginInstallerTools)));

        var settings = await service.FetchAsync("conversation-installer");

        Assert.Equal("text", settings.SelectedModelId);
        Assert.Equal(
            "text-revision",
            Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut)
                .SelectedModelConfigRevision);
    }

    [Fact]
    public async Task ModelUpdateUsesSnapshotRevisionAndExpectedVersion()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-1", "none", false, 7),
        };
        var service = CreateService(
            host,
            Model("model-1", "revision-1", null),
            Model("model-2", "revision-2", "high"));

        var settings = await service.UpdateModelAsync("conversation-1", "model-2");

        Assert.Equal("model-2", settings.SelectedModelId);
        Assert.Equal("high", settings.SelectedThinkingLevel);
        Assert.True(settings.ReasoningEnabled);
        var put = Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut);
        Assert.Equal("revision-2", put.SelectedModelConfigRevision);
        Assert.Equal((ulong)7, put.ExpectedVersion);
    }

    [Fact]
    public async Task ReasoningDisablePersistsNoneWithoutChangingModel()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-1", "high", true, 3),
        };
        var service = CreateService(host, Model("model-1", "revision-1", "high"));

        var settings = await service.UpdateReasoningAsync("conversation-1", false);

        Assert.False(settings.ReasoningEnabled);
        Assert.Equal("none", settings.SelectedThinkingLevel);
        var put = Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut);
        Assert.Equal("model-1", put.SelectedModelConfigRef);
        Assert.Equal((ulong)3, put.ExpectedVersion);
    }

    [Fact]
    public async Task ReasoningLevelUpdatePersistsTheSelectedProviderLevel()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-1", "high", true, 4),
        };
        var service = CreateService(host, Model("model-1", "revision-1", "high"));

        var settings = await service.UpdateReasoningLevelAsync("conversation-1", "low");

        Assert.True(settings.ReasoningEnabled);
        Assert.Equal("low", settings.SelectedThinkingLevel);
        Assert.Equal("low", Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut).SelectedThinkingLevel);
    }

    [Fact]
    public async Task RemoteConnectionUpdatePreservesModelAndReasoningSettings()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-1", "high", true, 5),
        };
        var service = CreateService(host, Model("model-1", "revision-1", "high"));

        var settings = await service.UpdateRemoteConnectionAsync(
            "conversation-1",
            "remote-1");

        Assert.Equal("remote-1", settings.RemoteConnectionId);
        Assert.Equal("model-1", settings.SelectedModelId);
        Assert.True(settings.ReasoningEnabled);
        var put = Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut);
        Assert.Equal("remote-1", put.RemoteConnectionId);
        Assert.Equal("high", put.SelectedThinkingLevel);
        Assert.Equal((ulong)5, put.ExpectedVersion);
    }

    [Fact]
    public async Task FetchRejectsAStaleRevisionWithoutRewritingThePersistedSelection()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-old", "high", true, 8) with
            {
                RemoteConnectionId = "remote-1",
            },
        };
        var service = CreateService(host, Model("model-1", "revision-new", "medium"));

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.FetchAsync("conversation-1"));

        Assert.Contains("revision is unavailable", error.Message, StringComparison.Ordinal);
        Assert.Null(host.LastPut);
    }

    [Fact]
    public async Task ExplicitModelUpdateReplacesAStaleRevisionWithTheExactCurrentSnapshot()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("model-1", "revision-old", "high", true, 8) with
            {
                RemoteConnectionId = "remote-1",
            },
        };
        var service = CreateService(host, Model("model-1", "revision-new", "medium"));

        var settings = await service.UpdateModelAsync("conversation-1", "model-1");

        Assert.Equal("model-1", settings.SelectedModelId);
        Assert.Equal("medium", settings.SelectedThinkingLevel);
        Assert.Equal("remote-1", settings.RemoteConnectionId);
        var put = Assert.IsType<PutLocalConversationRuntimeSettingsCommand>(host.LastPut);
        Assert.Equal("revision-new", put.SelectedModelConfigRevision);
        Assert.Equal((ulong)8, put.ExpectedVersion);
        Assert.Equal("remote-1", put.RemoteConnectionId);
    }

    [Fact]
    public async Task FetchRejectsARemovedModelWithoutRewritingThePersistedSelection()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("removed-model", "removed-revision", "high", true, 2),
        };
        var service = CreateService(host, Model("model-available", "revision-1", "medium"));

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.FetchAsync("conversation-1"));

        Assert.Contains("revision is unavailable", error.Message, StringComparison.Ordinal);
        Assert.Null(host.LastPut);
    }

    [Fact]
    public async Task SolCapabilityRepairsLegacyOffToLowAndCannotBeDisabled()
    {
        var host = new RuntimeSettingsHost
        {
            Current = Settings("sol", "revision-1", "none", false, 6),
        };
        var service = CreateService(
            host,
            Model("sol", "revision-1", "disabled", "gpt-6.1-sol-2026-10-09"));

        var fetched = await service.FetchAsync("conversation-1");
        var options = await service.FetchAvailableModelsAsync();

        Assert.Equal("low", fetched.SelectedThinkingLevel);
        Assert.True(fetched.ReasoningEnabled);
        Assert.Equal(["low", "medium", "high", "xhigh", "max"],
            Assert.Single(options).EffectiveThinkingLevels);
        var disabled = await service.UpdateReasoningAsync("conversation-1", false);
        Assert.Equal("low", disabled.SelectedThinkingLevel);
        Assert.True(disabled.ReasoningEnabled);
    }

    private static WindowsLocalAgentConversationRuntimeSettingsService CreateService(
        RuntimeSettingsHost host,
        params WindowsLocalAgentModelSnapshot[] models)
    {
        var service = new WindowsLocalAgentConversationRuntimeSettingsService(
            new WindowsLocalAgentConversationRuntimeSettingsClient(host));
        var options = models.Select(model =>
        {
            var capability = WindowsModelReasoningCapabilities.ForModel(model.Model);
            return new ConversationModelOption(
                model.ModelConfigRef,
                model.Model,
                model.Model,
                capability?.Normalize(model.ThinkingLevel) ?? model.ThinkingLevel,
                ThinkingLevels: capability?.Levels ??
                    new[] { "none", "low", "medium", "high", "xhigh" });
        }).ToArray();
        service.Configure("user-1", new WindowsLocalAgentBootstrapSnapshot(
            "user-1",
            models,
            options,
            [],
            new WindowsLocalAgentCapabilitySnapshot(
                "user-1",
                "main_chat",
                "revision",
                null,
                [],
                []),
            new WindowsLocalAgentCapabilitySnapshot(
                "user-1",
                "main_chat",
                "installer-revision",
                WindowsLocalAgentCapabilityCatalog.PluginInstallerInstructions,
                [],
                WindowsLocalAgentCapabilityCatalog.PluginInstallerTools)));
        return service;
    }

    private static WindowsLocalAgentCapabilitySnapshot Capability(
        string revision,
        IReadOnlyList<System.Text.Json.JsonElement>? tools = null) => new(
        "user-1",
        "main_chat",
        revision,
        null,
        [],
        tools ?? []);

    private static WindowsLocalAgentModelSnapshot Model(
        string id,
        string revision,
        string? thinkingLevel,
        string? modelName = null) => new(
            "user-1",
            id,
            revision,
            $"env:CHATOS_LOCAL_AGENT_MODEL_{id.ToUpperInvariant().Replace('-', '_')}",
            "https://example.invalid/v1",
            modelName ?? id,
            "openai",
            true,
            false,
            null,
            null,
            null,
            thinkingLevel,
            false,
            null,
            null,
            null);

    private static WindowsLocalAgentConversationRuntimeSettings Settings(
        string modelId,
        string revision,
        string? thinkingLevel,
        bool reasoningEnabled,
        ulong version) => new(
            "user-1",
            "conversation-1",
            modelId,
            revision,
            thinkingLevel,
            null,
            reasoningEnabled,
            version,
            1_000);

    private sealed class RuntimeSettingsHost : ILocalAgentHostClient
    {
        public WindowsLocalAgentConversationRuntimeSettings? Current { get; set; }

        public LocalAgentHostRequestException? GetError { get; set; }

        public PutLocalConversationRuntimeSettingsCommand? LastPut { get; private set; }

        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            if (command is GetLocalConversationRuntimeSettingsCommand)
            {
                if (GetError is { } error)
                {
                    GetError = null;
                    throw error;
                }
                return Response<TResponse>(Current ?? throw new InvalidOperationException(
                    "No fake Local Agent runtime settings are configured."));
            }
            if (command is PutLocalConversationRuntimeSettingsCommand put)
            {
                LastPut = put;
                Current = new WindowsLocalAgentConversationRuntimeSettings(
                    put.OwnerUserId,
                    put.ConversationId,
                    put.SelectedModelConfigRef,
                    put.SelectedModelConfigRevision,
                    put.SelectedThinkingLevel,
                    put.RemoteConnectionId,
                    put.ReasoningEnabled,
                    put.ExpectedVersion is { } version ? version + 1 : 1,
                    1_000);
                return Response<TResponse>(Current);
            }
            throw new InvalidOperationException($"Unexpected command: {typeof(TCommand).Name}");
        }

        private static Task<TResponse> Response<TResponse>(
            WindowsLocalAgentConversationRuntimeSettings settings) =>
            Task.FromResult((TResponse)(object)new LocalConversationRuntimeSettingsResult(
                "conversation_runtime_settings",
                settings));
    }
}
