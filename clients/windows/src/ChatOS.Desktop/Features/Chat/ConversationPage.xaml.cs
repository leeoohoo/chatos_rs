using System.Collections.Specialized;
using System.ComponentModel;
using System.Text;
using ChatOS.Core.Domain;
using ChatOS.Desktop.DesignSystem;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Shapes;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Streams;
using Windows.System;
using Windows.UI.Core;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml.Automation;
using WindowsClipboard = Windows.ApplicationModel.DataTransfer.Clipboard;

namespace ChatOS.Desktop.Features.Chat;

public sealed partial class ConversationPage : UserControl
{
    private const double TaskGraphNodeWidth = 232;
    private const double TaskGraphNodeHeight = 142;
    private const double TaskGraphHorizontalGap = 84;
    private const double TaskGraphVerticalGap = 46;
    private const double TaskGraphCanvasMargin = 54;
    private ScrollViewer? _timelineScrollViewer;
    private bool _isPinnedToBottom = true;
    private readonly Dictionary<string, Windows.Foundation.Point> _taskGraphNodePositions =
        new(StringComparer.Ordinal);
    private string? _taskGraphCanvasSignature;
    private bool _isTaskGraphCanvasPanning;
    private bool _taskGraphPanHandlersAttached;
    private uint _taskGraphCanvasPointerId;
    private Windows.Foundation.Point _taskGraphPanStart;
    private double _taskGraphPanHorizontalOffset;
    private double _taskGraphPanVerticalOffset;

    public ConversationPage(
        ConversationSessionViewModel viewModel,
        MessageTaskGraphViewModel taskGraph,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        TaskGraph = taskGraph;
        Localization = localization;
        InitializeComponent();
        Loaded += OnLoaded;
        ViewModel.Turns.CollectionChanged += OnTurnsChanged;
        ViewModel.PropertyChanged += OnConversationPropertyChanged;
        TaskGraph.Nodes.CollectionChanged += OnTaskGraphNodesChanged;
        TaskGraph.PropertyChanged += OnTaskGraphPropertyChanged;
    }

    public ConversationSessionViewModel ViewModel { get; }

    public MessageTaskGraphViewModel TaskGraph { get; }

    public LocalizationViewModel Localization { get; }

