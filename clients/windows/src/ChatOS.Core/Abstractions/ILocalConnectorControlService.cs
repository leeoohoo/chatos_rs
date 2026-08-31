using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface ILocalConnectorPairingTicketService
{
    Task<string> IssueAsync(CancellationToken cancellationToken = default);
}

public interface ILocalConnectorControlService
{
    Task<LocalConnectorStatus> GetStatusAsync(CancellationToken cancellationToken = default);

    Task<LocalConnectorStatus> PairAsync(
        LocalConnectorPairingDraft draft,
        string ticket,
        CancellationToken cancellationToken = default);

    Task DisconnectAsync(CancellationToken cancellationToken = default);
}
