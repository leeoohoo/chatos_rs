using System.Runtime.InteropServices;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using WinRT.Interop;

namespace ChatOS.Desktop.Features.QuickSearch;

public enum WindowsGlobalUtilityAction
{
    Screenshot,
    ScreenRecording,
    ClipboardHistory,
    QuickSearch,
}

public sealed class WindowsGlobalHotKeyService : IDisposable
{
    private const int ScreenshotHotKeyId = 0x4348;
    private const int RecordingHotKeyId = 0x4349;
    private const int ClipboardHotKeyId = 0x434A;
    private const int QuickSearchHotKeyId = 0x434B;
    private const uint WmHotKey = 0x0312;
    private const uint ModAlt = 0x0001;
    private const uint ModControl = 0x0002;
    private const uint ModShift = 0x0004;
    private const uint ModNoRepeat = 0x4000;
    private const uint VirtualKeyA = 0x41;
    private const uint VirtualKeyE = 0x45;
    private const uint VirtualKeyQ = 0x51;
    private const uint VirtualKeySpace = 0x20;

    private static readonly IReadOnlyDictionary<WindowsGlobalUtilityAction, HotKeyDefinition> Definitions =
        new Dictionary<WindowsGlobalUtilityAction, HotKeyDefinition>
        {
            [WindowsGlobalUtilityAction.Screenshot] = new(
                ScreenshotHotKeyId, ModControl | ModShift, VirtualKeyA, "Ctrl+Shift+A"),
            [WindowsGlobalUtilityAction.ScreenRecording] = new(
                RecordingHotKeyId, ModControl | ModShift, VirtualKeyQ, "Ctrl+Shift+Q"),
            [WindowsGlobalUtilityAction.ClipboardHistory] = new(
                ClipboardHotKeyId, ModControl | ModShift, VirtualKeyE, "Ctrl+Shift+E"),
            [WindowsGlobalUtilityAction.QuickSearch] = new(
                QuickSearchHotKeyId, ModControl, VirtualKeySpace, "Ctrl+Space",
                ModControl | ModAlt, VirtualKeySpace, "Ctrl+Alt+Space"),
        };

    private readonly Dictionary<WindowsGlobalUtilityAction, WindowsGlobalHotKeyState> _states = [];
    private SubclassProcedure? _procedure;
    private IntPtr _windowHandle;
    private DispatcherQueue? _dispatcher;
    private AppPreferencesManager? _preferences;

    public event EventHandler<WindowsGlobalUtilityAction>? ActionPressed;

    public event EventHandler? ConfigurationChanged;

    public string RegisteredShortcut => GetState(WindowsGlobalUtilityAction.QuickSearch).ActiveShortcut ?? string.Empty;

    public WindowsGlobalHotKeyState GetState(WindowsGlobalUtilityAction action) =>
        _states.TryGetValue(action, out var state)
            ? state
            : new WindowsGlobalHotKeyState(false, false, Definitions[action].DisplayName, null);

    public void Register(Window owner, AppPreferencesManager preferences)
    {
        if (_windowHandle != IntPtr.Zero) return;
        _windowHandle = WindowNative.GetWindowHandle(owner);
        _dispatcher = owner.DispatcherQueue;
        _preferences = preferences;
        _procedure = WindowProcedure;
        if (!SetWindowSubclass(_windowHandle, _procedure, (UIntPtr)ScreenshotHotKeyId, UIntPtr.Zero))
            throw new InvalidOperationException("Unable to observe ChatOS global shortcuts.");

        preferences.Changed += OnPreferencesChanged;
        Reconfigure(preferences.Current);
    }

    public void Dispose()
    {
        if (_preferences is not null) _preferences.Changed -= OnPreferencesChanged;
        _preferences = null;
        UnregisterAll();
        if (_windowHandle != IntPtr.Zero && _procedure is not null)
            RemoveWindowSubclass(_windowHandle, _procedure, (UIntPtr)ScreenshotHotKeyId);
        _windowHandle = IntPtr.Zero;
        _procedure = null;
        _states.Clear();
    }

    private void OnPreferencesChanged(object? sender, AppPreferences preferences)
    {
        if (_dispatcher?.HasThreadAccess == true)
        {
            Reconfigure(preferences);
        }
        else
        {
            _dispatcher?.TryEnqueue(() => Reconfigure(preferences));
        }
    }

