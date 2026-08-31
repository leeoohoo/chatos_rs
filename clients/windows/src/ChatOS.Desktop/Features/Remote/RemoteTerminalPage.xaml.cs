using System.Collections.Specialized;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace ChatOS.Desktop.Features.Remote;

public sealed partial class RemoteTerminalPage : UserControl
{
    public RemoteTerminalPage(RemoteTerminalViewModel viewModel, LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
        ViewModel.Lines.CollectionChanged += OnLinesChanged;
    }

    public RemoteTerminalViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }
    public event EventHandler? CloseRequested;
    public void Open(RemoteConnection connection) { ViewModel.Open(connection); CommandInput.Focus(FocusState.Programmatic); }
    private void OnBackClick(object sender, RoutedEventArgs e) { ViewModel.CancelCommand.Execute(null); CloseRequested?.Invoke(this, EventArgs.Empty); }
    private async void OnCommandKeyDown(object sender, KeyRoutedEventArgs e) { if (e.Key == VirtualKey.Enter && ViewModel.CanSubmit) { e.Handled = true; await ViewModel.SubmitCommand.ExecuteAsync(null); } }
    private void OnLinesChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (ViewModel.Lines.LastOrDefault() is { } line) _ = DispatcherQueue.TryEnqueue(() => OutputList.ScrollIntoView(line));
    }
}
