using System.ComponentModel;
using System.Runtime.InteropServices;
using ChatOS.Connector.Approval;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Pet;
using Microsoft.UI.Input;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.Foundation;
using Windows.Graphics;

namespace ChatOS.Desktop.Features.Pet;

public sealed partial class PetWindow : Window
{
    private const int CollapsedWidth = 230;
    private const int CollapsedHeight = 190;
    private const int SummaryWidth = 360;
    private const int SummaryHeight = 315;
    private const int ApprovalWidth = 430;
    private const int ApprovalHeight = 520;
    private const int ExpandedWidth = 430;
    private const int ExpandedHeight = 650;
    private const int GwlExStyle = -20;
    private const long WsExLayered = 0x00080000L;
    private const long WsExToolWindow = 0x00000080L;
    private const uint LwaColorKey = 0x00000001;
    private readonly CommandApprovalCoordinator _approvals;
    private readonly IPetWindowPlacementStore _placementStore;
    private readonly DispatcherTimer _animationTimer = new() { Interval = TimeSpan.FromMilliseconds(90) };
    private readonly DispatcherTimer _decisionTimer = new() { Interval = TimeSpan.FromSeconds(5) };
    private ConnectorPendingApproval? _activeApproval;
    private ConnectorApprovalDecisionEventArgs? _transientDecision;
    private bool _isVisible;
    private bool _isDragging;
    private bool _dragMoved;
    private PointInt32 _dragStartCursor;
    private PointInt32 _dragStartWindow;
    private bool _animationPhase;

    public PetWindow(
        PetOverlayViewModel viewModel,
        PetQuickChatViewModel quickChat,
        CommandApprovalCoordinator approvals,
        IPetWindowPlacementStore placementStore)
    {
        ViewModel = viewModel;
        QuickChat = quickChat;
        _approvals = approvals;
        _placementStore = placementStore;
        InitializeComponent();
        ConfigureNativeWindow();
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ViewModel.Activities.CollectionChanged += (_, _) => UpdateVisualState();
        QuickChat.PropertyChanged += OnQuickChatPropertyChanged;
        QuickChat.Conversation.PropertyChanged += OnQuickChatPropertyChanged;
        QuickChat.Conversation.Turns.CollectionChanged += (_, _) => UpdateVisualState();
        _approvals.PendingChanged += OnPendingApprovalsChanged;
        _approvals.DecisionRecorded += OnApprovalDecisionRecorded;
        _animationTimer.Tick += OnAnimationTick;
        _decisionTimer.Tick += OnDecisionTimerTick;
        Closed += OnClosed;
        UpdateVisualState();
    }

    public PetOverlayViewModel ViewModel { get; }

    public PetQuickChatViewModel QuickChat { get; }

    public async Task ShowAsync(CancellationToken cancellationToken = default)
    {
        if (_isVisible) return;
        Activate();
        _isVisible = true;
        try
        {
            await RestorePositionAsync(cancellationToken);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            PositionInitially();
            ViewModel.ErrorMessage = exception.Message;
        }

        await ViewModel.StartAsync(cancellationToken);
        RefreshApproval();
        UpdateVisualState();
    }

    public async Task HidePetAsync()
    {
        ViewModel.Stop();
        await QuickChat.CloseAsync();
        if (!_isVisible) return;
        AppWindow.Hide();
        _isVisible = false;
    }

    private void ConfigureNativeWindow()
    {
        AppWindow.Title = "ChatOS Pet";
        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.SetBorderAndTitleBar(false, false);
            presenter.IsAlwaysOnTop = true;
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = false;
        }

