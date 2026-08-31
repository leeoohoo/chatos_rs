using System.Text.Json;

namespace ChatOS.Connector.Relay;

public sealed class RelayDispatcher
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly IReadOnlyList<IRelayRequestHandler> _handlers;
    private readonly IReadOnlyList<IRelayOneWayHandler> _oneWayHandlers;
    private readonly IRelayRequestVerifier _verifier;

    public RelayDispatcher(
        IEnumerable<IRelayRequestHandler> handlers,
        IRelayRequestVerifier verifier,
        IEnumerable<IRelayOneWayHandler>? oneWayHandlers = null)
    {
        _handlers = handlers.ToArray();
        _verifier = verifier;
        _oneWayHandlers = oneWayHandlers?.ToArray() ?? [];
    }

    public async Task<bool> DispatchOneWayAsync(
        string payload,
        CancellationToken cancellationToken = default)
    {
        try
        {
            var request = JsonSerializer.Deserialize<RelayRequest>(payload, JsonOptions)
                ?? throw new RelayRequestException(400, "Relay request body is empty.");
            ValidateEnvelope(request);
            var handler = _oneWayHandlers.SingleOrDefault(candidate => candidate.CanHandle(request.Type));
            if (handler is null)
            {
                return false;
            }

            await _verifier.VerifyAsync(request, cancellationToken).ConfigureAwait(false);
            await handler.HandleAsync(request, cancellationToken).ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            return false;
        }
    }

    public async Task<RelayResponse> DispatchAsync(
        string payload,
        CancellationToken cancellationToken = default)
    {
        RelayRequest? request = null;
        try
        {
            request = JsonSerializer.Deserialize<RelayRequest>(payload, JsonOptions)
                ?? throw new RelayRequestException(400, "Relay request body is empty.");
            ValidateEnvelope(request);

            var handler = _handlers.SingleOrDefault(candidate => candidate.CanHandle(request.Type))
                ?? throw new RelayRequestException(400, $"Unsupported Relay request: {request.Type}");

            await _verifier.VerifyAsync(request, cancellationToken).ConfigureAwait(false);
            var result = await handler.HandleAsync(request, cancellationToken).ConfigureAwait(false);
            return new RelayResponse
            {
                Type = handler.ResponseType(request.Type),
                RequestId = request.RequestId,
                Status = result.Status,
                Body = result.Body,
            };
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            var status = exception is RelayRequestException relayError
                ? relayError.StatusCode
                : 400;
            return new RelayResponse
            {
                Type = ResponseTypeFor(request?.Type),
                RequestId = request?.RequestId ?? TryReadString(payload, "request_id") ?? string.Empty,
                Status = status,
                Body = JsonSerializer.SerializeToElement(new { error = exception.Message }, JsonOptions),
            };
        }
    }

    private static void ValidateEnvelope(RelayRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.Type))
        {
            throw new RelayRequestException(400, "Relay request type is required.");
        }

        if (string.IsNullOrWhiteSpace(request.RequestId))
        {
            throw new RelayRequestException(400, "Relay request id is required.");
        }

        if (string.IsNullOrWhiteSpace(request.WorkspaceId) &&
            request.Type is not ("plugin_prepare_request" or "plugin_execute_request" or "plugin_cancel_request"))
        {
            throw new RelayRequestException(400, "Relay workspace id is required.");
        }
    }

    private static string ResponseTypeFor(string? requestType) => requestType switch
    {
        "workspace_directory_list_request" => "workspace_directory_list_response",
        "workspace_directory_create_request" => "workspace_directory_create_response",
        "terminal_exec_request" => "terminal_response",
        "terminal_session_create_request" => "terminal_session_create_response",
        "plugin_prepare_request" => "plugin_prepare_response",
        "plugin_execute_request" => "plugin_execute_response",
        "plugin_cancel_request" => "plugin_cancel_response",
        "mcp" => "mcp",
        _ => "workspace_filesystem_response",
    };

    private static string? TryReadString(string payload, string property)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            return document.RootElement.TryGetProperty(property, out var value) &&
                value.ValueKind is JsonValueKind.String
                    ? value.GetString()
                    : null;
        }
        catch (JsonException)
        {
            return null;
        }
    }
}
