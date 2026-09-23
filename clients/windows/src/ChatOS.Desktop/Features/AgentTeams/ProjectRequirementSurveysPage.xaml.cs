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
        var answers = new Dictionary<string, List<CheckBox>>(StringComparer.Ordinal);
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
            var boxes = new List<CheckBox>();
            foreach (var option in question.Options)
            {
                var box = new CheckBox { Content = option.Label, Tag = option.Id };
                if (question.Kind == AgentRequirementQuestionKind.SingleChoice)
                {
                    box.Checked += (_, _) =>
                    {
                        foreach (var other in boxes.Where(value => !ReferenceEquals(value, box)))
                            other.IsChecked = false;
                    };
                }
                boxes.Add(box);
                questionPanel.Children.Add(box);
            }
            answers[question.Id] = boxes;
            panel.Children.Add(questionPanel);
        }
        var notes = new TextBox
        {
            Header = "备注（选项未覆盖时补充）",
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MinHeight = 90,
            MaxLength = 16_000,
        };
        panel.Children.Add(notes);

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
                question.IsRequired && answers[question.Id].All(value => value.IsChecked != true));
            if (missing is null) return;
            args.Cancel = true;
            validation.Text = $"请先回答必填问题：{missing.Prompt}";
            validation.Visibility = Visibility.Visible;
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;

        var submission = new AgentRequirementSubmission(survey.Draft.Questions
            .Select(question => new AgentRequirementAnswer(question.Id,
                answers[question.Id].Where(value => value.IsChecked == true)
                    .Select(value => (string)value.Tag).ToArray()))
            .Where(value => value.SelectedOptionIds.Count > 0).ToArray(), notes.Text.Trim());
        await ViewModel.SubmitAsync(survey, submission);
    }
}
