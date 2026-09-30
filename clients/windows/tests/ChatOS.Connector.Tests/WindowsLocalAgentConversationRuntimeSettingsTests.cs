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

    private static WindowsLocalAgentConversationRuntimeSettingsService CreateService(
        RuntimeSettingsHost host,
        params WindowsLocalAgentModelSnapshot[] models)
    {
        var service = new WindowsLocalAgentConversationRuntimeSettingsService(
            new WindowsLocalAgentConversationRuntimeSettingsClient(host));
        var options = models.Select(model => new ConversationModelOption(
            model.ModelConfigRef,
            model.Model,
            model.Model,
            model.ThinkingLevel)).ToArray();
        service.Configure("user-1", new WindowsLocalAgentBootstrapSnapshot(
            "user-1",
            models,
            options,
            new WindowsLocalAgentCapabilitySnapshot(
                "user-1",
                "main_chat",
                "revision",
                null,
                [],
                [])));
        return service;
    }

    private static WindowsLocalAgentModelSnapshot Model(
        string id,
        string revision,
        string? thinkingLevel) => new(
            "user-1",
            id,
            revision,
            $"env:CHATOS_LOCAL_AGENT_MODEL_{id.ToUpperInvariant().Replace('-', '_')}",
            "https://example.invalid/v1",
            id,
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
