using System.ComponentModel;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Input;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.System;

namespace ChatOS.Desktop.Features.Notepad;

public sealed partial class NotepadPage : UserControl
{
    public NotepadPage(NotepadViewModel viewModel, LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        UpdateModeColumns();
        RenderPreview();
    }

    public event EventHandler? CloseRequested;

    public NotepadViewModel ViewModel { get; }

    public LocalizationViewModel Localization { get; }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(NotepadViewModel.Content)) RenderPreview();
        if (e.PropertyName == nameof(NotepadViewModel.EditorMode)) UpdateModeColumns();
    }

    private void RenderPreview()
    {
        MarkdownPreview.Blocks.Clear();
        var inCode = false;
        foreach (var line in ViewModel.Content.Replace("\r\n", "\n").Split('\n'))
        {
            if (line.TrimStart().StartsWith("```", StringComparison.Ordinal)) { inCode = !inCode; continue; }
            var paragraph = new Paragraph { Margin = new Thickness(0, 0, 0, 8) };
            var text = line;
            if (inCode) { paragraph.FontFamily = new Microsoft.UI.Xaml.Media.FontFamily("Cascadia Mono, Consolas"); paragraph.FontSize = 12; }
            else if (line.StartsWith("### ")) { paragraph.FontSize = 16; paragraph.FontWeight = FontWeights.SemiBold; text = line[4..]; }
            else if (line.StartsWith("## ")) { paragraph.FontSize = 19; paragraph.FontWeight = FontWeights.SemiBold; text = line[3..]; }
            else if (line.StartsWith("# ")) { paragraph.FontSize = 24; paragraph.FontWeight = FontWeights.Bold; text = line[2..]; }
            else if (line.StartsWith("- ") || line.StartsWith("* ")) { text = "• " + line[2..]; }
            else if (line.StartsWith("> ")) { paragraph.FontStyle = Windows.UI.Text.FontStyle.Italic; text = line[2..]; }
            paragraph.Inlines.Add(new Run { Text = text });
            MarkdownPreview.Blocks.Add(paragraph);
        }
    }

    private void OnCloseClick(object sender, RoutedEventArgs e) => CloseRequested?.Invoke(this, EventArgs.Empty);
    private void OnRootFolderClick(object sender, RoutedEventArgs e) => ViewModel.SelectFolderCommand.Execute(null);
    private void OnFolderClick(object sender, ItemClickEventArgs e) => ViewModel.SelectFolderCommand.Execute(e.ClickedItem as NotepadFolderItem);
    private async void OnNoteClick(object sender, ItemClickEventArgs e) => await ViewModel.SelectNoteCommand.ExecuteAsync(e.ClickedItem as NotepadNote);
    private void OnPreviewModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Preview;
    private void OnEditModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Edit;
    private void OnSplitModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Split;

    private void UpdateModeColumns()
    {
        PreviewColumn.Width = ViewModel.EditorMode == NotepadEditorMode.Edit ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
        EditorColumn.Width = ViewModel.EditorMode == NotepadEditorMode.Preview ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
    }
    private void OnSearchKeyDown(object sender, KeyRoutedEventArgs e) { if (e.Key == VirtualKey.Enter) { ViewModel.SearchCommand.Execute(null); e.Handled = true; } }

    private async void OnCreateFolderClick(object sender, RoutedEventArgs e) => await ShowTextDialogAsync(Localization.NewFolder, Localization.Text("文件夹名称", "Folder name"), value => ViewModel.CreateFolderCommand.ExecuteAsync(value));
    private async void OnCreateNoteClick(object sender, RoutedEventArgs e) => await ShowTextDialogAsync(Localization.NewNote, Localization.Text("笔记标题", "Note title"), value => ViewModel.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest(value)));
    private async void OnRenameFolderClick(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: NotepadFolderItem folder }) await ShowTextDialogAsync(Localization.Text("重命名文件夹", "Rename folder"), Localization.Text("新路径", "New path"), value => ViewModel.RenameFolderCommand.ExecuteAsync(new NotepadFolderRenameRequest(folder.Path, value)), folder.Path);
    }
    private async void OnDeleteFolderClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: NotepadFolderItem folder }) return;
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"删除“{folder.Path}”？", $"Delete “{folder.Path}”?"), Content = Localization.Text("文件夹及其中全部笔记会被删除。", "The folder and all notes in it will be deleted."), PrimaryButtonText = Localization.Delete, CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Close };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.DeleteFolderCommand.ExecuteAsync(folder);
    }

    private async void OnDeleteNoteClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedNote is not { } note) return;
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"删除“{note.Title}”？", $"Delete “{note.Title}”?"), Content = Localization.Text("这篇笔记会被永久删除。", "This note will be permanently deleted."), PrimaryButtonText = Localization.Delete, CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Close };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.DeleteNoteCommand.ExecuteAsync(note);
    }

    private async Task ShowTextDialogAsync(string title, string placeholder, Func<string, Task> action, string initial = "")
    {
        var input = new TextBox { Text = initial, PlaceholderText = placeholder };
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = title, Content = input, PrimaryButtonText = Localization.Confirm, CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Primary };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await action(input.Text);
    }

    private async void OnExportClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedNote is null) return;
        var picker = new FileSavePicker { SuggestedFileName = SafeFileName(ViewModel.Title) };
        picker.FileTypeChoices.Add("Markdown", [".md"]);
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSaveFileAsync();
        if (file is not null) await FileIO.WriteTextAsync(file, ViewModel.Content);
    }

    private string SafeFileName(string value)
    {
        var invalid = Path.GetInvalidFileNameChars();
        var name = new string(value.Trim().Select(character => invalid.Contains(character) ? '_' : character).ToArray());
        return string.IsNullOrWhiteSpace(name) ? Localization.Text("未命名笔记", "Untitled note") : name;
    }
}
