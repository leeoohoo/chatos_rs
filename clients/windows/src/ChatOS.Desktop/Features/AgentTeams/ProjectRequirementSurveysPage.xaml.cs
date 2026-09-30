using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed partial class ProjectRequirementSurveysPage : UserControl
{
    public ProjectRequirementSurveysPage(ProjectRequirementSurveysViewModel viewModel)
    {
        ViewModel = viewModel;
        ViewModel.PropertyChanged += (_, _) => Bindings.Update();
        InitializeComponent();
    }

    public ProjectRequirementSurveysViewModel ViewModel { get; }
    public string Notice => ViewModel.ErrorMessage ?? ViewModel.StatusMessage;
    public bool HasNotice => !string.IsNullOrWhiteSpace(Notice);
    public InfoBarSeverity NoticeSeverity => ViewModel.ErrorMessage is null
        ? InfoBarSeverity.Informational
        : InfoBarSeverity.Error;

    private async void OnRefreshClick(object sender, RoutedEventArgs e) =>
        await ViewModel.RefreshAsync();

    private async void OnFillSurveyClick(object sender, RoutedEventArgs e)
    {
        var item = ViewModel.SelectedSurvey;
        if (item is null || !item.CanSubmit) return;
        var survey = item.Survey;
        var answers = new Dictionary<string, SurveyAnswerEditor>(StringComparer.Ordinal);
        var validation = new TextBlock
        {
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSFailureBrush"],
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var panel = new StackPanel { Spacing = 16, MinWidth = 560, MaxWidth = 760 };
        panel.Children.Add(new TextBlock
        {
            Text = survey.Draft.Purpose,
            TextWrapping = TextWrapping.Wrap,
            Opacity = 0.75,
        });
        panel.Children.Add(validation);
        foreach (var question in survey.Draft.Questions)
        {
            var questionPanel = new StackPanel { Spacing = 6 };
            questionPanel.Children.Add(new TextBlock
            {
                Text = question.IsRequired ? $"{question.Prompt} *" : question.Prompt,
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                TextWrapping = TextWrapping.Wrap,
            });
            var editor = new SurveyAnswerEditor(question.Kind);
            if (question.Kind == AgentRequirementQuestionKind.Text)
            {
                editor.Text = new TextBox
                {
                    AcceptsReturn = true,
                    TextWrapping = TextWrapping.Wrap,
                    MaxLength = 16_000,
                    MinHeight = 72,
                };
                questionPanel.Children.Add(editor.Text);
            }
            else if (question.Kind == AgentRequirementQuestionKind.Boolean)
            {
                editor.Boolean = new ComboBox
                {
                    PlaceholderText = "请选择",
                    ItemsSource = new[] { "是", "否" },
                    MinWidth = 160,
                };
                questionPanel.Children.Add(editor.Boolean);
            }
            else
            {
                foreach (var option in question.Options)
                {
                    var box = new CheckBox { Content = option.Label, Tag = option.Id };
                    if (question.Kind == AgentRequirementQuestionKind.SingleChoice)
                    {
                        box.Checked += (_, _) =>
                        {
                            foreach (var other in editor.Options
                                .Where(value => !ReferenceEquals(value, box)))
                                other.IsChecked = false;
                        };
                    }
                    editor.Options.Add(box);
                    questionPanel.Children.Add(box);
                }
            }
            answers[question.Id] = editor;
            panel.Children.Add(questionPanel);
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = survey.Draft.Title,
            Content = new ScrollViewer
            {
                Content = panel,
                MaxHeight = 620,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            },
            PrimaryButtonText = "提交",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Primary,
        };
        dialog.Closing += (_, args) =>
        {
            if (args.Result != ContentDialogResult.Primary) return;
            var missing = survey.Draft.Questions.FirstOrDefault(question =>
                question.IsRequired && !answers[question.Id].IsAnswered);
            if (missing is null) return;
            args.Cancel = true;
            validation.Text = $"请先回答必填问题：{missing.Prompt}";
            validation.Visibility = Visibility.Visible;
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

        var submission = new AgentRequirementSubmission(survey.Draft.Questions
            .Where(question => answers[question.Id].IsAnswered)
            .Select(question => answers[question.Id].ToAnswer(question.Id))
            .ToArray());
        await ViewModel.SubmitAsync(survey, submission);
    }
}

internal sealed class SurveyAnswerEditor(AgentRequirementQuestionKind kind)
{
    public List<CheckBox> Options { get; } = [];
    public TextBox? Text { get; set; }
    public ComboBox? Boolean { get; set; }

    public bool IsAnswered => kind switch
    {
        AgentRequirementQuestionKind.Text => !string.IsNullOrWhiteSpace(Text?.Text),
        AgentRequirementQuestionKind.Boolean => Boolean?.SelectedIndex is 0 or 1,
        _ => Options.Any(value => value.IsChecked == true),
    };

    public AgentRequirementAnswer ToAnswer(string questionId) => kind switch
    {
        AgentRequirementQuestionKind.Text => new(questionId, [], Text?.Text.Trim() ?? string.Empty),
        AgentRequirementQuestionKind.Boolean => new(
            questionId, [], BooleanValue: Boolean?.SelectedIndex == 0),
        _ => new(questionId, Options.Where(value => value.IsChecked == true)
            .Select(value => (string)value.Tag).ToArray()),
    };
}
