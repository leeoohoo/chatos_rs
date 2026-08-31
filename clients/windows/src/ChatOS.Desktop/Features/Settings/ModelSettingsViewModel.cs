using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Connector.Approval;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class ModelSettingsViewModel : ObservableObject
{
    private readonly IConversationRuntimeSettingsService _models;
    private readonly IConnectorModelSettingsStore _store;
    private readonly IApprovalReviewerReadinessService? _reviewerReadiness;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _operationGate = new(1, 1);

    public ModelSettingsViewModel(
        IConversationRuntimeSettingsService models,
        IConnectorModelSettingsStore store,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher,
        IApprovalReviewerReadinessService? reviewerReadiness = null)
    {
        _models = models;
        _store = store;
        _localization = localization;
        _dispatcher = dispatcher;
        _reviewerReadiness = reviewerReadiness;
        _localization.PropertyChanged += (_, _) =>
        {
            foreach (var item in AvailableModels) item.ApplyLocalization(_localization);
            OnPropertyChanged(string.Empty);
        };
    }

    public ObservableCollection<ConnectorModelOptionViewModel> AvailableModels { get; } = [];

    [ObservableProperty]
    private ConnectorModelOptionViewModel? _selectedApprovalModel;

    [ObservableProperty]
    private int _modelRequestMaxRetries = 5;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(ReviewerHint))]
    private ApprovalReviewerReadinessState _reviewerReadinessState =
        ApprovalReviewerReadinessState.ManagedConfigurationInvalid;

    public string SectionDescription => _localization.Text(
        "同步 ChatOS 已启用模型，并选择这台 Windows 设备未来用于本机审批的模型。",
        "Sync enabled ChatOS models and select the model this Windows device will use for local approval.");

    public string ReviewerHint => ReviewerReadinessState switch
    {
        ApprovalReviewerReadinessState.Ready => _localization.Text(
            "模型选择保存在本机；Agent Prompt 与权限策略均已验证，自动审批可以使用。",
            "The selection is stored locally. Agent Prompt and capability policy are verified, so automatic approval is available."),
        ApprovalReviewerReadinessState.ModelNotSelected => _localization.Text(
            "请选择并保存一个本机审批模型；未选择时自动审批会回退为用户确认。",
            "Select and save a local approval model. Without one, automatic approval falls back to the user."),
        ApprovalReviewerReadinessState.ConnectorNotPaired => _localization.Text(
            "请先配对本机 Connector，之后才能校验模型凭据、Agent Prompt 与权限策略。",
            "Pair the local Connector before validating model credentials, Agent Prompt, and capability policy."),
        _ => _localization.Text(
            "当前模型凭据、Agent Prompt 或权限策略未通过校验，自动审批会安全回退为用户确认。",
            "Model credentials, Agent Prompt, or capability policy did not validate, so automatic approval safely falls back to the user."),
    };

    public async Task OpenAsync(CancellationToken cancellationToken = default) =>
        await LoadAsync(cancellationToken).ConfigureAwait(false);

    public async Task LoadAsync(CancellationToken cancellationToken = default)
    {
        await RunAsync(async token =>
        {
            var settingsTask = _store.LoadAsync(token);
            var modelsTask = _models.FetchAvailableModelsAsync(token);
            var readinessTask = CheckReadinessAsync(token);
            await Task.WhenAll(settingsTask, modelsTask, readinessTask).ConfigureAwait(false);
            var settings = settingsTask.Result.Normalize();
            var items = modelsTask.Result
                .Where(static value => value.TaskEnabled)
                .Select(value => new ConnectorModelOptionViewModel(value, _localization))
                .ToArray();
            await _dispatcher.InvokeAsync(() =>
            {
                AvailableModels.Clear();
                foreach (var item in items) AvailableModels.Add(item);
                ModelRequestMaxRetries = settings.ModelRequestMaxRetries;
                ReviewerReadinessState = readinessTask.Result.State;
                SelectedApprovalModel = items.FirstOrDefault(value => string.Equals(
                    value.Id,
                    settings.CommandApprovalModelConfigId,
                    StringComparison.Ordinal));
                if (settings.CommandApprovalModelConfigId is not null && SelectedApprovalModel is null)
                {
                    ActionMessage = _localization.Text(
                        "之前选择的审批模型已不可用，请重新选择并保存。",
                        "The previously selected approval model is unavailable. Select another model and save.");
                }
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    public async Task SaveAsync(CancellationToken cancellationToken = default)
    {
        await RunAsync(async token =>
        {
            var settings = new ConnectorModelSettings(
                ModelRequestMaxRetries,
                SelectedApprovalModel?.Id).Normalize();
            await _store.SaveAsync(settings, token).ConfigureAwait(false);
            var readiness = await CheckReadinessAsync(token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                ModelRequestMaxRetries = settings.ModelRequestMaxRetries;
                ReviewerReadinessState = readiness.State;
                ActionMessage = _localization.Text(
                    "本机模型设置已保存。",
                    "Local model settings were saved.");
            }, token).ConfigureAwait(false);
        }, cancellationToken).ConfigureAwait(false);
    }

    private Task<ApprovalReviewerReadiness> CheckReadinessAsync(CancellationToken cancellationToken) =>
        _reviewerReadiness?.CheckAsync(cancellationToken) ?? Task.FromResult(
            new ApprovalReviewerReadiness(ApprovalReviewerReadinessState.ManagedConfigurationInvalid));

    private async Task RunAsync(
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken)
    {
        await _operationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = true;
                ErrorMessage = null;
                ActionMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            await operation(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsBusy = false).ConfigureAwait(false);
            _operationGate.Release();
        }
    }
}

public sealed partial class ConnectorModelOptionViewModel : ObservableObject
{
    public ConnectorModelOptionViewModel(
        ConversationModelOption model,
        LocalizationViewModel localization)
    {
        Model = model;
        ApplyLocalization(localization);
    }

    public ConversationModelOption Model { get; }
    public string Id => Model.Id;
    public string DisplayName => Model.DisplayName;
    public string ModelName => Model.ModelName;

    [ObservableProperty]
    private string _detail = string.Empty;

    public void ApplyLocalization(LocalizationViewModel localization)
    {
        Detail = string.IsNullOrWhiteSpace(Model.ThinkingLevel)
            ? Model.ModelName
            : localization.Text(
                $"{Model.ModelName} · 思考级别 {Model.ThinkingLevel}",
                $"{Model.ModelName} · Thinking {Model.ThinkingLevel}");
    }
}
