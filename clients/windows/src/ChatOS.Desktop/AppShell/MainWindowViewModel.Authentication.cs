using ChatOS.Api.Http;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Desktop.AppShell;

public sealed partial class MainWindowViewModel
{
    private CancellationTokenSource? _registrationCountdownCancellation;

    [ObservableProperty]
    private bool _isRegistrationMode;

    [ObservableProperty]
    private string _inviteCode = string.Empty;

    [ObservableProperty]
    private string _verificationCode = string.Empty;

    [ObservableProperty]
    private string _confirmPassword = string.Empty;

    [ObservableProperty]
    private bool _isSendingRegistrationCode;

    [ObservableProperty]
    private int _registrationCodeCountdown;

    [ObservableProperty]
    private string? _registrationMessage;

    public string RegistrationCodeActionTitle => IsSendingRegistrationCode
        ? Localization.Text("发送中", "Sending")
        : RegistrationCodeCountdown > 0
            ? $"{RegistrationCodeCountdown}s"
            : Localization.Text("发送验证码", "Send code");

    private bool CanSendRegistrationCode() =>
        !IsBusy && !IsSendingRegistrationCode && RegistrationCodeCountdown == 0 &&
        Username.Trim().Length > 0 && InviteCode.Trim().Length > 0;

    private bool CanRegister() =>
        !IsBusy && Username.Trim().Length > 0 && InviteCode.Trim().Length > 0 &&
        VerificationCode.Trim().Length > 0 && Password.Length > 0 && ConfirmPassword.Length > 0;

    [RelayCommand]
    private void ShowRegistration()
    {
        IsRegistrationMode = true;
        ClearRegistrationFeedback(clearIdentity: false);
    }

    [RelayCommand]
    private void ShowLogin()
    {
        IsRegistrationMode = false;
        ClearRegistrationFeedback(clearIdentity: false);
    }

    [RelayCommand(CanExecute = nameof(CanSendRegistrationCode))]
    private async Task SendRegistrationCodeAsync()
    {
        if (!IsLikelyEmail(Username))
        {
            ErrorMessage = Localization.Text("请输入有效的邮箱地址。", "Enter a valid email address.");
            return;
        }

        IsSendingRegistrationCode = true;
        ErrorMessage = null;
        RegistrationMessage = null;
        try
        {
            var delivery = await _authenticationService.SendRegistrationCodeAsync(Username, InviteCode);
            RegistrationMessage = Localization.Text(
                "验证码已发送，请查看邮箱。",
                "The verification code was sent. Check your email.");
            StartRegistrationCountdown(delivery.ResendAfterSeconds);
        }
        catch (Exception exception)
        {
            ErrorMessage = FriendlyRegistrationError(exception);
        }
        finally
        {
            IsSendingRegistrationCode = false;
        }
    }

    [RelayCommand(CanExecute = nameof(CanRegister))]
    private async Task RegisterAsync()
    {
        var validationMessage = RegistrationValidationMessage();
        if (validationMessage is not null)
        {
            ErrorMessage = validationMessage;
            return;
        }

        IsBusy = true;
        ErrorMessage = null;
        RegistrationMessage = null;
        try
        {
            var authenticationGeneration = AccountGeneration;
            var session = await _authenticationService.RegisterAsync(
                Username, Password, InviteCode, VerificationCode);
            if (authenticationGeneration != AccountGeneration) return;
            StopRegistrationCountdown();
            Password = ConfirmPassword = VerificationCode = string.Empty;
            ApplySession(session);
            await ReloadWorkspaceCoreAsync();
        }
        catch (OperationCanceledException) { }
        catch (Exception exception)
        {
            ErrorMessage = FriendlyRegistrationError(exception);
        }
        finally
        {
            IsBusy = false;
        }
    }

    partial void OnUsernameChanged(string value) => NotifyRegistrationCommands();
    partial void OnPasswordChanged(string value) => NotifyRegistrationCommands();
    partial void OnInviteCodeChanged(string value) => NotifyRegistrationCommands();
    partial void OnVerificationCodeChanged(string value) => NotifyRegistrationCommands();
    partial void OnConfirmPasswordChanged(string value) => NotifyRegistrationCommands();
    partial void OnIsBusyChanged(bool value) => NotifyRegistrationCommands();

    partial void OnIsSendingRegistrationCodeChanged(bool value)
    {
        OnPropertyChanged(nameof(RegistrationCodeActionTitle));
        NotifyRegistrationCommands();
    }

