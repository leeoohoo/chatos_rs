using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using Windows.Security.Credentials;

namespace ChatOS.Connector.LocalAgent;

internal interface IWindowsLocalAgentCredentialStore
{
    ValueTask<string?> LoadCredentialAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default);

    Task<byte[]?> LoadDeviceKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default);
}

public sealed class WindowsLocalAgentCredentialStore : IWindowsLocalAgentCredentialStore
{
    public const string CredentialResource = "ChatOS.Windows.LocalAgent.Credentials.v1";
    private const int MaximumSecretBytes = 64 * 1024;
    private readonly Lazy<PasswordVault> _vault;
    private readonly string _protectedKeyDirectory;

    public WindowsLocalAgentCredentialStore(string? protectedKeyDirectory = null)
    {
        _vault = new Lazy<PasswordVault>(() => new PasswordVault());
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        _protectedKeyDirectory = Path.GetFullPath(protectedKeyDirectory
            ?? Path.Combine(appData, "ChatOS", "LocalAgent", "ProtectedKeys"));
    }

    internal WindowsLocalAgentCredentialStore(
        PasswordVault vault,
        string? protectedKeyDirectory = null)
    {
        _vault = new Lazy<PasswordVault>(() => vault);
        var appData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        _protectedKeyDirectory = Path.GetFullPath(protectedKeyDirectory
            ?? Path.Combine(appData, "ChatOS", "LocalAgent", "ProtectedKeys"));
    }

    /// Stores account-style secrets such as the Model Gateway bearer token or
    /// PostgreSQL credential envelope in Windows Credential Manager.
    public ValueTask SaveCredentialAsync(
        string accountId,
        string reference,
        string secret,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var account = AccountKey(accountId, reference);
        if (string.IsNullOrEmpty(secret) || Encoding.UTF8.GetByteCount(secret) > MaximumSecretBytes)
        {
            throw new ArgumentException("Local Agent credential is empty or too large.", nameof(secret));
        }

        DeleteVaultCredential(account);
        _vault.Value.Add(new PasswordCredential(CredentialResource, account, secret));
        return ValueTask.CompletedTask;
    }