        AppWindow.Resize(new SizeInt32(CollapsedWidth, CollapsedHeight));
        var hwnd = WinRT.Interop.WindowNative.GetWindowHandle(this);
        var style = GetWindowLongPtr(hwnd, GwlExStyle).ToInt64();
        SetWindowLongPtr(hwnd, GwlExStyle, new IntPtr(style | WsExLayered | WsExToolWindow));
        if (!SetLayeredWindowAttributes(hwnd, 0, 0, LwaColorKey))
        {
            throw new InvalidOperationException("Unable to configure the transparent pet window.");
        }
    }

    private void PositionInitially()
    {
        var display = DisplayArea.GetFromWindowId(AppWindow.Id, DisplayAreaFallback.Primary);
        var work = display.WorkArea;
        AppWindow.Move(new PointInt32(
            work.X + work.Width - AppWindow.Size.Width - 28,
            work.Y + work.Height - AppWindow.Size.Height - 24));
    }

    private async Task RestorePositionAsync(CancellationToken cancellationToken)
    {
        var saved = await _placementStore.LoadAsync(cancellationToken);
        if (saved is null)
        {
            PositionInitially();
            return;
        }

        MoveIntoWorkArea(new PointInt32(
            saved.AnchorX - AppWindow.Size.Width,
            saved.AnchorY - AppWindow.Size.Height));
    }

    private void MoveIntoWorkArea(PointInt32 position)
    {
        var center = new PointInt32(
            position.X + AppWindow.Size.Width / 2,
            position.Y + AppWindow.Size.Height / 2);
        var work = DisplayArea.GetFromPoint(center, DisplayAreaFallback.Nearest).WorkArea;
        position.X = Math.Clamp(position.X, work.X, work.X + Math.Max(0, work.Width - AppWindow.Size.Width));
        position.Y = Math.Clamp(position.Y, work.Y, work.Y + Math.Max(0, work.Height - AppWindow.Size.Height));
        AppWindow.Move(position);
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        _ = DispatcherQueue.TryEnqueue(UpdateVisualState);
    }

    private void OnPendingApprovalsChanged(object? sender, EventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(RefreshApproval);

    private void OnApprovalDecisionRecorded(
        object? sender,
        ConnectorApprovalDecisionEventArgs e)
    {
        if (e.Outcome.Reviewer is not (
            ConnectorApprovalReviewer.Ai or
            ConnectorApprovalReviewer.Policy or
            ConnectorApprovalReviewer.Session))
        {
            return;
        }

        _ = DispatcherQueue.TryEnqueue(() => ShowTransientDecision(e));
    }

    private void ShowTransientDecision(ConnectorApprovalDecisionEventArgs decision)
    {
        _transientDecision = decision;
        DecisionTitleText.Text = decision.Outcome.Approved
            ? ViewModel.Localization.OperationAllowed
            : ViewModel.Localization.OperationDenied;
        DecisionSourceText.Text = decision.Outcome.Reviewer switch
        {
            ConnectorApprovalReviewer.Ai => ViewModel.Localization.AiApproval,
            ConnectorApprovalReviewer.Session => ViewModel.Localization.SessionAuthorization,
            _ => ViewModel.Localization.ApprovalPolicy,
        };
        DecisionCommandText.Text = decision.Request.DisplayCommand;
        DecisionReasonText.Text = decision.Outcome.Reason;
        DecisionStatusText.Text = decision.Outcome.Approved
            ? ViewModel.Localization.ExecutionContinued
            : ViewModel.Localization.ExecutionStopped;
        DecisionStatusText.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources[
            decision.Outcome.Approved ? "ChatOSSuccessBrush" : "ChatOSFailureBrush"];
        _decisionTimer.Stop();
        _decisionTimer.Start();
        UpdateVisualState();
    }

    private void OnDecisionTimerTick(object? sender, object e)
    {
        _decisionTimer.Stop();
        _transientDecision = null;
        UpdateVisualState();
    }

    private void RefreshApproval()
    {
        var pending = _approvals.Snapshot();
        _activeApproval = pending.FirstOrDefault();
        if (_activeApproval is { } approval)
        {
            ApprovalSourceText.Text = $"{approval.Source} · {approval.CreatedAt.ToLocalTime():HH:mm:ss}";
            ApprovalCommandText.Text = approval.Command;
            ApprovalWorkingDirectoryText.Text = approval.WorkingDirectory;
            ApprovalReasonText.Text = approval.Reason ?? approval.Risk.Reason ?? ViewModel.Localization.DefaultApprovalReason;
            ApprovalQueueText.Text = pending.Count > 1
                ? ViewModel.Localization.AdditionalQueuedApprovals(pending.Count - 1)
                : ViewModel.Localization.ApprovalTaskOutcome;
            ApprovalRiskText.Text = approval.Risk.Level switch
            {
                ConnectorApprovalRiskLevel.High => ViewModel.Localization.HighRisk,
                ConnectorApprovalRiskLevel.Medium => ViewModel.Localization.MediumRisk,
                _ => ViewModel.Localization.LowRisk,
            };
        }

        UpdateVisualState();
    }

    private void UpdateVisualState()
    {
        if (WindowRoot is null) return;
        var approvalVisible = _activeApproval is not null;
        var decisionVisible = _transientDecision is not null && !approvalVisible;
        var quickChatVisible = QuickChat.IsOpen && !approvalVisible && !decisionVisible;
        var expanded = ViewModel.IsExpanded && !approvalVisible && !decisionVisible && !quickChatVisible;
        var primary = ViewModel.Activities.FirstOrDefault();
        var summaryVisible = !expanded && !quickChatVisible && !approvalVisible && !decisionVisible && primary is not null;

        ApprovalCard.Visibility = approvalVisible ? Visibility.Visible : Visibility.Collapsed;
        DecisionCard.Visibility = decisionVisible ? Visibility.Visible : Visibility.Collapsed;
        QuickChatCard.Visibility = quickChatVisible ? Visibility.Visible : Visibility.Collapsed;
        InboxCard.Visibility = expanded ? Visibility.Visible : Visibility.Collapsed;
        CompactActivityCard.Visibility = summaryVisible ? Visibility.Visible : Visibility.Collapsed;
        ActivityDetailCard.Visibility = expanded && ViewModel.IsDetailOpen
            ? Visibility.Visible
            : Visibility.Collapsed;
        ActivityList.Visibility = expanded && !ViewModel.IsDetailOpen
            ? Visibility.Visible
            : Visibility.Collapsed;
        EmptyInbox.Visibility = expanded && !ViewModel.IsDetailOpen && !ViewModel.HasActivities
            ? Visibility.Visible
            : Visibility.Collapsed;
        InboxCountBadge.Visibility = ViewModel.HasActivities ? Visibility.Visible : Visibility.Collapsed;
        InboxCountText.Text = ViewModel.Activities.Count.ToString();
        QuickChatNotificationDot.Visibility = ViewModel.HasActivities ? Visibility.Visible : Visibility.Collapsed;
        QuickChatBackButton.Visibility = QuickChat.HasSelectedResource ? Visibility.Visible : Visibility.Collapsed;
        QuickChatResourceList.Visibility = quickChatVisible && !QuickChat.HasSelectedResource && !QuickChat.IsPreparing
            ? Visibility.Visible
            : Visibility.Collapsed;
        QuickChatEmptyHint.Visibility = quickChatVisible && !QuickChat.HasSelectedResource && !QuickChat.IsPreparing && !QuickChat.HasResources
            ? Visibility.Visible
            : Visibility.Collapsed;
        QuickChatConversationRoot.Visibility = quickChatVisible && QuickChat.HasSelectedResource && !QuickChat.IsPreparing
            ? Visibility.Visible
            : Visibility.Collapsed;
        QuickChatPreparing.Visibility = quickChatVisible && QuickChat.IsPreparing
            ? Visibility.Visible
            : Visibility.Collapsed;
        QuickChatHeaderTitle.Text = QuickChat.SelectedResource?.Title ?? QuickChat.Title;
        QuickChatHeaderSubtitle.Text = QuickChat.SelectedResource is null
            ? QuickChat.Description
            : QuickChat.SelectedResource.Kind == WorkspaceResourceKind.Project ? "项目会话" : "联系人会话";
        AttentionBadge.Visibility = ViewModel.AttentionCount > 0 ? Visibility.Visible : Visibility.Collapsed;
        AttentionBadgeText.Text = ViewModel.AttentionCount > 99 ? "99+" : ViewModel.AttentionCount.ToString();

        if (primary is not null)
        {
            CompactActivityStatus.Text = primary.StatusLabel;
            CompactActivityTitle.Text = primary.Title;
            CompactActivityDetail.Text = primary.Detail;
            CompactActivityAccent.Background = primary.RequiresAttention
                ? (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSFailureBrush"]
                : (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSAccentBrush"];
        }

        ResizeKeepingPetAnchor(
            approvalVisible ? ApprovalWidth : expanded || quickChatVisible ? ExpandedWidth : decisionVisible || summaryVisible ? SummaryWidth : CollapsedWidth,
            approvalVisible ? ApprovalHeight : expanded || quickChatVisible ? ExpandedHeight : decisionVisible || summaryVisible ? SummaryHeight : CollapsedHeight);

        var shouldAnimate = _isDragging || ViewModel.AnimationState is PetAnimationState.Running or
            PetAnimationState.Review or PetAnimationState.Waiting;
        if (shouldAnimate && !_animationTimer.IsEnabled) _animationTimer.Start();
        if (!shouldAnimate && _animationTimer.IsEnabled)
        {
            _animationTimer.Stop();
            ResetPetPose();
        }
    }

    private void ResizeKeepingPetAnchor(int width, int height)
    {
        if (AppWindow.Size.Width == width && AppWindow.Size.Height == height) return;
        var old = AppWindow.Size;
        var position = AppWindow.Position;
        AppWindow.Resize(new SizeInt32(width, height));
        AppWindow.Move(new PointInt32(
            position.X + old.Width - width,
            position.Y + old.Height - height));
    }

    private void OnPetPointerPressed(object sender, PointerRoutedEventArgs e)
    {
        if (!GetCursorPos(out _dragStartCursor)) return;
        _dragStartWindow = AppWindow.Position;
        _isDragging = true;
        _dragMoved = false;
        PetHitTarget.CapturePointer(e.Pointer);
        e.Handled = true;
        UpdateVisualState();
    }

    private void OnPetPointerMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging || !GetCursorPos(out var cursor)) return;
        var deltaX = cursor.X - _dragStartCursor.X;
        var deltaY = cursor.Y - _dragStartCursor.Y;
        if (Math.Abs(deltaX) + Math.Abs(deltaY) >= 6) _dragMoved = true;
        if (deltaX != 0) PetDirectionTransform.ScaleX = deltaX < 0 ? -1 : 1;

        var proposed = new PointInt32(_dragStartWindow.X + deltaX, _dragStartWindow.Y + deltaY);
        var display = DisplayArea.GetFromPoint(cursor, DisplayAreaFallback.Nearest);
        var work = display.WorkArea;
        proposed.X = Math.Clamp(proposed.X, work.X, work.X + Math.Max(0, work.Width - AppWindow.Size.Width));
        proposed.Y = Math.Clamp(proposed.Y, work.Y, work.Y + Math.Max(0, work.Height - AppWindow.Size.Height));
        AppWindow.Move(proposed);
        e.Handled = true;
    }

    private async void OnPetPointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_isDragging) return;
        PetHitTarget.ReleasePointerCapture(e.Pointer);
        _isDragging = false;
        if (!_dragMoved)
        {
            if (QuickChat.IsOpen)
            {
                await QuickChat.CloseAsync();
            }
            else
            {
                ViewModel.IsExpanded = false;
                ViewModel.CloseDetail();
                QuickChat.Open();
            }
        }
        else
        {
            try
            {
                await _placementStore.SaveAsync(new PetWindowPlacement(
                    AppWindow.Position.X + AppWindow.Size.Width,
                    AppWindow.Position.Y + AppWindow.Size.Height));
            }
            catch (Exception exception)
            {
                ViewModel.ErrorMessage = exception.Message;
            }
        }
        e.Handled = true;
        UpdateVisualState();
    }

    private void OnPetPointerCanceled(object sender, PointerRoutedEventArgs e)
    {
        _isDragging = false;
        PetHitTarget.ReleasePointerCapture(e.Pointer);
        UpdateVisualState();
    }

    private void OnAnimationTick(object? sender, object e)
    {
        _animationPhase = !_animationPhase;
        PetBodyTransform.Y = _animationPhase ? -4 : 1;
        LeftPawTransform.Angle = _animationPhase ? -13 : 11;
        RightPawTransform.Angle = _animationPhase ? 13 : -11;
    }

    private void ResetPetPose()
    {
        PetBodyTransform.Y = 0;
        LeftPawTransform.Angle = 0;
        RightPawTransform.Angle = 0;
    }

    private async void OnCompactActivityTapped(object sender, TappedRoutedEventArgs e)
    {
        if (QuickChat.IsOpen) await QuickChat.CloseAsync();
        ViewModel.IsExpanded = true;
        if (ViewModel.Activities.FirstOrDefault() is { } activity)
        {
            await ViewModel.SelectAsync(activity);
        }
        UpdateVisualState();
    }

    private void OnQuickChatPropertyChanged(object? sender, PropertyChangedEventArgs e) =>
        _ = DispatcherQueue.TryEnqueue(UpdateVisualState);

    private async void OnQuickChatResourceClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is PetQuickChatResourceViewModel resource)
        {
            await QuickChat.SelectAsync(resource);
        }
    }

    private async void OnQuickChatBackClicked(object sender, RoutedEventArgs e) => await QuickChat.BackAsync();

    private async void OnCloseQuickChatClicked(object sender, RoutedEventArgs e) => await QuickChat.CloseAsync();

    private async void OnOpenInboxFromChatClicked(object sender, RoutedEventArgs e)
    {
        await QuickChat.CloseAsync();
        ViewModel.IsExpanded = true;
    }

    private async void OnRefreshClicked(object sender, RoutedEventArgs e) => await ViewModel.RefreshAsync();

    private void OnCloseInboxClicked(object sender, RoutedEventArgs e)
    {
        ViewModel.IsExpanded = false;
        ViewModel.CloseDetail();
    }

    private async void OnActivityItemClick(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is PetActivityItemViewModel activity)
        {
            await ViewModel.SelectAsync(activity);
        }
    }

    private void OnCloseDetailClicked(object sender, RoutedEventArgs e) => ViewModel.CloseDetail();

    private async void OnIgnoreActivityClicked(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedActivity is { } activity) await ViewModel.IgnoreAsync(activity);
    }

    private async void OnMarkHandledClicked(object sender, RoutedEventArgs e)
    {
        if (ViewModel.SelectedActivity is { } activity) await ViewModel.MarkHandledAsync(activity);
    }

    private async void OnCancelTaskClicked(object sender, RoutedEventArgs e) => await ViewModel.CancelSelectedAsync();

    private static void OnAskUserSecretChanged(object sender, RoutedEventArgs e)
    {
        if (sender is PasswordBox { DataContext: AskUserFieldInputViewModel field } passwordBox)
        {
            field.Value = passwordBox.Password;
        }
    }

    private async void OnDeclineApproval(object sender, RoutedEventArgs e) =>
        await ResolveApprovalAsync(ConnectorApprovalAction.Decline);

    private async void OnAcceptApproval(object sender, RoutedEventArgs e) =>
        await ResolveApprovalAsync(ConnectorApprovalAction.Accept);

    private async void OnAcceptSessionApproval(object sender, RoutedEventArgs e) =>
        await ResolveApprovalAsync(ConnectorApprovalAction.AcceptForSession);

    private async Task ResolveApprovalAsync(ConnectorApprovalAction action)
    {
        if (_activeApproval is not { } approval) return;
        SetApprovalButtonsEnabled(false);
        ApprovalProgress.IsActive = true;
        try
        {
            await _approvals.ResolveAsync(approval.Id, action);
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = exception.Message;
        }
        finally
        {
            ApprovalProgress.IsActive = false;
            SetApprovalButtonsEnabled(true);
            RefreshApproval();
        }
    }

    private void SetApprovalButtonsEnabled(bool enabled)
    {
        DeclineApprovalButton.IsEnabled = enabled;
        AcceptApprovalButton.IsEnabled = enabled;
        AcceptSessionApprovalButton.IsEnabled = enabled;
    }

    private void OnClosed(object sender, WindowEventArgs args)
    {
        _animationTimer.Stop();
        _decisionTimer.Stop();
        ViewModel.PropertyChanged -= OnViewModelPropertyChanged;
        QuickChat.PropertyChanged -= OnQuickChatPropertyChanged;
        QuickChat.Conversation.PropertyChanged -= OnQuickChatPropertyChanged;
        _approvals.PendingChanged -= OnPendingApprovalsChanged;
        _approvals.DecisionRecorded -= OnApprovalDecisionRecorded;
        ViewModel.Stop();
        QuickChat.Dispose();
    }

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool GetCursorPos(out PointInt32 point);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr64(IntPtr window, int index);

    [DllImport("user32.dll", EntryPoint = "GetWindowLongW", SetLastError = true)]
    private static extern IntPtr GetWindowLongPtr32(IntPtr window, int index);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr64(IntPtr window, int index, IntPtr value);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtr32(IntPtr window, int index, IntPtr value);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetLayeredWindowAttributes(IntPtr window, uint colorKey, byte alpha, uint flags);

    private static IntPtr GetWindowLongPtr(IntPtr window, int index) =>
        IntPtr.Size == 8 ? GetWindowLongPtr64(window, index) : GetWindowLongPtr32(window, index);

    private static IntPtr SetWindowLongPtr(IntPtr window, int index, IntPtr value) =>
        IntPtr.Size == 8 ? SetWindowLongPtr64(window, index, value) : SetWindowLongPtr32(window, index, value);
}
