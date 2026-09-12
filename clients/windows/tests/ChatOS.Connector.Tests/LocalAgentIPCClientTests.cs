using System.Buffers.Binary;
using System.Runtime.CompilerServices;
using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class LocalAgentIPCClientTests
{
    [Fact]
    public async Task EncodesRustTaggedCommandsAndPluralIdFields()
    {
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"accepted","payload":{"operation_id":"operation-1"}}
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        var operationId = await client.AcceptAsync(LocalAgentCommand.AnswerUserQuestion(
            "run-1",
            "interaction-1",
            new LocalAgentUserAnswer("Use this direction", ["option-1"], [])));

        Assert.Equal("operation-1", operationId);
        using var request = JsonDocument.Parse(transport.Request!);
        var root = request.RootElement;
        Assert.Equal(LocalAgentProtocol.Version, root.GetProperty("protocol_version").GetUInt32());
        Assert.Equal("user-1", root.GetProperty("owner_user_id").GetString());
        var command = root.GetProperty("command");
        Assert.Equal("answer_user_question", command.GetProperty("type").GetString());
        var answer = command.GetProperty("payload").GetProperty("answer");
        Assert.Equal("option-1", answer.GetProperty("selected_option_ids")[0].GetString());
        Assert.False(answer.TryGetProperty("selected_option_i_ds", out _));
    }

    [Fact]
    public async Task EncodesToolApprovalWithExactRunAndInvocationIdentity()
    {
        using var expectedRequest = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("tool_approval_request.json")));
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"accepted","payload":{"operation_id":"invocation-1"}}
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        await client.AcceptAsync(LocalAgentCommand.DecideToolApproval(
            "run-1",
            "invocation-1",
            LocalAgentToolApprovalDecision.Reject,
            "Rejected by the local user"));

        using var request = JsonDocument.Parse(transport.Request!);
        var command = request.RootElement.GetProperty("command");
        Assert.True(JsonElement.DeepEquals(
            expectedRequest.RootElement.GetProperty("command"),
            command));
        Assert.Equal("decide_tool_approval", command.GetProperty("type").GetString());
        var payload = command.GetProperty("payload");
        Assert.Equal("run-1", payload.GetProperty("run_id").GetString());
        Assert.Equal("invocation-1", payload.GetProperty("invocation_id").GetString());
        Assert.Equal("reject", payload.GetProperty("decision").GetString());
    }

    [Fact]
    public async Task EncodesRunControlAgainstTheExactObservedVersion()
    {
        using var expectedRequest = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("run_control_request.json")));
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"accepted","payload":{"operation_id":"operation-1"}}
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        await client.AcceptAsync(LocalAgentCommand.PauseRun("run-1", 7));

        using var request = JsonDocument.Parse(transport.Request!);
        Assert.True(JsonElement.DeepEquals(expectedRequest.RootElement, request.RootElement));
    }

    [Fact]
    public async Task UsesTheSharedV15RetryTaskAndTaskSnapshotFixtures()
    {
        using var expectedRequest = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("retry_task_request.json")));
        var requestTransport = new RecordingTransport(request => Reply(request, """
            {"type":"accepted","payload":{"operation_id":"operation-1"}}
            """));
        var requestClient = new WindowsLocalAgentIPCClient("user-1", requestTransport);

        await requestClient.SendAsync(LocalAgentCommand.RetryTask(new LocalAgentRetryTask(
            "task-1",
            "task-run-1",
            "Preserve the approved visual hierarchy.")));

        using var actualRequest = JsonDocument.Parse(requestTransport.Request!);
        Assert.Equal(15u, LocalAgentProtocol.Version);
        Assert.True(JsonElement.DeepEquals(
            expectedRequest.RootElement.GetProperty("command"),
            actualRequest.RootElement.GetProperty("command")));

        using var expectedReply = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("task_snapshot_response.json")));
        var responseJson = expectedReply.RootElement.GetProperty("response").GetRawText();
        var responseTransport = new RecordingTransport(request => Reply(request, responseJson));
        var responseClient = new WindowsLocalAgentIPCClient("user-1", responseTransport);

        var task = await responseClient.GetTaskAsync("task-1");

        Assert.Equal("task-run-1", task.InitialRunId);
        Assert.Equal("task-run-2", task.CurrentRunId);
        Assert.Equal(["task-run-1", "task-run-2"], task.RunIds);
        Assert.Equal("project-1", task.ProjectId);
    }

    [Fact]
    public async Task QueriesGenericRunDetailForRestartRecovery()
    {
        using var fixture = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("run_detail_response.json")));
        var transport = new RecordingTransport(request => Reply(
            request,
            fixture.RootElement.GetProperty("response").GetRawText()));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        var detail = await client.GetRunDetailAsync("run-1", eventLimit: 50, eventOffset: 10);

        Assert.Equal("run-1", detail.Run.RunId);
        Assert.Equal(42ul, detail.SnapshotEventSequence);
        using var request = JsonDocument.Parse(transport.Request!);
        var command = request.RootElement.GetProperty("command");
        Assert.Equal("get_run_detail", command.GetProperty("type").GetString());
        var payload = command.GetProperty("payload");
        Assert.Equal("run-1", payload.GetProperty("run_id").GetString());
        Assert.Equal(50u, payload.GetProperty("event_limit").GetUInt32());
        Assert.Equal(10u, payload.GetProperty("event_offset").GetUInt32());
    }

    [Fact]
    public async Task UsesSharedV15TaskGraphAndRunDetailProjections()
    {
        using var graphFixture = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("task_graph_response.json")));
        var graphTransport = new RecordingTransport(request => Reply(
            request,
            graphFixture.RootElement.GetProperty("response").GetRawText()));
        var graphClient = new WindowsLocalAgentIPCClient("user-1", graphTransport);

        var graph = await graphClient.GetTaskGraphAsync("thread-1", "turn-1");

        Assert.Equal(["task-1"], graph.RootTaskIds);
        Assert.Equal("project-1", Assert.Single(graph.Nodes).Task.Task.ProjectId);
        using (var request = JsonDocument.Parse(graphTransport.Request!))
        {
            var command = request.RootElement.GetProperty("command");
            Assert.Equal("get_task_graph", command.GetProperty("type").GetString());
            Assert.Equal(
                "thread-1",
                command.GetProperty("payload").GetProperty("source_thread_id").GetString());
        }

        using var detailFixture = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("task_run_detail_response.json")));
        var detailTransport = new RecordingTransport(request => Reply(
            request,
            detailFixture.RootElement.GetProperty("response").GetRawText()));
        var detailClient = new WindowsLocalAgentIPCClient("user-1", detailTransport);

        var detail = await detailClient.GetTaskRunDetailAsync(
            "task-1",
            "task-run-2",
            eventLimit: 40);

        Assert.Equal("task-run-2", detail.Run.Run.RunId);
        Assert.Equal("run_started", Assert.Single(detail.Events).EventType);
    }

    [Fact]
    public async Task EncodesUnitAndStorageProfileCommandsExactly()
    {
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"success"}
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        await client.SendAsync(LocalAgentCommand.GetStorageProfile());
        using (var request = JsonDocument.Parse(transport.Request!))
        {
            var command = request.RootElement.GetProperty("command");
            Assert.Equal("get_storage_profile", command.GetProperty("type").GetString());
            Assert.False(command.TryGetProperty("payload", out _));
        }

        await client.SendAsync(LocalAgentCommand.ApplyStorageProfile(
            LocalAgentStorageProfileSelection.Postgres("secret-postgres-1"),
            confirmNoActiveRuns: true));
        using var second = JsonDocument.Parse(transport.Request!);
        var payload = second.RootElement.GetProperty("command").GetProperty("payload");
        Assert.True(payload.GetProperty("confirm_no_active_runs").GetBoolean());
        var profile = payload.GetProperty("profile");
        Assert.Equal("postgres", profile.GetProperty("backend").GetString());
        Assert.Equal("secret-postgres-1", profile.GetProperty("connection_secret_reference").GetString());
        Assert.False(profile.TryGetProperty("database_reference", out _));
    }

    [Fact]
    public async Task EncodesOneSelectedPluginCapabilityInsteadOfAnInstallerStateEnvelope()
    {
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"success"}
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);
        using var capability = JsonDocument.Parse("""{"schema_version":2,"project_id":"project-1"}""");

        await client.SendAsync(LocalAgentCommand.InstallProjectPluginCapability(
            "project-1",
            "plugin-1",
            "release-1",
            capability.RootElement.Clone()));

        using var request = JsonDocument.Parse(transport.Request!);
        var command = request.RootElement.GetProperty("command");
        Assert.Equal("install_project_plugin_capability", command.GetProperty("type").GetString());
        var payload = command.GetProperty("payload");
        Assert.Equal("project-1", payload.GetProperty("project_id").GetString());
        Assert.Equal("plugin-1", payload.GetProperty("plugin_id").GetString());
        Assert.Equal(2, payload.GetProperty("capability_record").GetProperty("schema_version").GetInt32());
    }

    [Fact]
    public async Task PreservesUInt64EventCursorsWithoutDoubleConversion()
    {
        const ulong cursor = 9_007_199_254_740_993;
        var transport = new RecordingTransport(request => Reply(request, $$$"""
            {
              "type":"events",
              "payload":{
                "events":[{
                  "event_seq":{{{cursor}}},
                  "emitted_at":"2026-09-12T03:00:00Z",
                  "event":{"type":"host_status","payload":{"model_config_id":"opaque-value"}}
                }],
                "next_seq":{{{cursor}}},
                "has_more":true
              }
            }
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        var page = await client.SubscribeRunEventsAsync(cursor - 1, 50);

        Assert.Equal(cursor, page.NextSequence);
        Assert.Equal(cursor, Assert.Single(page.Events).EventSeq);
        Assert.True(page.HasMore);
        using var request = JsonDocument.Parse(transport.Request!);
        Assert.Equal(
            cursor - 1,
            request.RootElement.GetProperty("command").GetProperty("payload")
                .GetProperty("after_seq").GetUInt64());
    }

    [Fact]
    public async Task PreservesRunBoundMemorySyncStatusFromTheSharedFixture()
    {
        using var fixture = JsonDocument.Parse(
            await File.ReadAllBytesAsync(Fixture("memory_sync_event_response.json")));
        var responseJson = fixture.RootElement.GetProperty("response").GetRawText();
        var transport = new RecordingTransport(request => Reply(request, responseJson));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        var page = await client.SubscribeRunEventsAsync(42, 50);

        var payload = Assert.Single(page.Events).Event.Payload!.Value;
        Assert.Equal("run-1", payload.GetProperty("run_id").GetString());
        Assert.Equal(2ul, payload.GetProperty("pending_count").GetUInt64());
        Assert.Equal(1ul, payload.GetProperty("failed_count").GetUInt64());
    }

    [Fact]
    public async Task RejectsMismatchedRequestAndProtocolVersions()
    {
        var wrongRequest = new RecordingTransport(_ => JsonSerializer.SerializeToUtf8Bytes(new
        {
            protocol_version = LocalAgentProtocol.Version,
            request_id = "another-request",
            response = new { type = "success" },
        }));
        await Assert.ThrowsAsync<InvalidDataException>(() =>
            new WindowsLocalAgentIPCClient("user-1", wrongRequest)
                .SendAsync(LocalAgentCommand.GetStorageProfile()));

        var wrongVersion = new RecordingTransport(request => Reply(
            request,
            """{"type":"success"}""",
            protocolVersion: 2));
        await Assert.ThrowsAsync<InvalidDataException>(() =>
            new WindowsLocalAgentIPCClient("user-1", wrongVersion)
                .SendAsync(LocalAgentCommand.GetStorageProfile()));
    }

    [Fact]
    public async Task RejectsUnknownTaggedResponseFields()
    {
        var transport = new RecordingTransport(request => Reply(request, """
            {"type":"success","unexpected":true}
            """));

        await Assert.ThrowsAsync<InvalidDataException>(() =>
            new WindowsLocalAgentIPCClient("user-1", transport)
                .SendAsync(LocalAgentCommand.GetStorageProfile()));
    }

    [Fact]
    public async Task SurfacesStructuredHostErrors()
    {
        var transport = new RecordingTransport(request => Reply(request, """
            {
              "type":"error",
              "payload":{"code":"run_conflict","message":"Run changed","retryable":true}
            }
            """));
        var client = new WindowsLocalAgentIPCClient("user-1", transport);

        var error = await Assert.ThrowsAsync<LocalAgentRejectedException>(() =>
            client.SendAsync(LocalAgentCommand.PauseRun("run-1", 1)));

        Assert.Equal("run_conflict", error.Error.Code);
        Assert.True(error.Error.Retryable);
    }

    [Fact]
    public async Task FrameCodecUsesBigEndianAndRejectsOversizedOrTruncatedFrames()
    {
        await using var written = new MemoryStream();
        await LocalAgentFrameCodec.WriteAsync(written, "hello"u8.ToArray(), 32);
        Assert.Equal([0, 0, 0, 5], written.ToArray()[..4]);

        await using var oversized = FrameHeader(33);
        await Assert.ThrowsAsync<InvalidDataException>(() =>
            LocalAgentFrameCodec.ReadAsync(oversized, 32));

        await using var truncated = FrameHeader(5, [1, 2]);
        await Assert.ThrowsAsync<EndOfStreamException>(() =>
            LocalAgentFrameCodec.ReadAsync(truncated, 32));
    }

    [Theory]
    [InlineData("chatos-local-agent-7bb214f0", true)]
    [InlineData("chatos-local-agent-test_1234", true)]
    [InlineData("chatos-local-agent-short", false)]
    [InlineData("chatos-local-agent-bad\\name", false)]
    [InlineData(@"\\.\pipe\chatos-local-agent-7bb214f0", false)]
    [InlineData(@"\\server\pipe\chatos-local-agent-7bb214f0", false)]
    [InlineData("other-7bb214f0", false)]
    public void AcceptsOnlyLocalPrivatePipeNames(string pipeName, bool valid)
    {
        var error = Record.Exception(() => NamedPipeLocalAgentTransport.ValidatePipeName(pipeName));
        Assert.Equal(valid, error is null);
    }

    [Theory]
    [InlineData("S-1-5-21-100-200-300-400", "S-1-5-21-100-200-300-400", true)]
    [InlineData("s-1-5-21-100-200-300-400", "S-1-5-21-100-200-300-400", true)]
    [InlineData("S-1-5-18", "S-1-5-21-100-200-300-400", false)]
    [InlineData("", "S-1-5-21-100-200-300-400", false)]
    public void LocalAgentRequiresTheCurrentDesktopUserSid(
        string actual,
        string expected,
        bool trusted) =>
        Assert.Equal(trusted, WindowsLocalAgentServerIdentityVerifier.IsExpectedUserSid(actual, expected));

    private static byte[] Reply(
        byte[] request,
        string responseJson,
        uint protocolVersion = LocalAgentProtocol.Version)
    {
        using var requestDocument = JsonDocument.Parse(request);
        var requestId = requestDocument.RootElement.GetProperty("request_id").GetString();
        using var response = JsonDocument.Parse(responseJson);
        return JsonSerializer.SerializeToUtf8Bytes(new
        {
            protocol_version = protocolVersion,
            request_id = requestId,
            response = response.RootElement.Clone(),
        });
    }

    private static string Fixture(
        string name,
        [CallerFilePath] string sourceFile = "") =>
        Path.GetFullPath(Path.Combine(
            Path.GetDirectoryName(sourceFile)!,
            "..",
            "..",
            "..",
            "shared",
            "fixtures",
            "local_agent",
            "v15",
            name));

    private static MemoryStream FrameHeader(uint length, byte[]? body = null)
    {
        var stream = new MemoryStream();
        var header = new byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(header, length);
        stream.Write(header);
        if (body is not null) stream.Write(body);
        stream.Position = 0;
        return stream;
    }

    private sealed class RecordingTransport(Func<byte[], byte[]> reply) : ILocalAgentFrameTransport
    {
        public byte[]? Request { get; private set; }

        public Task<byte[]> ExchangeAsync(
            byte[] request,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            Request = request;
            return Task.FromResult(reply(request));
        }
    }
}
