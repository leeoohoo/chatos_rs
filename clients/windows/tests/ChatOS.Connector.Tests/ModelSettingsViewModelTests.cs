using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class ModelSettingsViewModelTests
{
    [Fact]
    public async Task LoadsCloudModelsAndLocalApprovalSelection()
    {
        var service = new FakeRuntimeSettingsService();
        var store = new MemoryModelSettingsStore(new ConnectorModelSettings(6, "model-two"));
        var viewModel = Create(service, store);

        await viewModel.OpenAsync();

        Assert.Equal(2, viewModel.AvailableModels.Count);
        Assert.Equal("model-two", viewModel.SelectedApprovalModel?.Id);
        Assert.Equal(6, viewModel.ModelRequestMaxRetries);
    }

    [Fact]
    public async Task MissingPreviousModelRequiresExplicitReplacement()
    {
        var viewModel = Create(
            new FakeRuntimeSettingsService(),
            new MemoryModelSettingsStore(new ConnectorModelSettings(5, "removed-model")));

        await viewModel.LoadAsync();

        Assert.Null(viewModel.SelectedApprovalModel);
        Assert.Contains("不可用", viewModel.ActionMessage);
    }

    [Fact]
    public async Task SavePersistsNormalizedRetryAndSelectedModel()
    {
        var store = new MemoryModelSettingsStore(ConnectorModelSettings.Default);
        var viewModel = Create(new FakeRuntimeSettingsService(), store);
        await viewModel.LoadAsync();
        viewModel.ModelRequestMaxRetries = 42;
        viewModel.SelectedApprovalModel = viewModel.AvailableModels[0];

        await viewModel.SaveAsync();

        Assert.Equal(new ConnectorModelSettings(10, "model-one"), store.Settings);
        Assert.Equal(10, viewModel.ModelRequestMaxRetries);
    }

    private static ModelSettingsViewModel Create(
        IConversationRuntimeSettingsService service,
        IConnectorModelSettingsStore store)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        var localization = new LocalizationViewModel(preferences, dispatcher);
        return new ModelSettingsViewModel(service, store, localization, dispatcher);
    }

    private sealed class FakeRuntimeSettingsService : IConversationRuntimeSettingsService
    {
        public Task<ConversationRuntimeSettings> FetchAsync(
            string conversationId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConversationModelOption>>(
            [
                new("model-one", "Model One", "provider/model-one", null),
                new("model-two", "Model Two", "provider/model-two", "high"),
            ]);

        public Task<ConversationRuntimeSettings> UpdateModelAsync(
            string conversationId,
            string modelId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<ConversationRuntimeSettings> UpdatePlanModeAsync(
            string conversationId,
            bool enabled,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<ConversationRuntimeSettings> UpdateReasoningAsync(
            string conversationId,
            bool enabled,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class MemoryModelSettingsStore(ConnectorModelSettings settings)
        : IConnectorModelSettingsStore
    {
        public ConnectorModelSettings Settings { get; private set; } = settings;

        public Task<ConnectorModelSettings> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Settings);

        public Task SaveAsync(
            ConnectorModelSettings settings,
            CancellationToken cancellationToken = default)
        {
            Settings = settings;
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(
            AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
