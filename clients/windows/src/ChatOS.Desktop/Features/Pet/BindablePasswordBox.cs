using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Pet;

public sealed class BindablePasswordBox : PasswordBox
{
    public static readonly DependencyProperty BoundPasswordProperty = DependencyProperty.Register(
        nameof(BoundPassword),
        typeof(string),
        typeof(BindablePasswordBox),
        new PropertyMetadata(string.Empty, OnBoundPasswordChanged));

    public BindablePasswordBox() => PasswordChanged += OnPasswordChanged;

    public string BoundPassword
    {
        get => (string)GetValue(BoundPasswordProperty);
        set => SetValue(BoundPasswordProperty, value);
    }

    private static void OnBoundPasswordChanged(DependencyObject sender, DependencyPropertyChangedEventArgs args)
    {
        if (sender is BindablePasswordBox passwordBox)
        {
            var value = args.NewValue as string ?? string.Empty;
            if (!string.Equals(passwordBox.Password, value, StringComparison.Ordinal))
            {
                passwordBox.Password = value;
            }
        }
    }

    private void OnPasswordChanged(object sender, RoutedEventArgs args)
    {
        if (!string.Equals(BoundPassword, Password, StringComparison.Ordinal))
        {
            BoundPassword = Password;
        }
    }
}
