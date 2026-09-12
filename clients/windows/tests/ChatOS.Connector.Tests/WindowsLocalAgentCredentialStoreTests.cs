using ChatOS.Connector.LocalAgent;
using System.Security.Cryptography;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentCredentialStoreTests
{
    [Theory]
    [InlineData("")]
    [InlineData(" padded")]
    [InlineData("line\nbreak")]
    public async Task RejectsUnsafeCredentialReferences(string reference)
    {
        var store = new WindowsLocalAgentCredentialStore();

        await Assert.ThrowsAsync<ArgumentException>(async () =>
            await store.SaveCredentialAsync("user-1", reference, "secret"));
    }

    [Fact]
    public async Task DpapiDeviceKeyRoundTripsOnlyForTheSameAccountAndReferenceOnWindows()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        var root = Path.Combine(Path.GetTempPath(), $"chatos-local-agent-{Guid.NewGuid():N}");
        var store = new WindowsLocalAgentCredentialStore(root);
        var key = Enumerable.Range(0, 32).Select(value => (byte)value).ToArray();
        try
        {
            await store.SaveDeviceKeyAsync("user-1", "sqlite-key", key);

            Assert.Equal(key, await store.LoadDeviceKeyAsync("user-1", "sqlite-key"));
            await Assert.ThrowsAsync<CryptographicException>(async () =>
                await store.LoadDeviceKeyAsync("user-2", "sqlite-key"));
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }
}
