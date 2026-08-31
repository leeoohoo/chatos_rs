using System.IO;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace VisualComputerUse.Windows;

internal sealed class StdioMcpServer(McpService service)
{
    private readonly Stream input = Console.OpenStandardInput();
    private readonly Stream output = Console.OpenStandardOutput();
    private readonly SemaphoreSlim writeGate = new(1, 1);

    internal async Task RunAsync(CancellationToken cancellationToken = default)
    {
        using var reader = new StreamReader(input, new UTF8Encoding(false), false, 64 * 1024, leaveOpen: true);
        while (!cancellationToken.IsCancellationRequested)
        {
            var line = await reader.ReadLineAsync(cancellationToken).ConfigureAwait(false);
            if (line is null)
                return;
            if (string.IsNullOrWhiteSpace(line))
                continue;
            JsonObject? request;
            try
            {
                request = JsonNode.Parse(line) as JsonObject;
            }
            catch (Exception error)
            {
                await WriteAsync(Error(null, -32700, $"Parse error: {error.Message}"), cancellationToken).ConfigureAwait(false);
                continue;
            }
            if (request is null)
            {
                await WriteAsync(Error(null, -32600, "Invalid JSON-RPC request."), cancellationToken).ConfigureAwait(false);
                continue;
            }
            if (request["id"] is null)
                continue;
            var response = await HandleAsync(request).ConfigureAwait(false);
            await WriteAsync(response, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task<JsonObject> HandleAsync(JsonObject request)
    {
        var id = request["id"]?.DeepClone();
        var method = request["method"]?.GetValue<string>();
        try
        {
            JsonNode result = method switch
            {
                "initialize" => service.Initialize(request["params"] as JsonObject),
                "ping" => new JsonObject(),
                "tools/list" => service.ListTools(),
                "tools/call" => await HandleToolCallAsync(request["params"] as JsonObject).ConfigureAwait(false),
                _ => throw new RpcException(-32601, $"Method not found: {method}")
            };
            return new JsonObject
            {
                ["jsonrpc"] = "2.0",
                ["id"] = id,
                ["result"] = result
            };
        }
        catch (RpcException error)
        {
            return Error(id, error.Code, error.Message);
        }
        catch (Exception error)
        {
            return Error(id, -32603, error.Message);
        }
    }

    private Task<JsonObject> HandleToolCallAsync(JsonObject? parameters)
    {
        var name = parameters?["name"]?.GetValue<string>()
            ?? throw new RpcException(-32602, "tools/call requires params.name.");
        var arguments = parameters?["arguments"] as JsonObject ?? new JsonObject();
        return service.CallToolAsync(name, arguments);
    }

    private async Task WriteAsync(JsonObject response, CancellationToken cancellationToken)
    {
        var bytes = Encoding.UTF8.GetBytes(response.ToJsonString(McpService.JsonOptions) + "\n");
        await writeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await output.WriteAsync(bytes, cancellationToken).ConfigureAwait(false);
            await output.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            writeGate.Release();
        }
    }

    private static JsonObject Error(JsonNode? id, int code, string message) => new()
    {
        ["jsonrpc"] = "2.0",
        ["id"] = id?.DeepClone(),
        ["error"] = new JsonObject { ["code"] = code, ["message"] = message }
    };

    private sealed class RpcException(int code, string message) : Exception(message)
    {
        internal int Code { get; } = code;
    }
}