    private async void OnTaskCallbackClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement
            {
                DataContext: ConversationReplyItemViewModel
                {
                    TaskGraphRequest: not null,
                } reply,
                Tag: string sectionName,
            })
        {
            var section = sectionName == "Process"
                ? TaskReplyInspectorSection.Process
                : TaskReplyInspectorSection.Detail;
            await reply.ToggleInspectorAsync(section, TaskGraph.LoadInspectorAsync);
        }
    }

    private async void OnRetryTaskRunClicked(object sender, RoutedEventArgs e)
    {
        StartupDiagnostics.RecordLifecycle("task graph retry click entered");
        try
        {
            await TaskGraph.RetryRunCommand.ExecuteAsync(null);
            StartupDiagnostics.RecordLifecycle(
                $"task graph retry click completed; has_error={TaskGraph.ErrorMessage is not null}");
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("task graph retry click", exception);
            TaskGraph.ErrorMessage = Localization.Text(
                $"无法重试运行：{exception.Message}",
                $"Unable to retry the run: {exception.Message}");
        }
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        UpdateTaskGraphWorkspaceLayout(ActualWidth);
        RebuildTaskGraphCanvas();
        UpdateTaskGraphCanvasLabels();
        if (!_taskGraphPanHandlersAttached)
        {
            _taskGraphPanHandlersAttached = true;
            TaskGraphCanvasViewport.AddHandler(
                PointerPressedEvent,
                new PointerEventHandler(OnTaskGraphCanvasPointerPressed),
                true);
            TaskGraphCanvasViewport.AddHandler(
                PointerMovedEvent,
                new PointerEventHandler(OnTaskGraphCanvasPointerMoved),
                true);
            TaskGraphCanvasViewport.AddHandler(
                PointerReleasedEvent,
                new PointerEventHandler(OnTaskGraphCanvasPointerReleased),
                true);
            TaskGraphCanvasViewport.AddHandler(
                PointerCanceledEvent,
                new PointerEventHandler(OnTaskGraphCanvasPointerReleased),
                true);
        }
        _timelineScrollViewer ??= FindDescendant<ScrollViewer>(TimelineList);
        if (_timelineScrollViewer is not null)
        {
            _timelineScrollViewer.ViewChanged -= OnTimelineViewChanged;
            _timelineScrollViewer.ViewChanged += OnTimelineViewChanged;
        }

        ScrollToBottom(false);
    }

    private void OnConversationPageSizeChanged(object sender, SizeChangedEventArgs e)
    {
        UpdateTaskGraphWorkspaceLayout(e.NewSize.Width);
    }

    private void UpdateTaskGraphWorkspaceLayout(double availableWidth)
    {
        if (availableWidth <= 0 || TaskGraphWorkspace is null || TaskGraphNodeColumn is null)
        {
            return;
        }

        // Match the macOS graph/inspector split on wide windows, while keeping both
        // regions reachable inside the conversation host on narrow ones.
        var workspaceWidth = Math.Min(1280, availableWidth);
        TaskGraphWorkspace.Width = workspaceWidth;
        TaskGraphNodeColumn.Width = new GridLength(workspaceWidth switch
        {
            < 720 => Math.Max(220, workspaceWidth * 0.45),
            < 1050 => workspaceWidth * 0.55,
            _ => Math.Min(760, workspaceWidth * 0.62),
        });
    }

    private void OnTaskGraphNodesChanged(object? sender, NotifyCollectionChangedEventArgs e) =>
        RebuildTaskGraphCanvas();

    private void OnTaskGraphPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(MessageTaskGraphViewModel.SelectedNode) or
            nameof(MessageTaskGraphViewModel.IsOpen))
        {
            RebuildTaskGraphCanvas();
        }
    }

    private void RebuildTaskGraphCanvas()
    {
        if (TaskGraphCanvas is null)
        {
            return;
        }

        var nodes = TaskGraph.Nodes.ToArray();
        var signature = string.Join('|', nodes.Select(node => $"{node.Id}:{node.Depth}"));
        var graphChanged = !string.Equals(signature, _taskGraphCanvasSignature, StringComparison.Ordinal);
        _taskGraphCanvasSignature = signature;
        TaskGraphCanvas.Children.Clear();
        _taskGraphNodePositions.Clear();

        if (nodes.Length == 0)
        {
            TaskGraphCanvas.Width = 800;
            TaskGraphCanvas.Height = 520;
            return;
        }

        var depths = MessageTaskGraphLayout.ResolveDepths(nodes);
        var columns = nodes.GroupBy(node => depths[node.Id])
            .OrderBy(group => group.Key)
            .ToArray();
        var maximumRows = Math.Max(1, columns.Max(group => group.Count()));
        var contentWidth = Math.Max(
            800,
            TaskGraphCanvasMargin * 2 + columns.Length * TaskGraphNodeWidth +
            Math.Max(0, columns.Length - 1) * TaskGraphHorizontalGap);
        var contentHeight = Math.Max(
            520,
            TaskGraphCanvasMargin * 2 + maximumRows * TaskGraphNodeHeight +
            Math.Max(0, maximumRows - 1) * TaskGraphVerticalGap);
        TaskGraphCanvas.Width = contentWidth;
        TaskGraphCanvas.Height = contentHeight;

        foreach (var (column, columnIndex) in columns.Select((value, index) => (value, index)))
        {
            var ordered = column.OrderBy(node => node.Title, StringComparer.CurrentCultureIgnoreCase).ToArray();
            var columnHeight = ordered.Length * TaskGraphNodeHeight +
                Math.Max(0, ordered.Length - 1) * TaskGraphVerticalGap;
            var startY = Math.Max(TaskGraphCanvasMargin, (contentHeight - columnHeight) / 2);
            for (var rowIndex = 0; rowIndex < ordered.Length; rowIndex++)
            {
                var node = ordered[rowIndex];
                _taskGraphNodePositions[node.Id] = new Windows.Foundation.Point(
                    TaskGraphCanvasMargin + TaskGraphNodeWidth / 2 +
                    columnIndex * (TaskGraphNodeWidth + TaskGraphHorizontalGap),
                    startY + TaskGraphNodeHeight / 2 +
                    rowIndex * (TaskGraphNodeHeight + TaskGraphVerticalGap));
            }
        }

        foreach (var target in nodes)
        {
            var prerequisiteIds = target.Task.PrerequisiteTaskIds
                .Concat(target.Task.PrerequisiteTasks.Select(reference => reference.Id))
                .Distinct(StringComparer.Ordinal);
            foreach (var prerequisiteId in prerequisiteIds)
            {
                if (_taskGraphNodePositions.TryGetValue(prerequisiteId, out var sourcePoint) &&
                    _taskGraphNodePositions.TryGetValue(target.Id, out var targetPoint))
                {
                    AddTaskGraphEdge(sourcePoint, targetPoint);
                }
            }
        }

        foreach (var node in nodes)
        {
            AddTaskGraphNode(node, _taskGraphNodePositions[node.Id]);
        }

        if (graphChanged)
        {
            DispatcherQueue.TryEnqueue(FitTaskGraphCanvas);
        }
    }

    private void AddTaskGraphEdge(
        Windows.Foundation.Point source,
        Windows.Foundation.Point target)
    {
        var sourcePoint = new Windows.Foundation.Point(source.X + TaskGraphNodeWidth / 2, source.Y);
        var targetPoint = new Windows.Foundation.Point(target.X - TaskGraphNodeWidth / 2, target.Y);
        var middleX = (sourcePoint.X + targetPoint.X) / 2;
        var figure = new PathFigure { StartPoint = sourcePoint };
        figure.Segments.Add(new BezierSegment
        {
            Point1 = new Windows.Foundation.Point(middleX, sourcePoint.Y),
            Point2 = new Windows.Foundation.Point(middleX, targetPoint.Y),
            Point3 = targetPoint,
        });
        var geometry = new PathGeometry();
        geometry.Figures.Add(figure);
        TaskGraphCanvas.Children.Add(new Microsoft.UI.Xaml.Shapes.Path
        {
            Data = geometry,
            Stroke = ResolveBrush("ChatOSPurpleBrush", Microsoft.UI.Colors.MediumPurple),
            StrokeThickness = 2,
            Opacity = 0.82,
            IsHitTestVisible = false,
        });
    }

    private void AddTaskGraphNode(
        MessageTaskGraphNodeItemViewModel node,
        Windows.Foundation.Point center)
    {
        var content = new StackPanel { Spacing = 5 };
        content.Children.Add(new TextBlock
        {
            Text = node.Title,
            TextWrapping = TextWrapping.Wrap,
            MaxHeight = 42,
            Style = ResolveStyle("ChatOSBodyStrongTextStyle"),
        });
        content.Children.Add(new TextBlock
        {
            Text = node.Status,
            Foreground = ResolveBrush("ChatOSPurpleBrush", Microsoft.UI.Colors.MediumPurple),
            Style = ResolveStyle("ChatOSCaptionTextStyle"),
        });
        if (node.HasPrerequisites)
        {
            var dependency = new TextBlock
            {
                Text = node.DependencySummary,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Style = ResolveStyle("ChatOSCaptionTextStyle"),
            };
            AutomationProperties.SetAutomationId(dependency, "ChatOS.Chat.TaskGraph.NodeDependencies");
            content.Children.Add(dependency);
        }
        if (node.HasRunIdentity)
        {
            content.Children.Add(new TextBlock
            {
                Text = node.RunIdentity,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Style = ResolveStyle("ChatOSCaptionTextStyle"),
            });
        }

        var button = new Button
        {
            Width = TaskGraphNodeWidth,
            Height = TaskGraphNodeHeight,
            Padding = new Thickness(13, 10, 13, 10),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Top,
            Background = ResolveBrush("ChatOSContentBackgroundBrush", Microsoft.UI.Colors.White),
            BorderBrush = ReferenceEquals(TaskGraph.SelectedNode, node)
                ? ResolveBrush("ChatOSPurpleBrush", Microsoft.UI.Colors.MediumPurple)
                : ResolveBrush("ChatOSBorderBrush", Microsoft.UI.Colors.LightGray),
            BorderThickness = ReferenceEquals(TaskGraph.SelectedNode, node)
                ? new Thickness(2)
                : new Thickness(1),
            CornerRadius = new CornerRadius(10),
            Content = content,
            Tag = node,
        };
        AutomationProperties.SetAutomationId(button, $"ChatOS.Chat.TaskGraph.Canvas.Node.{node.Id}");
        AutomationProperties.SetName(button, node.AccessibilityName);
        button.Click += OnTaskGraphCanvasNodeClicked;
        Canvas.SetLeft(button, center.X - TaskGraphNodeWidth / 2);
        Canvas.SetTop(button, center.Y - TaskGraphNodeHeight / 2);
        TaskGraphCanvas.Children.Add(button);
    }

    private void OnTaskGraphCanvasNodeClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: MessageTaskGraphNodeItemViewModel node })
        {
            TaskGraph.SelectNodeCommand.Execute(node);
        }
    }

    private void OnTaskGraphZoomOutClicked(object sender, RoutedEventArgs e) =>
        ChangeTaskGraphZoom(Math.Max(0.25f, TaskGraphCanvasViewport.ZoomFactor - 0.1f));

    private void OnTaskGraphZoomResetClicked(object sender, RoutedEventArgs e) =>
        ChangeTaskGraphZoom(1);

    private void OnTaskGraphZoomInClicked(object sender, RoutedEventArgs e) =>
        ChangeTaskGraphZoom(Math.Min(2, TaskGraphCanvasViewport.ZoomFactor + 0.1f));

    private void OnTaskGraphCenterClicked(object sender, RoutedEventArgs e) =>
        CenterTaskGraphCanvas();

    private void OnTaskGraphFitClicked(object sender, RoutedEventArgs e) =>
        FitTaskGraphCanvas();

    private void ChangeTaskGraphZoom(float zoom)
    {
        // Submit zoom and center offsets as one atomic viewport update. Two separate
        // ChangeView calls race inside ScrollViewer and can leave the zoom at one step.
        var horizontalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Width * zoom - TaskGraphCanvasViewport.ActualWidth) / 2);
        var verticalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Height * zoom - TaskGraphCanvasViewport.ActualHeight) / 2);
        TaskGraphCanvasViewport.ChangeView(horizontalOffset, verticalOffset, zoom, true);
    }

    private void FitTaskGraphCanvas()
    {
        if (TaskGraphCanvasViewport.ActualWidth <= 0 || TaskGraphCanvasViewport.ActualHeight <= 0)
        {
            return;
        }

        var widthScale = Math.Max(1, TaskGraphCanvasViewport.ActualWidth - 72) / TaskGraphCanvas.Width;
        var heightScale = Math.Max(1, TaskGraphCanvasViewport.ActualHeight - 72) / TaskGraphCanvas.Height;
        var zoom = (float)Math.Min(1, Math.Max(0.25, Math.Min(widthScale, heightScale)));
        var horizontalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Width * zoom - TaskGraphCanvasViewport.ActualWidth) / 2);
        var verticalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Height * zoom - TaskGraphCanvasViewport.ActualHeight) / 2);
        TaskGraphCanvasViewport.ChangeView(horizontalOffset, verticalOffset, zoom, true);
    }

    private void CenterTaskGraphCanvas()
    {
        var zoom = TaskGraphCanvasViewport.ZoomFactor;
        var horizontalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Width * zoom - TaskGraphCanvasViewport.ActualWidth) / 2);
        var verticalOffset = Math.Max(
            0,
            (TaskGraphCanvas.Height * zoom - TaskGraphCanvasViewport.ActualHeight) / 2);
        TaskGraphCanvasViewport.ChangeView(horizontalOffset, verticalOffset, null, false);
    }

    private void OnTaskGraphCanvasViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
    {
        if (TaskGraphZoomLabel is not null)
        {
            var percentage = $"{Math.Round(TaskGraphCanvasViewport.ZoomFactor * 100):0}%";
            TaskGraphZoomLabel.Content = percentage;
            AutomationProperties.SetName(
                TaskGraphZoomLabel,
                Localization.Text($"任务图缩放 {percentage}，点击重置", $"Task graph zoom {percentage}; click to reset"));
        }
    }

    private void OnTaskGraphCanvasPointerPressed(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(TaskGraphCanvasViewport);
        if (!point.Properties.IsLeftButtonPressed || HasButtonAncestor(e.OriginalSource as DependencyObject))
        {
            return;
        }

        _isTaskGraphCanvasPanning = true;
        _taskGraphCanvasPointerId = point.PointerId;
        _taskGraphPanStart = point.Position;
        _taskGraphPanHorizontalOffset = TaskGraphCanvasViewport.HorizontalOffset;
        _taskGraphPanVerticalOffset = TaskGraphCanvasViewport.VerticalOffset;
        TaskGraphCanvasViewport.CapturePointer(e.Pointer);
        e.Handled = true;
    }

    private void OnTaskGraphCanvasPointerMoved(object sender, PointerRoutedEventArgs e)
    {
        var point = e.GetCurrentPoint(TaskGraphCanvasViewport);
        if (!_isTaskGraphCanvasPanning || point.PointerId != _taskGraphCanvasPointerId)
        {
            return;
        }

        TaskGraphCanvasViewport.ChangeView(
            _taskGraphPanHorizontalOffset - (point.Position.X - _taskGraphPanStart.X),
            _taskGraphPanVerticalOffset - (point.Position.Y - _taskGraphPanStart.Y),
            null,
            true);
        e.Handled = true;
    }

    private void OnTaskGraphCanvasPointerReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_isTaskGraphCanvasPanning || e.Pointer.PointerId != _taskGraphCanvasPointerId)
        {
            return;
        }

        _isTaskGraphCanvasPanning = false;
        TaskGraphCanvasViewport.ReleasePointerCapture(e.Pointer);
        e.Handled = true;
    }

    private void UpdateTaskGraphCanvasLabels()
    {
        var labels = new (string Id, string Chinese, string English)[]
        {
            ("ChatOS.Chat.TaskGraph.ZoomOut", "缩小任务图", "Zoom out task graph"),
            ("ChatOS.Chat.TaskGraph.ZoomReset", "重置为 100%", "Reset to 100%"),
            ("ChatOS.Chat.TaskGraph.ZoomIn", "放大任务图", "Zoom in task graph"),
            ("ChatOS.Chat.TaskGraph.Center", "居中任务图", "Center task graph"),
            ("ChatOS.Chat.TaskGraph.Fit", "适应窗口", "Fit task graph"),
        };
        foreach (var (id, chinese, english) in labels)
        {
            if (FindByAutomationId(TaskGraphCanvasHost, id) is FrameworkElement element)
            {
                var label = Localization.Text(chinese, english);
                AutomationProperties.SetName(element, label);
                ToolTipService.SetToolTip(element, label);
            }
        }
    }

    private static FrameworkElement? FindByAutomationId(DependencyObject root, string automationId)
    {
        if (root is FrameworkElement element && AutomationProperties.GetAutomationId(element) == automationId)
        {
            return element;
        }

        for (var index = 0; index < VisualTreeHelper.GetChildrenCount(root); index++)
        {
            if (FindByAutomationId(VisualTreeHelper.GetChild(root, index), automationId) is { } match)
            {
                return match;
            }
        }
        return null;
    }

    private static bool HasButtonAncestor(DependencyObject? element)
    {
        while (element is not null)
        {
            if (element is Button)
            {
                return true;
            }
            element = VisualTreeHelper.GetParent(element);
        }
        return false;
    }

    private static Brush ResolveBrush(string key, Windows.UI.Color fallback) =>
        Application.Current.Resources.TryGetValue(key, out var resource) && resource is Brush brush
            ? brush
            : new SolidColorBrush(fallback);

    private static Style? ResolveStyle(string key) =>
        Application.Current.Resources.TryGetValue(key, out var resource) ? resource as Style : null;

    private async void OnAddAttachmentClicked(object sender, RoutedEventArgs e)
    {
        StartupDiagnostics.RecordLifecycle("chat attachment picker click entered");
        try
        {
            var window = (Application.Current as App)?.MainWindow;
            if (window is null)
            {
                ViewModel.AttachmentError = Localization.Text("无法找到当前窗口。", "Unable to find the current window.");
                return;
            }

            var paths = WindowsFileOpenDialog.PickFiles(
                WinRT.Interop.WindowNative.GetWindowHandle(window),
                Localization.Text("选择聊天附件", "Choose chat attachments"));
            if (paths.Count == 0)
            {
                StartupDiagnostics.RecordLifecycle("chat attachment picker cancelled");
                return;
            }

            var storageFiles = new List<StorageFile>(paths.Count);
            foreach (var path in paths)
            {
                storageFiles.Add(await StorageFile.GetFileFromPathAsync(path));
            }

            await AddStorageFilesAsync(storageFiles);
            StartupDiagnostics.RecordLifecycle($"chat attachment picker completed; files={storageFiles.Count}");
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("chat attachment picker", exception);
            ViewModel.AttachmentError = Localization.Text(
                $"无法添加附件：{exception.Message}",
                $"Unable to add attachment: {exception.Message}");
        }
    }

    private void OnComposerDragOver(object sender, DragEventArgs e)
    {
        if (e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            e.AcceptedOperation = DataPackageOperation.Copy;
            e.DragUIOverride.Caption = Localization.Text("添加到当前消息", "Add to current message");
            e.DragUIOverride.IsCaptionVisible = true;
        }
    }

    private async void OnComposerDrop(object sender, DragEventArgs e)
    {
        if (!e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            return;
        }

        try
        {
            var items = await e.DataView.GetStorageItemsAsync();
            await AddStorageFilesAsync(items.OfType<StorageFile>());
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取拖入的文件：{exception.Message}",
                $"Unable to read dropped files: {exception.Message}");
        }
    }

    private async void OnComposerPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Enter)
        {
            return;
        }

        var shiftState = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Shift);
        if ((shiftState & CoreVirtualKeyStates.Down) != 0)
        {
            // Shift+Enter keeps the macOS composer behaviour for inserting a newline.
            return;
        }

        e.Handled = true;
        if (ViewModel.SendCommand.CanExecute(null))
        {
            await ViewModel.SendCommand.ExecuteAsync(null);
        }
    }

    private async void OnComposerPaste(object sender, TextControlPasteEventArgs e)
    {
        DataPackageView content;
        try
        {
            content = WindowsClipboard.GetContent();
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取剪贴板：{exception.Message}",
                $"Unable to read the clipboard: {exception.Message}");
            return;
        }

        if (content.Contains(StandardDataFormats.StorageItems))
        {
            e.Handled = true;
            var items = await content.GetStorageItemsAsync();
            await AddStorageFilesAsync(items.OfType<StorageFile>());
            return;
        }

        if (content.Contains(StandardDataFormats.Bitmap))
        {
            e.Handled = true;
            try
            {
                var bitmap = await content.GetBitmapAsync();
                var data = await NormalizeClipboardImageToPngAsync(bitmap);
                ViewModel.AddAttachments(new[]
                {
                    ConversationAttachmentDraft.Create(
                        Localization.Text(
                            $"粘贴的图片 {DateTime.Now:yyyy-MM-dd HH.mm.ss}.png",
                            $"Pasted image {DateTime.Now:yyyy-MM-dd HH.mm.ss}.png"),
                        "image/png",
                        ConversationAttachmentKind.Image,
                        ConversationAttachmentOrigin.PastedImage,
                        data),
                });
            }
            catch (Exception exception)
            {
                ViewModel.AttachmentError = Localization.Text(
                    $"无法读取粘贴的图片：{exception.Message}",
                    $"Unable to read pasted image: {exception.Message}");
            }

            return;
        }

        if (!content.Contains(StandardDataFormats.Text))
        {
            return;
        }

        // TextControlPasteEventArgs must be handled before the first await. Otherwise the
        // native TextBox completes its own paste while long text is also being attached.
        e.Handled = true;
        try
        {
            var text = await content.GetTextAsync();
            var dataBytes = Encoding.UTF8.GetBytes(text);
            if (!ConversationComposerTextPaste.ShouldCreateAttachment(text))
            {
                var edit = ConversationComposerTextPaste.Insert(
                    ComposerTextBox.Text,
                    ComposerTextBox.SelectionStart,
                    ComposerTextBox.SelectionLength,
                    text);
                ComposerTextBox.Text = edit.Text;
                ComposerTextBox.SelectionStart = edit.CaretIndex;
                ComposerTextBox.SelectionLength = 0;
                return;
            }

            ViewModel.AddAttachments(new[]
            {
                ConversationAttachmentDraft.Create(
                    Localization.Text(
                        $"粘贴的长文本 {DateTime.Now:yyyy-MM-dd HH.mm.ss}.txt",
                        $"Pasted long text {DateTime.Now:yyyy-MM-dd HH.mm.ss}.txt"),
                    "text/plain",
                    ConversationAttachmentKind.File,
                    ConversationAttachmentOrigin.PastedText,
                    dataBytes),
            });
        }
        catch (Exception exception)
        {
            ViewModel.AttachmentError = Localization.Text(
                $"无法读取粘贴的文本：{exception.Message}",
                $"Unable to read pasted text: {exception.Message}");
        }
    }

    private void OnRemoveAttachmentClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { DataContext: ConversationAttachmentDraft attachment })
        {
            ViewModel.RemoveAttachmentCommand.Execute(attachment);
        }
    }

    private void OnAskUserSecretChanged(object sender, RoutedEventArgs e)
    {
        if (sender is PasswordBox
            {
                DataContext: AskUserFieldInputViewModel field,
                Password: var password,
            })
        {
            field.Value = password;
        }
    }

    private void OnUnreadNewerClicked(object sender, RoutedEventArgs e)
    {
        ViewModel.MarkNewerContentReadCommand.Execute(null);
        _isPinnedToBottom = true;
        ViewModel.SetViewportPinnedToBottom(true);
        ScrollToBottom(true);
    }

    private async void OnModelSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: ConversationModelOption model })
        {
            await ViewModel.SelectModelAsync(model);
        }
    }

    private async void OnReasoningToggled(object sender, RoutedEventArgs e)
    {
        if (sender is ToggleSwitch toggle)
        {
            await ViewModel.SetReasoningAsync(toggle.IsOn);
        }
    }

    private async void OnReasoningLevelSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string level })
        {
            await ViewModel.SetReasoningLevelAsync(level);
        }
    }

    private void OnTimelineViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        if (_timelineScrollViewer is null)
        {
            return;
        }

        var remaining = _timelineScrollViewer.ScrollableHeight - _timelineScrollViewer.VerticalOffset;
        var pinned = remaining <= 32;
        if (pinned == _isPinnedToBottom)
        {
            return;
        }

        _isPinnedToBottom = pinned;
        ViewModel.SetViewportPinnedToBottom(pinned);
    }

    private void OnTurnsChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (_isPinnedToBottom && ViewModel.UnreadNewerCount == 0)
        {
            DispatcherQueue.TryEnqueue(() => ScrollToBottom(false));
        }
    }

    private async void OnRemoteSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: ConversationRemoteConnectionOption connection })
        {
            await ViewModel.SetRemoteConnectionAsync(connection);
        }
    }

    private async void OnPreviewAttachmentClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { DataContext: ConversationAttachmentDraft attachment })
        {
            return;
        }

        FrameworkElement preview;
        try
        {
            if (attachment.Kind == ConversationAttachmentKind.Image)
            {
                BitmapImage source = await ConversationAttachmentThumbnail.DecodeImageAsync(attachment.Data);
                preview = new ScrollViewer
                {
                    MaxWidth = 760,
                    MaxHeight = 600,
                    HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
                    VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                    Content = new Image
                    {
                        Source = source,
                        MaxWidth = 720,
                        MaxHeight = 560,
                        Stretch = Stretch.Uniform,
                    },
                };
            }
            else if (IsTextLike(attachment.MimeType))
            {
                preview = new TextBox
                {
                    Width = 680,
                    MaxHeight = 540,
                    AcceptsReturn = true,
                    FontFamily = new FontFamily("Cascadia Mono, Consolas"),
                    IsReadOnly = true,
                    Text = Encoding.UTF8.GetString(attachment.Data),
                    TextWrapping = TextWrapping.Wrap,
                };
            }
            else
            {
                preview = new TextBlock
                {
                    MaxWidth = 520,
                    Text = Localization.Text(
                        $"{attachment.MimeType} · {attachment.Size:N0} 字节\n文件已准备发送，发送后将由 ChatOS 读取和处理。",
                        $"{attachment.MimeType} · {attachment.Size:N0} bytes\nThe file is ready and will be processed by ChatOS after sending."),
                    TextWrapping = TextWrapping.Wrap,
                };
            }
        }
        catch (Exception exception)
        {
            preview = new TextBlock
            {
                Text = Localization.Text(
                    $"无法预览附件：{exception.Message}",
                    $"Unable to preview attachment: {exception.Message}"),
                TextWrapping = TextWrapping.Wrap,
            };
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = attachment.Name,
            Content = preview,
            CloseButtonText = Localization.Text("关闭", "Close"),
            DefaultButton = ContentDialogButton.Close,
        };
        await dialog.ShowAsync();
    }

    private async void OnPreviewHistoryAttachmentClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement
            {
                DataContext: ConversationAttachmentItemViewModel
                {
                    Attachment: { } attachment,
                    HasImagePreview: true,
                },
            })
        {
            return;
        }

        FrameworkElement preview;
        try
        {
            var source = await ConversationAttachmentThumbnail.DecodeReferenceImageAsync(attachment);
            preview = new ScrollViewer
            {
                MaxWidth = 900,
                MaxHeight = 680,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
                VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                Content = new Image
                {
                    Source = source,
                    MaxWidth = 860,
                    MaxHeight = 640,
                    Stretch = Stretch.Uniform,
                },
            };
        }
        catch (Exception exception)
        {
            preview = new TextBlock
            {
                Text = Localization.Text(
                    $"图片加载失败：{exception.Message}",
                    $"Unable to load image: {exception.Message}"),
                TextWrapping = TextWrapping.Wrap,
            };
        }

        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = attachment.Name,
            Content = preview,
            CloseButtonText = Localization.Text("关闭", "Close"),
            DefaultButton = ContentDialogButton.Close,
        };
        await dialog.ShowAsync();
    }

    private async void OnTurnTaskGraphClicked(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement
            {
                DataContext: ConversationTurnItemViewModel { TaskGraphRequest: { } request },
            })
        {
            await TaskGraph.OpenAsync(request);
        }
    }

    private void OnConversationPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(ConversationSessionViewModel.ConversationId))
        {
            return;
        }

        // The same page instance hosts contact and project conversations. A task panel or
        // viewport anchor from the previous conversation must never leak into the new one.
        TaskGraph.CloseCommand.Execute(null);
        _isPinnedToBottom = true;
        ViewModel.SetViewportPinnedToBottom(true);
        _ = DispatcherQueue.TryEnqueue(() => ScrollToBottom(false));
    }

    private void ScrollToBottom(bool animated)
    {
        _timelineScrollViewer?.ChangeView(
            null,
            _timelineScrollViewer.ScrollableHeight,
            null,
            !animated);
    }

    private async Task AddStorageFilesAsync(IEnumerable<StorageFile> files)
    {
        var attachments = new List<ConversationAttachmentDraft>();
        var errors = new List<string>();
        var remainingCount = Math.Max(
            0,
            ConversationSessionViewModel.MaximumAttachmentCount - ViewModel.Attachments.Count);
        var remainingBytes = Math.Max(
            0,
            ConversationSessionViewModel.MaximumAttachmentBytes -
            ViewModel.Attachments.Sum(static attachment => attachment.Size));
        foreach (var file in files)
        {
            if (attachments.Count >= remainingCount)
            {
                errors.Add(Localization.Text(
                    $"单次最多添加 {ConversationSessionViewModel.MaximumAttachmentCount} 个附件",
                    $"You can add at most {ConversationSessionViewModel.MaximumAttachmentCount} attachments at a time"));
                break;
            }

            try
            {
                var properties = await file.GetBasicPropertiesAsync();
                if (properties.Size > ConversationSessionViewModel.MaximumAttachmentBytes)
                {
                    errors.Add(Localization.Text(
                        $"“{file.Name}”超过 20 MB",
                        $"“{file.Name}” exceeds 20 MB"));
                    continue;
                }

                if (properties.Size > (ulong)remainingBytes)
                {
                    errors.Add(Localization.Text(
                        "附件总大小不能超过 20 MB",
                        "The total attachment size cannot exceed 20 MB"));
                    continue;
                }

                using var stream = await file.OpenReadAsync();
                var data = await ReadAllBytesAsync(stream, remainingBytes);
                var mimeType = string.IsNullOrWhiteSpace(file.ContentType)
                    ? "application/octet-stream"
                    : file.ContentType;
                attachments.Add(ConversationAttachmentDraft.Create(
                    file.Name,
                    mimeType,
                    AttachmentKind(mimeType),
                    ConversationAttachmentOrigin.File,
                    data));
                remainingBytes -= data.Length;
            }
            catch (Exception exception)
            {
                errors.Add(Localization.Text(
                    $"无法读取“{file.Name}”：{exception.Message}",
                    $"Unable to read “{file.Name}”: {exception.Message}"));
            }
        }

        ViewModel.AddAttachments(attachments);
        if (errors.Count > 0)
        {
            if (!string.IsNullOrWhiteSpace(ViewModel.AttachmentError))
            {
                errors.Add(ViewModel.AttachmentError);
            }

            ViewModel.AttachmentError = string.Join("；", errors);
        }
    }

    private static async Task<byte[]> ReadAllBytesAsync(
        IRandomAccessStream stream,
        int maximumBytes = ConversationSessionViewModel.MaximumAttachmentBytes)
    {
        if (stream.Size > (ulong)maximumBytes)
        {
            throw new InvalidOperationException("The attachment exceeds the remaining size limit.");
        }

        var bytes = new byte[(int)stream.Size];
        using var reader = new DataReader(stream.GetInputStreamAt(0));
        await reader.LoadAsync((uint)stream.Size);
        reader.ReadBytes(bytes);
        return bytes;
    }

    private static async Task<byte[]> NormalizeClipboardImageToPngAsync(
        RandomAccessStreamReference bitmap)
    {
        using var source = await bitmap.OpenReadAsync();
        if (source.Size > ConversationSessionViewModel.MaximumAttachmentBytes)
        {
            throw new InvalidOperationException("The pasted image exceeds 20 MB.");
        }

        var decoder = await BitmapDecoder.CreateAsync(source);
        var pixelData = await decoder.GetPixelDataAsync(
            BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied,
            new BitmapTransform(),
            ExifOrientationMode.IgnoreExifOrientation,
            ColorManagementMode.ColorManageToSRgb);
        var pixels = pixelData.DetachPixelData();

        using var output = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, output);
        encoder.SetPixelData(
            BitmapPixelFormat.Bgra8,
            BitmapAlphaMode.Premultiplied,
            decoder.PixelWidth,
            decoder.PixelHeight,
            decoder.DpiX,
            decoder.DpiY,
            pixels);
        await encoder.FlushAsync();
        output.Seek(0);
        return await ReadAllBytesAsync(output);
    }

    private static ConversationAttachmentKind AttachmentKind(string mimeType)
    {
        if (mimeType.StartsWith("image/", StringComparison.OrdinalIgnoreCase))
        {
            return ConversationAttachmentKind.Image;
        }

        if (mimeType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase))
        {
            return ConversationAttachmentKind.Audio;
        }

        return ConversationAttachmentKind.File;
    }

    private static bool IsTextLike(string mimeType) =>
        mimeType.StartsWith("text/", StringComparison.OrdinalIgnoreCase) ||
        mimeType.Equals("application/json", StringComparison.OrdinalIgnoreCase) ||
        mimeType.Equals("application/xml", StringComparison.OrdinalIgnoreCase) ||
        mimeType.Equals("application/x-yaml", StringComparison.OrdinalIgnoreCase);

    private static T? FindDescendant<T>(DependencyObject root)
        where T : DependencyObject
    {
        var count = VisualTreeHelper.GetChildrenCount(root);
        for (var index = 0; index < count; index++)
        {
            var child = VisualTreeHelper.GetChild(root, index);
            if (child is T result)
            {
                return result;
            }

            var nested = FindDescendant<T>(child);
            if (nested is not null)
            {
                return nested;
            }
        }

        return null;
    }
}
