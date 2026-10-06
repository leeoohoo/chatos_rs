using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;

namespace ChatOS.Desktop.Features.MediaStudio;

internal sealed class MediaStudioImagePreviewDialog
{
    private const double ViewportWidth = 820;
    private const double ViewportHeight = 500;
    private readonly IReadOnlyList<MediaStudioImageItem> _images;
    private readonly Image _image;
    private readonly Grid _canvas;
    private readonly TextBlock _counter;
    private readonly TextBlock _details;
    private readonly TextBlock _zoomLabel;
    private readonly Button _previousButton;
    private readonly Button _nextButton;
    private BitmapImage? _bitmap;
    private int _index;
    private bool _fitToWindow = true;
    private double _scale = 1;

    public MediaStudioImagePreviewDialog(
        XamlRoot xamlRoot,
        IReadOnlyList<MediaStudioImageItem> images,
        int selectedIndex)
    {
        _images = images;
        _index = Math.Clamp(selectedIndex, 0, Math.Max(0, images.Count - 1));
        _image = new Image
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Stretch = Stretch.Uniform,
        };
        _image.ImageOpened += OnImageOpened;
        _image.ImageFailed += OnImageFailed;
        _canvas = new Grid
        {
            Width = ViewportWidth,
            Height = ViewportHeight,
            Background = new SolidColorBrush(Windows.UI.Color.FromArgb(255, 20, 20, 22)),
        };
        _canvas.Children.Add(_image);