    public ValueTask<string?> LoadCredentialAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var account = AccountKey(accountId, reference);
        try
        {
            var credential = _vault.Value.Retrieve(CredentialResource, account);
            credential.RetrievePassword();
            return ValueTask.FromResult<string?>(credential.Password);
        }
        catch
        {
            return ValueTask.FromResult<string?>(null);
        }
    }

    public ValueTask DeleteCredentialAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        DeleteVaultCredential(AccountKey(accountId, reference));
        return ValueTask.CompletedTask;
    }

    /// Stores fixed local device keys with DPAPI CurrentUser and additional
    /// account/reference entropy. The protected blob can live in app data but
    /// is unusable by another Windows account and is authenticated by DPAPI.
    public async Task SaveDeviceKeyAsync(
        string accountId,
        string reference,
        ReadOnlyMemory<byte> key,
        CancellationToken cancellationToken = default)
    {
        EnsureWindows();
        var path = ProtectedKeyPath(accountId, reference);
        if (key.IsEmpty || key.Length > 4096)
        {
            throw new ArgumentException("Local Agent device key is empty or too large.", nameof(key));
        }

        EnsurePrivateKeyDirectory();
        var plaintext = key.ToArray();
        var entropy = Entropy(accountId, reference);
        byte[] protectedBytes;
        try
        {
            protectedBytes = Protect(plaintext, entropy);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(plaintext);
            CryptographicOperations.ZeroMemory(entropy);
        }

        var temporary = $"{path}.{Guid.NewGuid():N}.tmp";
        try
        {
            await File.WriteAllBytesAsync(temporary, protectedBytes, cancellationToken)
                .ConfigureAwait(false);
            File.Move(temporary, path, overwrite: true);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(protectedBytes);
            if (File.Exists(temporary))
            {
                File.Delete(temporary);
            }
        }
    }

    public async Task<byte[]?> LoadDeviceKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default)
    {
        EnsureWindows();
        var path = ProtectedKeyPath(accountId, reference);
        if (!File.Exists(path))
        {
            return null;
        }

        var information = new FileInfo(path);
        if (information.Attributes.HasFlag(FileAttributes.ReparsePoint)
            || information.Length <= 0
            || information.Length > MaximumSecretBytes)
        {
            throw new CryptographicException("Local Agent DPAPI key file is invalid.");
        }

        var protectedBytes = await File.ReadAllBytesAsync(path, cancellationToken).ConfigureAwait(false);
        var entropy = Entropy(accountId, reference);
        try
        {
            return Unprotect(protectedBytes, entropy);
        }
        finally
        {
            CryptographicOperations.ZeroMemory(protectedBytes);
            CryptographicOperations.ZeroMemory(entropy);
        }
    }

    public ValueTask DeleteDeviceKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var path = ProtectedKeyPath(accountId, reference);
        if (File.Exists(path))
        {
            File.Delete(path);
        }

        return ValueTask.CompletedTask;
    }

    private void DeleteVaultCredential(string account)
    {
        try
        {
            _vault.Value.Remove(_vault.Value.Retrieve(CredentialResource, account));
        }
        catch
        {
            // PasswordVault reports an absent credential as an exception.
        }
    }

    private string ProtectedKeyPath(string accountId, string reference)
    {
        var account = AccountKey(accountId, reference);
        var digest = SHA256.HashData(Encoding.UTF8.GetBytes(account));
        return Path.Combine(_protectedKeyDirectory, $"{Convert.ToHexString(digest).ToLowerInvariant()}.dpapi");
    }

    private void EnsurePrivateKeyDirectory()
    {
        Directory.CreateDirectory(_protectedKeyDirectory);
        var information = new DirectoryInfo(_protectedKeyDirectory);
        if (information.Attributes.HasFlag(FileAttributes.ReparsePoint))
        {
            throw new CryptographicException("Local Agent DPAPI key directory is a reparse point.");
        }
    }

    private static string AccountKey(string accountId, string reference)
    {
        ValidateIdentity(accountId, nameof(accountId));
        ValidateIdentity(reference, nameof(reference));
        return $"v1:{Encoding.UTF8.GetByteCount(accountId)}:{accountId}{reference}";
    }

    private static void ValidateIdentity(string value, string parameter)
    {
        if (string.IsNullOrWhiteSpace(value)
            || value != value.Trim()
            || value.Length > 512
            || value.Any(char.IsControl))
        {
            throw new ArgumentException("Local Agent credential reference is invalid.", parameter);
        }
    }

    private static byte[] Entropy(string accountId, string reference) =>
        SHA256.HashData(Encoding.UTF8.GetBytes(
            $"chatos-local-agent-dpapi-v1\n{AccountKey(accountId, reference)}"));

    private static void EnsureWindows()
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("DPAPI device keys require Windows.");
        }
    }

    private static byte[] Protect(byte[] plaintext, byte[] entropy) =>
        Transform(plaintext, entropy, NativeMethods.CryptProtectData);

    private static byte[] Unprotect(byte[] protectedBytes, byte[] entropy) =>
        Transform(protectedBytes, entropy, NativeMethods.CryptUnprotectData);

    private static byte[] Transform(byte[] input, byte[] entropy, CryptTransform transform)
    {
        var inputHandle = GCHandle.Alloc(input, GCHandleType.Pinned);
        var entropyHandle = GCHandle.Alloc(entropy, GCHandleType.Pinned);
        var inputBlob = new DataBlob(input.Length, inputHandle.AddrOfPinnedObject());
        var entropyBlob = new DataBlob(entropy.Length, entropyHandle.AddrOfPinnedObject());
        var outputBlob = new DataBlob();
        try
        {
            if (!transform(
                    ref inputBlob,
                    null,
                    ref entropyBlob,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    NativeMethods.CryptProtectUiForbidden,
                    out outputBlob))
            {
                throw new CryptographicException(Marshal.GetLastWin32Error());
            }

            var output = new byte[outputBlob.Length];
            Marshal.Copy(outputBlob.Data, output, 0, output.Length);
            return output;
        }
        finally
        {
            if (outputBlob.Data != IntPtr.Zero)
            {
                NativeMethods.LocalFree(outputBlob.Data);
            }

            entropyHandle.Free();
            inputHandle.Free();
        }
    }

    private delegate bool CryptTransform(
        ref DataBlob input,
        string? description,
        ref DataBlob entropy,
        IntPtr reserved,
        IntPtr prompt,
        uint flags,
        out DataBlob output);

    [StructLayout(LayoutKind.Sequential)]
    private struct DataBlob
    {
        public DataBlob(int length, IntPtr data)
        {
            Length = length;
            Data = data;
        }

        public int Length;
        public IntPtr Data;
    }

    private static class NativeMethods
    {
        internal const uint CryptProtectUiForbidden = 0x1;

        [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool CryptProtectData(
            ref DataBlob input,
            string? description,
            ref DataBlob entropy,
            IntPtr reserved,
            IntPtr prompt,
            uint flags,
            out DataBlob output);

        [DllImport("crypt32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool CryptUnprotectData(
            ref DataBlob input,
            string? description,
            ref DataBlob entropy,
            IntPtr reserved,
            IntPtr prompt,
            uint flags,
            out DataBlob output);

        [DllImport("kernel32.dll")]
        internal static extern IntPtr LocalFree(IntPtr memory);
    }
}
