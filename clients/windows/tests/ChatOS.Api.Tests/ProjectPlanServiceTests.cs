using System.Text.Json;
using ChatOS.Api.Projects;

namespace ChatOS.Api.Tests;

public sealed class ProjectPlanServiceTests
{
    [Fact]
    public async Task PlanMapsHierarchyCountsAndNormalizesGraphNodePrefixes()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request => request.RequestUri!.AbsolutePath switch
        {
            "/api/chatos/projects/project%2F1/plan" => StubHttpMessageHandler.Json("""
                {
                  "project_id":"project/1",
                  "requirements":[
                    {"id":"root","title":"根需求","priority":90,"status":"in_progress"},
                    {"id":"child","parent_requirement_id":"root","title":"子需求","priority":50,"status":"draft"}
                  ],
                  "work_items":[{"id":"task-1","requirement_id":"root","title":"实现","status":"todo","priority":10}],
                  "work_item_counts":{"total":1,"open":1,"done":0,"blocked":0},
                  "dependency_graph":{"edges":[{"from":"requirement:root","to":"requirement:child","edge_type":"depends_on"}]}
                }
                """),
            _ => throw new InvalidOperationException(request.RequestUri.ToString()),
        });
        var service = new ProjectPlanService(client);

        var plan = await service.FetchPlanAsync("project/1");

        Assert.Equal(2, plan.Requirements.Count);
        Assert.Equal("root", plan.Requirements.Single(value => value.Id == "child").ParentRequirementId);
        Assert.Equal(1, plan.Counts.Total);
        var edge = Assert.Single(plan.Edges);
        Assert.Equal("root", edge.SourceId);
        Assert.Equal("child", edge.TargetId);
    }

    [Fact]
    public async Task DocumentsAndExecutionPreserveWorkbenchIdentity()
    {
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request => request.RequestUri!.AbsolutePath switch
        {
            "/api/chatos/projects/project-1/requirements/req-1/documents" =>
                StubHttpMessageHandler.Json("""
                    [{"id":"doc-1","doc_type":"implementation_plan","title":"实施计划","format":"markdown","content":"# 计划","version":3,"updated_at":"2026-08-25T03:00:00Z"}]
                    """),
            "/api/chatos/projects/project-1/requirements/req-1/execution-plan" =>
                StubHttpMessageHandler.Json("""
                    {"found":true,"project_id":"project-1","requirement_id":"req-1","conversation_id":"conversation-1","execution_group_id":"group-1","message_id":"message-1","contact_id":"contact-1","status":"awaiting_confirmation","confirmation_status":"awaiting_confirmation","task_count":4,"has_started_runs":false,"include_prerequisite_dependents":true,"created_at":"2026-08-25T03:00:00Z"}
                    """),
            _ => throw new InvalidOperationException(request.RequestUri.ToString()),
        });
        var service = new ProjectPlanService(client);

        var document = Assert.Single(await service.FetchDocumentsAsync("project-1", "req-1"));
        var execution = await service.FetchExecutionAsync("project-1", "req-1");

        Assert.Equal("implementation_plan", document.Type);
        Assert.Equal(3, document.Version);
        Assert.NotNull(document.UpdatedAt);
        Assert.Equal("group-1", execution?.ExecutionGroupId);
        Assert.Equal("contact-1", execution?.ContactId);
        Assert.Equal(4, execution?.TaskCount);
        Assert.True(execution?.IncludePrerequisiteDependents);
    }

    [Fact]
    public async Task CreateExecutionTrimsFeedbackAndRequiresStableIdentity()
    {
        JsonElement body = default;
        var store = new MemoryTokenStore();
        store.Seed("valid");
        var client = ApiTestClient.Create(store, request =>
        {
            using var document = JsonDocument.Parse(
                request.Content!.ReadAsStringAsync().GetAwaiter().GetResult());
            body = document.RootElement.Clone();
            return StubHttpMessageHandler.Json("""
                {"conversation_id":"conversation-1","execution_group_id":"group-1","confirmation_status":"pending","has_started_runs":false}
                """);
        });
        var service = new ProjectPlanService(client);

        var execution = await service.CreateExecutionAsync(
            "project-1",
            "req-1",
            true,
            "  先检查依赖  ");

        Assert.Equal("group-1", execution.ExecutionGroupId);
        Assert.True(body.GetProperty("include_prerequisite_dependents").GetBoolean());
        Assert.Equal("先检查依赖", body.GetProperty("planning_feedback").GetString());
    }
}
