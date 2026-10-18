using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.UI;

namespace ChatOS.Desktop.DesignSystem;

public sealed class CheckerboardCanvas : Canvas
{
    private const double TileSize = 24;
    private int _columns;
    private int _rows;

    public CheckerboardCanvas()
    {
        IsHitTestVisible = false;
        SizeChanged += (_, _) => Rebuild();
    }

    private void Rebuild()
    {
        var columns = (int)Math.Ceiling(ActualWidth / TileSize);
        var rows = (int)Math.Ceiling(ActualHeight / TileSize);
        if (columns == _columns && rows == _rows)
        {
            return;
        }

        _columns = columns;
        _rows = rows;
        Children.Clear();
        var brush = new SolidColorBrush(Color.FromArgb(12, 80, 80, 80));
        for (var row = 0; row < rows; row++)
        {
            for (var column = row % 2; column < columns; column += 2)
            {
                var tile = new Rectangle
                {
                    Width = TileSize,
                    Height = TileSize,
                    Fill = brush,
                };
                SetLeft(tile, column * TileSize);
                SetTop(tile, row * TileSize);
                Children.Add(tile);
            }
        }
    }
}
