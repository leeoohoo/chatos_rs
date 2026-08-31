using System.Collections.ObjectModel;
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

    public AskUserPromptViewModel(
        AskUserPrompt prompt,
        IAskUserPromptService service,
        Func<Task> onChanged,
        LocalizationViewModel? localization = null)
    {
        Prompt = prompt;
        _service = service;
        _onChanged = onChanged;
        _localization = localization;
        Fields = new ObservableCollection<AskUserFieldInputViewModel>(
            prompt.Fields.Select(static field => new AskUserFieldInputViewModel(field)));
        Options = new ObservableCollection<AskUserChoiceOptionViewModel>(
            (prompt.Choice?.Options ?? Array.Empty<AskUserChoiceOption>())
            .Select(option => new AskUserChoiceOptionViewModel(
                option,
                prompt.Choice?.DefaultSelection.Contains(option.Value, StringComparer.Ordinal) == true)));
        SelectedSingleOption = Options.FirstOrDefault(static option => option.IsSelected);
    }

    public AskUserPrompt Prompt { get; }

    public string Id => Prompt.Id;

    public string Title => string.IsNullOrWhiteSpace(Prompt.Title) ? L("需要你的输入", "Your input is needed") : Prompt.Title;

    public string IgnoreLabel => L("忽略", "Ignore");

    public string CancelLabel => L("取消", "Cancel");

    public string SubmitLabel => L("提交", "Submit");

    public string Message => Prompt.Message;

    public bool AllowsCancel => Prompt.AllowsCancel;

    public bool HasFields => Fields.Count > 0;

    public bool HasChoice => Options.Count > 0;

    public bool AllowsMultiple => Prompt.Choice?.AllowsMultiple == true;

    public bool IsSingleChoice => HasChoice && !AllowsMultiple;

    public ObservableCollection<AskUserFieldInputViewModel> Fields { get; }

    public ObservableCollection<AskUserChoiceOptionViewModel> Options { get; }

    [ObservableProperty]
    private AskUserChoiceOptionViewModel? _selectedSingleOption;

    [ObservableProperty]
    private bool _isSubmitting;

    [ObservableProperty]
    private string? _errorMessage;

    [RelayCommand]
    private async Task SubmitAsync()
    {
        if (IsSubmitting)
        {
            return;
        }

        var missing = Fields.FirstOrDefault(field =>
            field.IsRequired && string.IsNullOrWhiteSpace(field.Value));
        if (missing is not null)
        {
            ErrorMessage = L($"请填写“{missing.Label}”。", $"Complete “{missing.Label}”.");
            return;
        }

        AskUserSelection? selection = null;
        if (AllowsMultiple)
        {
            var values = Options.Where(static option => option.IsSelected)
                .Select(static option => option.Value)
                .ToArray();
            var minimum = Prompt.Choice?.MinimumSelectionCount ?? 0;
            var maximum = Prompt.Choice?.MaximumSelectionCount ?? Options.Count;
            if (values.Length < minimum || values.Length > maximum)
            {
                ErrorMessage = L($"请选择 {minimum}–{maximum} 项。", $"Select {minimum}–{maximum} options.");
                return;
            }

            selection = new AskUserSelection.Multiple(values);
        }
        else if (HasChoice)
        {
            if (SelectedSingleOption is null)
            {
                ErrorMessage = L("请选择一个选项。", "Select an option.");
                return;
            }

            selection = new AskUserSelection.Single(SelectedSingleOption.Value);
        }

        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            await _service.SubmitAsync(
                Prompt.Id,
                Prompt.ConversationId,
                new AskUserSubmission(
                    Fields.ToDictionary(static field => field.Key, static field => field.Value),
                    selection));
            await _onChanged();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsSubmitting = false;
        }
    }

    [RelayCommand]
    private async Task CancelAsync()
    {
        if (IsSubmitting || !AllowsCancel)
        {
            return;
        }

        IsSubmitting = true;
        ErrorMessage = null;
        try
        {
            await _service.CancelAsync(Prompt.Id, Prompt.ConversationId);
            await _onChanged();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsSubmitting = false;
        }
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;
}
