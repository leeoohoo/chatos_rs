using System.Text.Json;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Crypto.Prng;
using Org.BouncyCastle.Pkcs;
using Org.BouncyCastle.Security;

if (args.Length is < 2 or > 3 || !string.Equals(args[0], "generate", StringComparison.OrdinalIgnoreCase))
{
    Console.Error.WriteLine("Usage: ChatOS.NetworkGuard.PolicyKeyTool generate <output-directory> [key-id]");
    return 2;
}

var outputDirectory = Path.GetFullPath(args[1]);
var keyId = args.Length == 3 ? args[2].Trim() : $"networkguard-{DateTime.UtcNow:yyyyMMdd}";
if (keyId.Length is < 1 or > 128 || keyId.Any(value => !char.IsAsciiLetterOrDigit(value) && value is not ('.' or '_' or '-')))
{
    Console.Error.WriteLine("Key id must contain only letters, numbers, dot, underscore or hyphen.");
    return 2;
}

Directory.CreateDirectory(outputDirectory);
var privateKeyPath = Path.Combine(outputDirectory, "controlled-network-signing-key.pk8");
var manifestPath = Path.Combine(outputDirectory, "controlled-network-signing-key.json");
if (File.Exists(privateKeyPath) || File.Exists(manifestPath))
{
    Console.Error.WriteLine("The target key files already exist; refusing to overwrite them.");
    return 3;
}

var random = new SecureRandom(new CryptoApiRandomGenerator());
var privateKey = new Ed25519PrivateKeyParameters(random);
var publicKey = privateKey.GeneratePublicKey();
var privateKeyInfo = PrivateKeyInfoFactory.CreatePrivateKeyInfo(privateKey);
await File.WriteAllBytesAsync(privateKeyPath, privateKeyInfo.GetEncoded());
var publicKeyText = "ed25519:" + Base64Url(publicKey.GetEncoded());
var manifest = new
{
    schema_version = 1,
    key_id = keyId,
    public_key = publicKeyText,
    private_key_path = privateKeyPath,
    created_at = DateTimeOffset.UtcNow,
};
await File.WriteAllTextAsync(
    manifestPath,
    JsonSerializer.Serialize(manifest, new JsonSerializerOptions { WriteIndented = true }));

Console.WriteLine(JsonSerializer.Serialize(manifest));
return 0;

static string Base64Url(byte[] value) => Convert.ToBase64String(value)
    .TrimEnd('=')
    .Replace('+', '-')
    .Replace('/', '_');