        _counter = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
        _details = new TextBlock
        {
            VerticalAlignment = VerticalAlignment.Center,
            Opacity = 0.72,
        };
        _zoomLabel = new TextBlock
        {
            Width = 54,
            TextAlignment = TextAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        _previousButton = CreateButton("‹", "上一张", (_, _) => Move(-1));
        _nextButton = CreateButton("›", "下一张", (_, _) => Move(1));

        var viewer = new ScrollViewer
        {
            Width = ViewportWidth,
            Height = ViewportHeight,
            Content = _canvas,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollMode = ScrollMode.Enabled,
            VerticalScrollMode = ScrollMode.Enabled,
            ZoomMode = ZoomMode.Disabled,
        };

        var topBar = new Grid { Margin = new Thickness(0, 0, 0, 10) };
        topBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        topBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        topBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        topBar.ColumnDefinitions.Add(new ColumnDefinition());
        topBar.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(_previousButton, 0);
        Grid.SetColumn(_counter, 1);
        Grid.SetColumn(_nextButton, 2);
        Grid.SetColumn(_details, 4);
        _counter.Margin = new Thickness(12, 0, 12, 0);
        topBar.Children.Add(_previousButton);
        topBar.Children.Add(_counter);
        topBar.Children.Add(_nextButton);
        topBar.Children.Add(_details);

        var zoomOut = CreateButton("−", "缩小", (_, _) => ChangeZoom(1 / 1.25));
        var zoomIn = CreateButton("+", "放大", (_, _) => ChangeZoom(1.25));
        var actualSize = CreateButton("100%", "按原始尺寸显示", (_, _) => SetZoom(1));
        var fit = CreateButton("适应窗口", "缩放到适合窗口", (_, _) => FitToWindow());
        var bottomBar = new StackPanel
        {
            Margin = new Thickness(0, 10, 0, 0),
            HorizontalAlignment = HorizontalAlignment.Right,
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        bottomBar.Children.Add(zoomOut);
        bottomBar.Children.Add(_zoomLabel);
        bottomBar.Children.Add(zoomIn);
        bottomBar.Children.Add(actualSize);
        bottomBar.Children.Add(fit);

        var content = new StackPanel();
        content.Children.Add(topBar);
        content.Children.Add(viewer);
        content.Children.Add(bottomBar);
        Dialog = new ContentDialog
        {
            XamlRoot = xamlRoot,
            Title = "图片预览",
            Content = content,
            PrimaryButtonText = "用作视频首帧",
            CloseButtonText = "关闭",
            DefaultButton = ContentDialogButton.Close,
        };
        LoadCurrentImage();
    }

    public ContentDialog Dialog { get; }

    public MediaStudioImageItem? CurrentImage =>
        _images.Count == 0 ? null : _images[_index];

    private static Button CreateButton(
        string content,
        string tooltip,
        RoutedEventHandler click)
    {
        var button = new Button
        {
            Content = content,
            MinWidth = 40,
        };
        ToolTipService.SetToolTip(button, tooltip);
        button.Click += click;
        return button;
    }

    private void Move(int offset)
    {
        var next = _index + offset;
        if (next < 0 || next >= _images.Count) return;
        _index = next;
        _fitToWindow = true;
        LoadCurrentImage();
    }

    private void LoadCurrentImage()
    {
        _previousButton.IsEnabled = _index > 0;
        _nextButton.IsEnabled = _index + 1 < _images.Count;
        _counter.Text = _images.Count > 1 ? $"{_index + 1} / {_images.Count}" : "1 / 1";
        _details.Text = "正在读取图片…";
        _bitmap = null;
        _image.Source = null;
        if (CurrentImage is not { FilePath: { Length: > 0 } path } || !File.Exists(path))
        {
            _details.Text = "图片文件不存在";
            Dialog.IsPrimaryButtonEnabled = false;
            return;
        }

        Dialog.IsPrimaryButtonEnabled = true;
        _bitmap = new BitmapImage(new Uri(path, UriKind.Absolute));
        _image.Source = _bitmap;
        ApplyZoom();
    }

    private void OnImageOpened(object sender, RoutedEventArgs e)
    {
        if (_bitmap is null) return;
        _details.Text = $"{_bitmap.PixelWidth} × {_bitmap.PixelHeight} · {CurrentImage?.Name}";
        ApplyZoom();
    }

    private void OnImageFailed(object sender, ExceptionRoutedEventArgs e)
    {
        _details.Text = "无法读取图片";
        Dialog.IsPrimaryButtonEnabled = false;
    }

    private void ChangeZoom(double factor)
    {
        var current = _fitToWindow ? CalculateFitScale() : _scale;
        SetZoom(current * factor);
    }

    private void SetZoom(double scale)
    {
        _fitToWindow = false;
        _scale = Math.Clamp(scale, 0.05, 8);
        ApplyZoom();
    }

    private void FitToWindow()
    {
        _fitToWindow = true;
        ApplyZoom();
    }

    private double CalculateFitScale()
    {
        if (_bitmap is not { PixelWidth: > 0, PixelHeight: > 0 }) return 1;
        return Math.Min(1, Math.Min(ViewportWidth / _bitmap.PixelWidth, ViewportHeight / _bitmap.PixelHeight));
    }

    private void ApplyZoom()
    {
        if (_fitToWindow || _bitmap is not { PixelWidth: > 0, PixelHeight: > 0 })
        {
            _canvas.Width = ViewportWidth;
            _canvas.Height = ViewportHeight;
            _image.Width = double.NaN;
            _image.Height = double.NaN;
            _image.MaxWidth = ViewportWidth;
            _image.MaxHeight = ViewportHeight;
            _image.Stretch = Stretch.Uniform;
            _zoomLabel.Text = "适应";
            return;
        }

        var width = _bitmap.PixelWidth * _scale;
        var height = _bitmap.PixelHeight * _scale;
        _canvas.Width = Math.Max(ViewportWidth, width);
        _canvas.Height = Math.Max(ViewportHeight, height);
        _image.MaxWidth = double.PositiveInfinity;
        _image.MaxHeight = double.PositiveInfinity;
        _image.Width = width;
        _image.Height = height;
        _image.Stretch = Stretch.Fill;
        _zoomLabel.Text = $"{_scale * 100:0}%";
    }
}
