using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Projects;

public sealed partial class ProjectPlanPage : UserControl
{
    public ProjectPlanPage(ProjectPlanViewModel viewModel, LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
    }

    public ProjectPlanViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }

    private void OnRequirementItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is ProjectRequirementItemViewModel requirement)
        {
            ViewModel.SelectRequirementCommand.Execute(requirement);
        }
    }

    private async void OnStopExecutionClick(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = ViewModel.ExecutionStopLabel == Localization.Text("停止执行", "Stop execution")
                ? Localization.Text("确定停止这批任务？", "Stop these tasks?")
                : Localization.Text("确定放弃这份执行计划？", "Discard this execution plan?"),
            Content = Localization.Text(
                "尚未继续运行的任务节点会被清理。停止后仍可回到需求重新生成执行计划。",
                "Task nodes that have not continued will be cleared. You can return to the requirement and create another execution plan."),
            PrimaryButtonText = ViewModel.ExecutionStopLabel,
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            ViewModel.StopExecutionCommand.Execute(null);
        }
    }
}
