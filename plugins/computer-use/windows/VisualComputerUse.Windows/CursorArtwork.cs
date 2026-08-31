using System.Windows;
using System.Windows.Media;

namespace VisualComputerUse.Windows;

internal static class CursorArtwork
{
    internal const double PointerSize = 30;

    internal static void DrawTrail(DrawingContext context, IReadOnlyList<Point> points)
    {
        if (points.Count < 2)
            return;

        var start = Math.Max(1, points.Count - 30);
        for (var index = start; index < points.Count; index++)
        {
            var progress = (double)(index - start + 1) / Math.Max(1, points.Count - start);
            var color = Blend(Color.FromRgb(103, 89, 255), Color.FromRgb(37, 214, 255), progress);
            var brush = new SolidColorBrush(Color.FromArgb((byte)(25 + 115 * progress), color.R, color.G, color.B));
            brush.Freeze();
            var pen = new Pen(brush, 1.2 + progress * 3.4)
            {
                StartLineCap = PenLineCap.Round,
                EndLineCap = PenLineCap.Round
            };
            pen.Freeze();
            context.DrawLine(pen, points[index - 1], points[index]);
        }
    }

    internal static void DrawPointer(DrawingContext context, Point hotspot, double size = PointerSize)
    {
        var scale = size / 30.0;
        var coreRadius = Math.Max(8, size * 0.34);
        var orbitRadius = Math.Max(11, size * 0.46);
        var fogRadius = Math.Max(16, size * 0.70);
        var glow = new RadialGradientBrush
        {
            GradientStops =
            {
                new GradientStop(Color.FromArgb(85, 76, 224, 255), 0),
                new GradientStop(Color.FromArgb(34, 108, 76, 255), 0.48),
                new GradientStop(Color.FromArgb(0, 45, 76, 255), 1)
            }
        };
        glow.Freeze();
        context.DrawEllipse(glow, null, hotspot, fogRadius, fogRadius);

        var fill = new LinearGradientBrush(
            Color.FromArgb(248, 13, 20, 46),
            Color.FromArgb(245, 42, 31, 87),
            new Point(0, 1),
            new Point(1, 0));
        fill.Freeze();
        var outline = new Pen(new SolidColorBrush(Color.FromArgb(190, 220, 240, 255)), Math.Max(1, size * 0.038));
        outline.Freeze();
        context.DrawEllipse(fill, outline, hotspot, coreRadius, coreRadius);

        DrawArc(
            context,
            hotspot,
            orbitRadius,
            -Math.PI * 0.18,
            Math.PI * 0.56,
            Color.FromArgb(250, 71, 237, 255),
            Math.Max(2, size * 0.075));
        DrawArc(
            context,
            hotspot,
            orbitRadius,
            Math.PI * 0.82,
            Math.PI * 1.48,
            Color.FromArgb(240, 140, 97, 255),
            Math.Max(2, size * 0.075));

        var tickPen = new Pen(
            new SolidColorBrush(Color.FromArgb(235, 230, 250, 255)),
            Math.Max(1.2, size * 0.045))
        {
            StartLineCap = PenLineCap.Round,
            EndLineCap = PenLineCap.Round
        };
        tickPen.Freeze();
        var tickInner = coreRadius * 0.52;
        var tickOuter = coreRadius * 0.86;
        for (var angle = 0.0; angle < Math.PI * 2; angle += Math.PI / 2)
        {
            context.DrawLine(
                tickPen,
                new Point(hotspot.X + Math.Cos(angle) * tickInner, hotspot.Y + Math.Sin(angle) * tickInner),
                new Point(hotspot.X + Math.Cos(angle) * tickOuter, hotspot.Y + Math.Sin(angle) * tickOuter));
        }

        var hotspotRadius = Math.Max(2.5, size * 0.082);
        context.DrawEllipse(
            new SolidColorBrush(Color.FromArgb(65, 56, 235, 255)),
            null,
            hotspot,
            hotspotRadius * 2.2,
            hotspotRadius * 2.2);
        context.DrawEllipse(
            new SolidColorBrush(Color.FromRgb(56, 235, 255)),
            new Pen(Brushes.White, Math.Max(1, size * 0.036)),
            hotspot,
            hotspotRadius,
            hotspotRadius);
    }

    private static void DrawArc(
        DrawingContext context,
        Point center,
        double radius,
        double startAngle,
        double endAngle,
        Color color,
        double width)
    {
        var start = new Point(
            center.X + Math.Cos(startAngle) * radius,
            center.Y + Math.Sin(startAngle) * radius);
        var end = new Point(
            center.X + Math.Cos(endAngle) * radius,
            center.Y + Math.Sin(endAngle) * radius);
        var geometry = new StreamGeometry();
        using (var stream = geometry.Open())
        {
            stream.BeginFigure(start, false, false);
            stream.ArcTo(
                end,
                new Size(radius, radius),
                0,
                endAngle - startAngle > Math.PI,
                SweepDirection.Clockwise,
                true,
                false);
        }
        geometry.Freeze();
        var pen = new Pen(new SolidColorBrush(color), width)
        {
            StartLineCap = PenLineCap.Round,
            EndLineCap = PenLineCap.Round
        };
        pen.Freeze();
        context.DrawGeometry(null, pen, geometry);
    }

    internal static void DrawOffscreenIndicator(DrawingContext context, Point point, double angle, double radius)
    {
        var glow = new RadialGradientBrush
        {
            GradientStops =
            {
                new GradientStop(Color.FromArgb(220, 69, 226, 255), 0),
                new GradientStop(Color.FromArgb(0, 96, 92, 255), 1)
            }
        };
        glow.Freeze();
        context.DrawEllipse(glow, null, point, radius * 1.45, radius * 1.45);

        var direction = new Vector(Math.Cos(angle), Math.Sin(angle));
        var normal = new Vector(-direction.Y, direction.X);
        var tip = point + direction * radius;
        var back = point - direction * radius * 0.55;
        var geometry = new StreamGeometry();
        using (var stream = geometry.Open())
        {
            stream.BeginFigure(tip, true, true);
            stream.LineTo(back + normal * radius * 0.55, true, false);
            stream.LineTo(back - normal * radius * 0.55, true, false);
        }
        geometry.Freeze();
        context.DrawGeometry(new SolidColorBrush(Color.FromRgb(48, 220, 255)), new Pen(Brushes.White, 1.5), geometry);
    }

    private static Color Blend(Color start, Color end, double amount)
    {
        amount = Math.Clamp(amount, 0, 1);
        return Color.FromRgb(
            (byte)(start.R + (end.R - start.R) * amount),
            (byte)(start.G + (end.G - start.G) * amount),
            (byte)(start.B + (end.B - start.B) * amount));
    }
}