    partial void OnRegistrationCodeCountdownChanged(int value)
    {
        OnPropertyChanged(nameof(RegistrationCodeActionTitle));
        NotifyRegistrationCommands();
    }

    private void NotifyRegistrationCommands()
    {
        SendRegistrationCodeCommand.NotifyCanExecuteChanged();
        RegisterCommand.NotifyCanExecuteChanged();
    }

    private void ClearRegistrationFeedback(bool clearIdentity)
    {
        Password = ConfirmPassword = VerificationCode = string.Empty;
        ErrorMessage = RegistrationMessage = null;
        if (!clearIdentity) return;
        Username = InviteCode = string.Empty;
        StopRegistrationCountdown();
    }

    private string? RegistrationValidationMessage()
    {
        if (!IsLikelyEmail(Username))
            return Localization.Text("请输入有效的邮箱地址。", "Enter a valid email address.");
        if (InviteCode.Trim().Length == 0)
            return Localization.Text("请输入邀请码。", "Enter an invitation code.");
        var code = VerificationCode.Trim();
        if (code.Length != 6 || code.Any(character => !char.IsAsciiDigit(character)))
            return Localization.Text("请输入 6 位邮箱验证码。", "Enter the 6-digit email verification code.");
        if (Password.Length < 6)
            return Localization.Text("密码至少需要 6 个字符。", "The password must contain at least 6 characters.");
        if (!string.Equals(Password, ConfirmPassword, StringComparison.Ordinal))
            return Localization.Text("两次输入的密码不一致。", "The passwords do not match.");
        return null;
    }

    private static bool IsLikelyEmail(string value)
    {
        var parts = value.Trim().Split('@', StringSplitOptions.RemoveEmptyEntries);
        return parts.Length == 2 && parts[1].Contains('.', StringComparison.Ordinal);
    }

    private string FriendlyRegistrationError(Exception exception)
    {
        var message = exception.Message;
        var normalized = message.ToLowerInvariant();
        return normalized switch
        {
            var value when value.Contains("email already registered", StringComparison.Ordinal) =>
                Localization.Text("这个邮箱已经注册，可以直接登录。", "This email is already registered. Sign in instead."),
            var value when value.Contains("invite code is invalid", StringComparison.Ordinal) =>
                Localization.Text("邀请码无效或已经失效。", "The invitation code is invalid or expired."),
            var value when value.Contains("verification code is invalid or expired", StringComparison.Ordinal) =>
                Localization.Text("邮箱验证码错误或已经过期。", "The email verification code is invalid or expired."),
            var value when value.Contains("verification code was sent recently", StringComparison.Ordinal) =>
                Localization.Text("验证码刚刚发送，请稍后再试。", "A code was sent recently. Try again later."),
            var value when value.Contains("too many verification emails", StringComparison.Ordinal) =>
                Localization.Text("验证码发送次数过多，请稍后再试。", "Too many codes were requested. Try again later."),
            var value when value.Contains("email format is invalid", StringComparison.Ordinal) =>
                Localization.Text("请输入有效的邮箱地址。", "Enter a valid email address."),
            var value when value.Contains("temporarily unavailable", StringComparison.Ordinal) =>
                Localization.Text("注册服务暂时不可用，请稍后再试。", "Registration is temporarily unavailable."),
            _ when exception is ChatOSApiException => message,
            _ => message,
        };
    }

    private void StartRegistrationCountdown(int seconds)
    {
        StopRegistrationCountdown();
        RegistrationCodeCountdown = Math.Max(1, seconds);
        _registrationCountdownCancellation = new CancellationTokenSource();
        _ = RunRegistrationCountdownAsync(_registrationCountdownCancellation.Token);
    }

    private async Task RunRegistrationCountdownAsync(CancellationToken cancellationToken)
    {
        try
        {
            while (RegistrationCodeCountdown > 0)
            {
                await Task.Delay(TimeSpan.FromSeconds(1), cancellationToken);
                if (RegistrationCodeCountdown > 0) RegistrationCodeCountdown--;
            }
        }
        catch (OperationCanceledException) { }
    }

    private void StopRegistrationCountdown()
    {
        _registrationCountdownCancellation?.Cancel();
        _registrationCountdownCancellation?.Dispose();
        _registrationCountdownCancellation = null;
        RegistrationCodeCountdown = 0;
    }
}
