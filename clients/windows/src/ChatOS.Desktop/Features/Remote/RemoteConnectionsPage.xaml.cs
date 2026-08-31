using System.ComponentModel;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.Remote;

public sealed partial class RemoteConnectionsPage : UserControl
{
    private bool _syncing;

    public RemoteConnectionsPage(
        RemoteConnectionsViewModel viewModel,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Localization = localization;
        InitializeComponent();
        Loaded += OnLoaded;
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
    }

    public RemoteConnectionsViewModel ViewModel { get; }
    public LocalizationViewModel Localization { get; }
    public event EventHandler<RemoteConnection>? OpenSftpRequested;
    public event EventHandler<RemoteConnection>? OpenTerminalRequested;

    private async void OnLoaded(object sender, RoutedEventArgs e) { await ViewModel.OpenAsync(); SyncControls(); }
    private async void OnRefreshClick(object sender, RoutedEventArgs e) => await ViewModel.OpenAsync();
    private void OnOpenSftpClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConnection is { } connection) OpenSftpRequested?.Invoke(this, connection);
    }
    private void OnOpenTerminalClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConnection is { } connection) OpenTerminalRequested?.Invoke(this, connection);
    }
    private void OnConnectionClick(object sender, ItemClickEventArgs e) { ViewModel.EditCommand.Execute(e.ClickedItem as RemoteConnection); SyncControls(); }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(RemoteConnectionsViewModel.SelectedConnection) or nameof(RemoteConnectionsViewModel.AuthenticationType) or nameof(RemoteConnectionsViewModel.HostKeyPolicy) or nameof(RemoteConnectionsViewModel.JumpEnabled)) SyncControls();
    }

    private void SyncControls()
    {
        if (AuthenticationPicker is null) return;
        _syncing = true;
        AuthenticationPicker.SelectedIndex = ViewModel.AuthenticationType switch { RemoteAuthenticationType.PrivateKeyCertificate => 1, RemoteAuthenticationType.Password => 2, _ => 0 };
        HostKeyPicker.SelectedIndex = ViewModel.HostKeyPolicy == RemoteHostKeyPolicy.AcceptNew ? 1 : 0;
        PasswordInput.Password = string.Empty;
        JumpPasswordInput.Password = string.Empty;
        KeyFields.Visibility = ViewModel.AuthenticationType == RemoteAuthenticationType.Password ? Visibility.Collapsed : Visibility.Visible;
        JumpFields.Visibility = ViewModel.JumpEnabled ? Visibility.Visible : Visibility.Collapsed;
        JumpConnectionPicker.SelectedItem = ViewModel.Connections.FirstOrDefault(value => value.Id == ViewModel.JumpConnectionId);
        _syncing = false;
    }

    private void OnAuthenticationChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || AuthenticationPicker.SelectedItem is not ComboBoxItem item || !Enum.TryParse<RemoteAuthenticationType>(item.Tag?.ToString(), out var value)) return;
        ViewModel.AuthenticationType = value;
    }

    private void OnHostKeyPolicyChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || HostKeyPicker.SelectedItem is not ComboBoxItem item || !Enum.TryParse<RemoteHostKeyPolicy>(item.Tag?.ToString(), out var value)) return;
        ViewModel.HostKeyPolicy = value;
    }

    private void OnPasswordChanged(object sender, RoutedEventArgs e) { if (!_syncing) ViewModel.Password = PasswordInput.Password; }
    private void OnJumpPasswordChanged(object sender, RoutedEventArgs e) { if (!_syncing) ViewModel.JumpPassword = JumpPasswordInput.Password; }
    private void OnJumpToggled(object sender, RoutedEventArgs e) => JumpFields.Visibility = JumpSwitch.IsOn ? Visibility.Visible : Visibility.Collapsed;
    private void OnJumpConnectionChanged(object sender, SelectionChangedEventArgs e) { if (!_syncing) ViewModel.JumpConnectionId = (JumpConnectionPicker.SelectedItem as RemoteConnection)?.Id; }

    private async void OnPickKeyClick(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string target }) return;
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add("*");
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        WinRT.Interop.InitializeWithWindow.Initialize(picker, WinRT.Interop.WindowNative.GetWindowHandle(window));
        var file = await picker.PickSingleFileAsync();
        if (file is null) return;
        if (target == "private") ViewModel.PrivateKeyPath = file.Path;
        else if (target == "certificate") ViewModel.CertificatePath = file.Path;
        else if (target == "jump-private") ViewModel.JumpPrivateKeyPath = file.Path;
        else ViewModel.JumpCertificatePath = file.Path;
    }

    private async void OnDeleteClick(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedConnection is not { } connection) return;
        var dialog = new ContentDialog { XamlRoot = XamlRoot, Title = Localization.Text($"删除“{connection.Name}”？", $"Delete “{connection.Name}”?"), Content = Localization.Text("云端连接记录和本机保存的 SSH 凭据都会删除。", "The cloud connection record and locally stored SSH credentials will both be deleted."), PrimaryButtonText = Localization.Delete, CloseButtonText = Localization.Cancel, DefaultButton = ContentDialogButton.Close };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary) await ViewModel.DeleteCommand.ExecuteAsync(connection);
    }
}
