using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Approval;

internal sealed record ApprovalModelRuntimeConfiguration(
    string ModelConfigId,
    string Provider,
    Uri BaseUri,
    string ApiKey,
    string Model,
    string? ThinkingLevel,
    double Temperature,
    int MaxOutputTokens,
    int MaximumTransientRetries,
    string SystemPrompt,
    long PromptRevision,
    long PromptBundleVersion,
    string CapabilityPolicyRevision);

internal sealed class ApprovalModelRuntimeConfigurationService(
    IConnectorModelSettingsStore settingsStore,
    ConnectorRuntimeContext runtime,
    IConnectorGatewayClient gateway)
{
    internal const string AgentKey = "local_connector_command_approval_agent";
    private const int MaximumPromptUtf8Bytes = 256 * 1024;

    public async Task<ApprovalModelRuntimeConfiguration> ResolveAsync(
        CancellationToken cancellationToken = default)
    {
        var settings = (await settingsStore.LoadAsync(cancellationToken).ConfigureAwait(false)).Normalize();
        var modelConfigId = settings.CommandApprovalModelConfigId
            ?? throw new InvalidOperationException("The Windows approval model is not selected.");
        var session = await runtime.SessionConfigurationAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new InvalidOperationException("The local connector is not paired.");
        var owner = runtime.Snapshot.State?.User.Id
            ?? throw new InvalidOperationException("The local connector owner is unavailable.");

        var modelTask = gateway.GetModelConfigAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            modelConfigId,
            includeSecret: true,
            cancellationToken);
        var promptTask = gateway.GetAgentPromptBundleAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken);
        var capabilityTask = gateway.GetAgentCapabilityAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            AgentKey,
            cancellationToken);
        await Task.WhenAll(modelTask, promptTask, capabilityTask).ConfigureAwait(false);

        var model = modelTask.Result;
        if (!string.Equals(model.Id, modelConfigId, StringComparison.Ordinal) || !model.Enabled ||
            !model.TaskEnabled ||
            string.IsNullOrWhiteSpace(model.Model) || string.IsNullOrWhiteSpace(model.ApiKey))
        {
            throw new InvalidOperationException("The selected approval model is disabled or has no local API key.");
        }

        var capability = capabilityTask.Result;
        if (!capability.AgentEnabled ||
            !string.Equals(capability.AgentKey, AgentKey, StringComparison.Ordinal) ||
            !string.Equals(capability.OwnerUserId, owner, StringComparison.Ordinal) ||
            string.IsNullOrWhiteSpace(capability.PolicyRevision))
        {
            throw new InvalidOperationException("The approval Agent capability policy is unavailable.");
        }

        var provider = NormalizeProvider(model.Provider);
        var vendor = NormalizePromptVendor(model.PromptVendor, provider);
        var bundle = promptTask.Result;
        var prompt = bundle.Prompts.FirstOrDefault(value =>
            string.Equals(value.AgentKey, AgentKey, StringComparison.Ordinal) &&
            string.Equals(value.Vendor, vendor, StringComparison.OrdinalIgnoreCase));
        if (bundle.BundleVersion <= 0 || prompt is null || prompt.Revision <= 0 ||
            string.IsNullOrWhiteSpace(prompt.Content) ||
            Encoding.UTF8.GetByteCount(prompt.Content) > MaximumPromptUtf8Bytes ||
            !ChecksumMatches(prompt.Content, prompt.Checksum))
        {
            throw new InvalidOperationException("The approval Agent Prompt bundle is missing or invalid.");
        }

        var baseUri = ResolveBaseUri(model.BaseUrl, provider);
        return new ApprovalModelRuntimeConfiguration(
            model.Id,
            provider,
            baseUri,
            model.ApiKey.Trim(),
            model.Model.Trim(),
            model.ThinkingLevel?.Trim(),
            model.Temperature ?? 0,
            Math.Clamp(model.MaxOutputTokens ?? 1_200, 256, 4_096),
            Math.Min(settings.ModelRequestMaxRetries, 1),
            prompt.Content,
            prompt.Revision,
            bundle.BundleVersion,
            capability.PolicyRevision);
    }

    private static Uri ResolveBaseUri(string? value, string provider)
    {
        value = string.IsNullOrWhiteSpace(value) ? provider switch
        {
            "deepseek" => "https://api.deepseek.com/v1",
            "kimi" => "https://api.moonshot.cn/v1",
            "glm" => "https://open.bigmodel.cn/api/paas/v4",
            _ => "https://api.openai.com/v1",
        } : value.Trim();
        if (!Uri.TryCreate(value, UriKind.Absolute, out var uri) ||
            uri.Scheme is not ("https" or "http") ||
            uri.Scheme == "http" && !uri.IsLoopback ||
            !string.IsNullOrEmpty(uri.UserInfo) || !string.IsNullOrEmpty(uri.Fragment))
        {
            throw new InvalidOperationException("The approval model base URL is unsafe.");
        }

        return new Uri(uri.AbsoluteUri.TrimEnd('/') + "/", UriKind.Absolute);
    }

    private static string NormalizeProvider(string value) =>
        value.Trim().ToLowerInvariant().Replace('-', '_') switch
        {
            "openai" or "gpt" => "gpt",
            "moonshot" or "kimik2" or "kimi" => "kimi",
            "zhipu" or "zhipuai" or "zai" or "chatglm" or "glm" => "glm",
            "deepseek" => "deepseek",
            _ => throw new InvalidOperationException("The approval model provider is unsupported."),
        };

    private static string NormalizePromptVendor(string? explicitVendor, string provider)
    {
        var vendor = string.IsNullOrWhiteSpace(explicitVendor) ? provider : explicitVendor.Trim().ToLowerInvariant();
        return vendor switch
        {
            "gpt" or "openai" => "gpt",
            "deepseek" => "deepseek",
            "kimi" or "moonshot" => "kimi",
            "glm" or "zhipu" or "zai" => "glm",
            _ => throw new InvalidOperationException("The approval Agent Prompt vendor is unsupported."),
        };
    }

    private static bool ChecksumMatches(string content, string checksum)
    {
        var expected = "sha256:" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(content)))
            .ToLowerInvariant();
        var left = Encoding.ASCII.GetBytes(expected);
        var right = Encoding.ASCII.GetBytes(checksum.Trim().ToLowerInvariant());
        return left.Length == right.Length && CryptographicOperations.FixedTimeEquals(left, right);
    }
}
