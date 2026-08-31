namespace ChatOS.Core.Abstractions;

public sealed record ConnectorModelSettings(
    int ModelRequestMaxRetries,
    string? CommandApprovalModelConfigId)
{
    public static ConnectorModelSettings Default { get; } = new(5, null);

    public ConnectorModelSettings Normalize() => this with
    {
        ModelRequestMaxRetries = Math.Clamp(ModelRequestMaxRetries, 0, 10),
        CommandApprovalModelConfigId = string.IsNullOrWhiteSpace(CommandApprovalModelConfigId)
            ? null
            : CommandApprovalModelConfigId.Trim(),
    };
}

public interface IConnectorModelSettingsStore
{
    Task<ConnectorModelSettings> LoadAsync(CancellationToken cancellationToken = default);

    Task SaveAsync(
        ConnectorModelSettings settings,
        CancellationToken cancellationToken = default);
}
