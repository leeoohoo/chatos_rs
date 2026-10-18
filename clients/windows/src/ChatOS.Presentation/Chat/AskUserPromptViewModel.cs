using System.Collections.ObjectModel;
using System.ComponentModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class AskUserPromptViewModel : ObservableObject
{
    private readonly IAskUserPromptService _service;
    private readonly Func<Task> _onChanged;
    private readonly LocalizationViewModel? _localization;
    private readonly Action? _onRequestStarted;
    private bool _isActive = true;

    public AskUserPromptViewModel(
        AskUserPrompt prompt,
        IAskUserPromptService service,
        Func<Task> onChanged,
        LocalizationViewModel? localization = null,
        Action? onRequestStarted = null)
    {
        Prompt = prompt;
        _service = service;
        _onChanged = onChanged;
        _localization = localization;
        _onRequestStarted = onRequestStarted;
        Fields = new ObservableCollection<AskUserFieldInputViewModel>(
            prompt.Fields.Select(static field => new AskUserFieldInputViewModel(field)));
        Options = new ObservableCollection<AskUserChoiceOptionViewModel>(
            (prompt.Choice?.Options ?? Array.Empty<AskUserChoiceOption>())
            .Select(option => new AskUserChoiceOptionViewModel(
                option,
                prompt.Choice?.DefaultSelection.Contains(option.Value, StringComparer.Ordinal) == true)));
        SelectedSingleOption = Options.FirstOrDefault(static option => option.IsSelected);
        foreach (var field in Fields) field.PropertyChanged += OnFieldChanged;
        foreach (var option in Options) option.PropertyChanged += OnOptionChanged;
    }

    public AskUserPrompt Prompt { get; private set; }

    public string Id => Prompt.Id;

    public string PromptAutomationId => $"ChatOS.Chat.AskUser.{Id}";

    public string Title => string.IsNullOrWhiteSpace(Prompt.Title) ? L("需要你的输入", "Your input is needed") : Prompt.Title;

    public string IgnoreLabel => L("忽略", "Ignore");

    public string WaitingLabel => L("任务正在等待你的答复", "The task is waiting for your response");

    public string PendingLabel => L("待处理", "Pending");

    public string ChoiceLabel => AllowsMultiple
        ? L("请选择（可多选）", "Select one or more")
        : L("请选择", "Select one");

    public string ChoiceHint => Prompt.Choice is not { } choice || choice.MinimumSelectionCount == 0
        ? string.Empty
        : choice.MinimumSelectionCount == choice.MaximumSelectionCount
            ? L($"选择 {choice.MinimumSelectionCount} 项", $"Select {choice.MinimumSelectionCount}")
            : L($"选择 {choice.MinimumSelectionCount}–{choice.MaximumSelectionCount} 项",
                $"Select {choice.MinimumSelectionCount}–{choice.MaximumSelectionCount} options");

    public string CancelLabel => L("取消请求", "Cancel Request");

    public string SubmitLabel => L("确认提交", "Submit");

    public string Message => Prompt.Message;

    public bool AllowsCancel => Prompt.AllowsCancel;

    public bool HasFields => Fields.Count > 0;

    public bool HasChoice => Options.Count > 0;

    public bool AllowsMultiple => Prompt.Choice?.AllowsMultiple == true;

    public bool IsSingleChoice => HasChoice && !AllowsMultiple;

    public bool CanSubmit => _isActive && Prompt.IsPending && !IsSubmitting && ValidationError() is null;

    public bool CanCancel => _isActive && Prompt.IsPending && AllowsCancel && !IsSubmitting;

    public ObservableCollection<AskUserFieldInputViewModel> Fields { get; }

    public ObservableCollection<AskUserChoiceOptionViewModel> Options { get; }

    [ObservableProperty]
    private AskUserChoiceOptionViewModel? _selectedSingleOption;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSubmit))]
    [NotifyPropertyChangedFor(nameof(CanCancel))]
    [NotifyCanExecuteChangedFor(nameof(SubmitCommand))]
    [NotifyCanExecuteChangedFor(nameof(CancelCommand))]
    private bool _isSubmitting;

    [ObservableProperty]
    private string? _errorMessage;

    partial void OnSelectedSingleOptionChanged(AskUserChoiceOptionViewModel? value) => NotifyActionAvailability();

    private void OnFieldChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(AskUserFieldInputViewModel.Value)) NotifyActionAvailability();
    }

    private void OnOptionChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(AskUserChoiceOptionViewModel.IsSelected)) return;
        if (AllowsMultiple && sender is AskUserChoiceOptionViewModel { IsSelected: true } option &&
            Options.Count(static value => value.IsSelected) > Prompt.Choice!.MaximumSelectionCount)
        {
            option.IsSelected = false;
            return;
        }

        NotifyActionAvailability();
    }

    private void NotifyActionAvailability()
    {
        OnPropertyChanged(nameof(CanSubmit));
        OnPropertyChanged(nameof(CanCancel));
        SubmitCommand.NotifyCanExecuteChanged();
        CancelCommand.NotifyCanExecuteChanged();
    }

    private string? ValidationError()
    {
        var missing = Fields.FirstOrDefault(field => field.IsRequired && string.IsNullOrWhiteSpace(field.Value));
        if (missing is not null) return L($"请填写“{missing.Label}”。", $"Complete “{missing.Label}”.");
        if (Prompt.Choice is not { } choice) return null;

        var count = AllowsMultiple
            ? Options.Count(static option => option.IsSelected)
            : SelectedSingleOption is null ? 0 : 1;
        if (count >= choice.MinimumSelectionCount && count <= choice.MaximumSelectionCount) return null;
        return !AllowsMultiple && count == 0
            ? L("请选择一个选项。", "Select an option.")
            : L($"请选择 {choice.MinimumSelectionCount}–{choice.MaximumSelectionCount} 项。",
                $"Select {choice.MinimumSelectionCount}–{choice.MaximumSelectionCount} options.");
    }

    private void ApplyResponse(AskUserPrompt response)
    {
        Prompt = response;
        OnPropertyChanged(nameof(Prompt));
        if (!response.IsPending)
        {
            foreach (var field in Fields.Where(static field => field.IsSecret)) field.Value = string.Empty;
        }

        NotifyActionAvailability();
    }

    internal bool HasSameInputs(AskUserPrompt prompt) =>
        Prompt.ConversationId == prompt.ConversationId && Prompt.TurnId == prompt.TurnId &&
        Prompt.ToolCallId == prompt.ToolCallId && Prompt.Kind == prompt.Kind &&
        Prompt.Fields.SequenceEqual(prompt.Fields) && SameChoice(Prompt.Choice, prompt.Choice);

    private static bool SameChoice(AskUserChoice? left, AskUserChoice? right) =>
        left is null ? right is null : right is not null &&
        left.AllowsMultiple == right.AllowsMultiple &&
        left.MinimumSelectionCount == right.MinimumSelectionCount &&
        left.MaximumSelectionCount == right.MaximumSelectionCount &&
        left.Options.SequenceEqual(right.Options) &&
        left.DefaultSelection.SequenceEqual(right.DefaultSelection);

    internal void Refresh(AskUserPrompt prompt)
    {
        ApplyResponse(prompt);
        OnPropertyChanged(nameof(Title));
        OnPropertyChanged(nameof(Message));
        OnPropertyChanged(nameof(AllowsCancel));
    }

    internal void Deactivate()
    {
        _isActive = false;
        foreach (var field in Fields.Where(static field => field.IsSecret)) field.Value = string.Empty;
        NotifyActionAvailability();
    }

    [RelayCommand(CanExecute = nameof(CanSubmit))]
    private async Task SubmitAsync()
    {
        if (!_isActive || IsSubmitting || !Prompt.IsPending)
        {
            return;
        }

        if (ValidationError() is { } validationError)
        {
            ErrorMessage = validationError;
            return;
        }

        AskUserSelection? selection = null;
        if (AllowsMultiple)
        {
            var values = Options.Where(static option => option.IsSelected)
                .Select(static option => option.Value)
                .ToArray();
            selection = new AskUserSelection.Multiple(values);
        }
        else if (Prompt.Choice is not null)
        {
            selection = new AskUserSelection.Single(SelectedSingleOption?.Value ?? string.Empty);
        }

        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            _onRequestStarted?.Invoke();
            var response = await _service.SubmitAsync(
                Prompt.Id,
                Prompt.ConversationId,
                new AskUserSubmission(
                    Fields.ToDictionary(static field => field.Key, static field => field.Value),
                    selection));
            if (!_isActive) return;
            ApplyResponse(response);
            await _onChanged();
        }
        catch (Exception exception)
        {
            if (_isActive) ErrorMessage = exception.Message;
        }
        finally
        {
            IsSubmitting = false;
        }
    }

    [RelayCommand(CanExecute = nameof(CanCancel))]
    private async Task CancelAsync()
    {
        if (!CanCancel)
        {
            return;
        }

        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            _onRequestStarted?.Invoke();
            var response = await _service.CancelAsync(Prompt.Id, Prompt.ConversationId);
            if (!_isActive) return;
            ApplyResponse(response);
            await _onChanged();
        }
        catch (Exception exception)
        {
            if (_isActive) ErrorMessage = exception.Message;
        }
        finally
        {
            IsSubmitting = false;
        }
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
}
