using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Pet;

public sealed class PasswordBoxBinding : DependencyObject
{
    public static readonly DependencyProperty ValueProperty = DependencyProperty.RegisterAttached(
        "Value",
        typeof(string),
        typeof(PasswordBoxBinding),
        new PropertyMetadata(null, OnValueChanged));

    public static string? GetValue(DependencyObject target) =>
        target.GetValue(ValueProperty) as string;

    public static void SetValue(DependencyObject target, string? value) =>
        target.SetValue(ValueProperty, value);

    private static void OnValueChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is not PasswordBox passwordBox)
        {
            return;
        }

        passwordBox.PasswordChanged -= OnPasswordChanged;
        var value = args.NewValue as string ?? string.Empty;
        if (!string.Equals(passwordBox.Password, value, StringComparison.Ordinal))
        {
            passwordBox.Password = value;
        }
        passwordBox.PasswordChanged += OnPasswordChanged;
    }

    private static void OnPasswordChanged(object sender, RoutedEventArgs args)
    {
        if (sender is PasswordBox passwordBox
            && !string.Equals(GetValue(passwordBox), passwordBox.Password, StringComparison.Ordinal))
        {
            SetValue(passwordBox, passwordBox.Password);
        }
    }
}