    private void Reconfigure(AppPreferences preferences)
    {
        if (_windowHandle == IntPtr.Zero) return;
        UnregisterAll();
        _states.Clear();

        foreach (var pair in Definitions)
        {
            var enabled = preferences.GlobalUtilitiesEnabled && IsActionEnabled(preferences, pair.Key);
            _states[pair.Key] = enabled ? Register(pair.Value) : new(
                false, false, pair.Value.DisplayName, null);
        }

        ChatOS.Desktop.StartupDiagnostics.RecordLifecycle(
            "global hotkeys configured: " + string.Join(", ", _states.Select(pair =>
                $"{pair.Key}={pair.Value.ActiveShortcut ?? (pair.Value.Enabled ? "conflict" : "disabled")}")));
        ConfigurationChanged?.Invoke(this, EventArgs.Empty);
    }

    private WindowsGlobalHotKeyState Register(HotKeyDefinition definition)
    {
        if (RegisterHotKey(
            _windowHandle,
            definition.Id,
            definition.Modifiers | ModNoRepeat,
            definition.VirtualKey))
        {
            return new(true, false, definition.DisplayName, definition.DisplayName);
        }

        if (definition.FallbackDisplayName is not null && RegisterHotKey(
            _windowHandle,
            definition.Id,
            definition.FallbackModifiers | ModNoRepeat,
            definition.FallbackVirtualKey))
        {
            return new(true, true, definition.DisplayName, definition.FallbackDisplayName);
        }

        return new(true, false, definition.DisplayName, null);
    }

    private void UnregisterAll()
    {
        if (_windowHandle == IntPtr.Zero) return;
        foreach (var definition in Definitions.Values) UnregisterHotKey(_windowHandle, definition.Id);
    }

    private static bool IsActionEnabled(AppPreferences preferences, WindowsGlobalUtilityAction action) =>
        action switch
        {
            WindowsGlobalUtilityAction.Screenshot => preferences.GlobalScreenshotEnabled,
            WindowsGlobalUtilityAction.ScreenRecording => preferences.GlobalRecordingEnabled,
            WindowsGlobalUtilityAction.ClipboardHistory => preferences.GlobalClipboardEnabled,
            WindowsGlobalUtilityAction.QuickSearch => preferences.GlobalQuickSearchEnabled,
            _ => false,
        };

    private IntPtr WindowProcedure(
        IntPtr window,
        uint message,
        UIntPtr wParam,
        IntPtr lParam,
        UIntPtr subclassId,
        UIntPtr referenceData)
    {
        if (message == WmHotKey)
        {
            var id = unchecked((int)wParam.ToUInt64());
            foreach (var pair in Definitions)
            {
                if (pair.Value.Id != id) continue;
                _ = _dispatcher?.TryEnqueue(() => ActionPressed?.Invoke(this, pair.Key));
                break;
            }
        }
        return DefSubclassProc(window, message, wParam, lParam);
    }

    private sealed record HotKeyDefinition(
        int Id,
        uint Modifiers,
        uint VirtualKey,
        string DisplayName,
        uint FallbackModifiers = 0,
        uint FallbackVirtualKey = 0,
        string? FallbackDisplayName = null);

    private delegate IntPtr SubclassProcedure(
        IntPtr window,
        uint message,
        UIntPtr wParam,
        IntPtr lParam,
        UIntPtr subclassId,
        UIntPtr referenceData);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint virtualKey);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UnregisterHotKey(IntPtr window, int id);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool SetWindowSubclass(
        IntPtr window,
        SubclassProcedure procedure,
        UIntPtr subclassId,
        UIntPtr referenceData);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool RemoveWindowSubclass(
        IntPtr window,
        SubclassProcedure procedure,
        UIntPtr subclassId);

    [DllImport("comctl32.dll")]
    private static extern IntPtr DefSubclassProc(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam);
}

public sealed record WindowsGlobalHotKeyState(
    bool Enabled,
    bool UsesFallback,
    string RequestedShortcut,
    string? ActiveShortcut)
{
    public bool IsRegistered => Enabled && ActiveShortcut is not null;
}
