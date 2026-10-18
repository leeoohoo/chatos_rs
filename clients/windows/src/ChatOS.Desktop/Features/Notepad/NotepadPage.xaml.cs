using System.ComponentModel;
using ChatOS.Core.Domain;
using ChatOS.Desktop.DesignSystem;
using ChatOS.Presentation.Notepad;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.System;
using WindowsClipboard = Windows.ApplicationModel.DataTransfer.Clipboard;
using WindowsDataPackage = Windows.ApplicationModel.DataTransfer.DataPackage;

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
        => MarkdownRichTextRenderer.Render(MarkdownPreview, ViewModel.Content);

    private string FormatUnsavedChanges(InterfaceLanguage language) => Localization.Text("未保存", "Unsaved");

    private async void OnSaveAcceleratorInvoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        if (ViewModel.SaveCommand.CanExecute(null)) await ViewModel.SaveCommand.ExecuteAsync(null);
    }

    private string FormatEmptyNotesTitle(bool searchActive, InterfaceLanguage language) => searchActive
        ? Localization.Text("没有匹配的笔记", "No matching notes")
        : Localization.Text("暂无笔记", "No notes yet");

    private string FormatEmptyNotesDescription(bool searchActive, InterfaceLanguage language) => searchActive
        ? Localization.Text("尝试其他关键词，或清空搜索。", "Try another keyword, or clear the search.")
        : Localization.Text("创建文件夹或笔记开始记录。", "Create a folder or note to start writing.");

    private async void OnCloseClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.IsSaving) return;

        ViewModel.CancelSearchCommand.Execute(null);
        if (!ViewModel.IsDirty)
        {
            CloseRequested?.Invoke(this, EventArgs.Empty);
            return;
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text("保存更改？", "Save changes?"),
            Content = Localization.Text(
                "当前笔记有未保存的修改。关闭前是否保存？",
                "This note has unsaved changes. Save them before closing?"),
            PrimaryButtonText = Localization.Save,
            SecondaryButtonText = Localization.Text("放弃", "Discard"),
            CloseButtonText = Localization.Cancel,
            DefaultButton = ContentDialogButton.Primary,
        };

        var result = await dialog.ShowAsync();
        if (result == ContentDialogResult.None)
        {
            return;
        }

        if (result == ContentDialogResult.Primary)
        {
            await ViewModel.SaveCommand.ExecuteAsync(null);
            if (ViewModel.IsDirty)
            {
                return;
            }
        }
        else if (result == ContentDialogResult.Secondary)
        {
            ViewModel.DiscardChangesCommand.Execute(null);
            if (ViewModel.IsDirty) return;
        }

        CloseRequested?.Invoke(this, EventArgs.Empty);
    }
    private void OnRootFolderClick(object sender, RoutedEventArgs e)
    {
        ViewModel.SelectFolderCommand.Execute(null);
        // Clicking the current root must not toggle its location indicator off.
        if (sender is Microsoft.UI.Xaml.Controls.Primitives.ToggleButton root) root.IsChecked = true;
    }
    private void OnFolderClick(object sender, ItemClickEventArgs e)
    {
        ViewModel.SelectFolderCommand.Execute(e.ClickedItem as NotepadFolderItem);
        if (sender is ListView folders) folders.SelectedItem = ViewModel.SelectedFolderItem;
    }
    private void OnFolderSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        // Keyboard navigation also changes folders. Ignore selection removal
        // while refresh replaces items; the model retains the current path.
        if (e.AddedItems.FirstOrDefault() is NotepadFolderItem folder) ViewModel.SelectFolderCommand.Execute(folder);
    }
    private async void OnNoteClick(object sender, ItemClickEventArgs e)
    {
        await ViewModel.SelectNoteCommand.ExecuteAsync(e.ClickedItem as NotepadNote);
        if (sender is ListView notes)
        {
            // ItemClick precedes the native selection update. A synchronous save
            // rejection must restore the editor's note after that update finishes.
            notes.DispatcherQueue.TryEnqueue(() => notes.SelectedItem = ViewModel.SelectedResultNote);
        }
    }
    private void OnPreviewModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Preview;
    private void OnEditModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Edit;
    private void OnSplitModeClick(object sender, RoutedEventArgs e) => ViewModel.EditorMode = NotepadEditorMode.Split;

    private void OnCopyClick(object sender, RoutedEventArgs e)
    {
        if (!ViewModel.HasSelection)
        {
            return;
        }

        try
        {
            var package = new WindowsDataPackage();
            package.SetText(ViewModel.Content);
            WindowsClipboard.SetContent(package);
            WindowsClipboard.Flush();
            ViewModel.ActionMessage = Localization.Text("笔记内容已复制。", "Note content copied.");
            ViewModel.ErrorMessage = null;
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = Localization.Text(
                $"复制失败：{exception.Message}",
                $"Copy failed: {exception.Message}");
        }
    }

    private void UpdateModeColumns()
    {
        PreviewColumn.Width = ViewModel.EditorMode == NotepadEditorMode.Edit ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
        EditorColumn.Width = ViewModel.EditorMode == NotepadEditorMode.Preview ? new GridLength(0) : new GridLength(1, GridUnitType.Star);
        BodyPanes.ColumnSpacing = ViewModel.IsSplitMode ? 12 : 0;
        EditModeButton.IsChecked = ViewModel.EditorMode == NotepadEditorMode.Edit;
        PreviewModeButton.IsChecked = ViewModel.EditorMode == NotepadEditorMode.Preview;
        SplitModeButton.IsChecked = ViewModel.EditorMode == NotepadEditorMode.Split;
    }
    private void OnSearchKeyDown(object sender, KeyRoutedEventArgs e) { if (e.Key == VirtualKey.Enter) { ViewModel.SearchCommand.Execute(null); e.Handled = true; } }

    private async void OnCreateFolderClick(object sender, RoutedEventArgs e) => await ShowTextDialogAsync(Localization.NewFolder, Localization.Text("文件夹名称", "Folder name"), value => ViewModel.CreateFolderCommand.ExecuteAsync(value));
    private async void OnCreateNoteClick(object sender, RoutedEventArgs e)
    {
        if (!ViewModel.CanReload) return;
        await ShowTextDialogAsync(Localization.NewNote, Localization.Text("笔记标题", "Note title"), value => ViewModel.CreateNoteCommand.ExecuteAsync(new NotepadNoteCreationRequest(value)));
    }
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
