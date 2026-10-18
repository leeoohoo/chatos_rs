using ChatOS.Core.Domain;
using ChatOS.Presentation.Tasks;

namespace ChatOS.Presentation.Tests;

public sealed partial class MessageTaskGraphViewModelTests
{
    [Fact]
    public void LayoutDerivesDependencyDepthWhenServerDepthsAreFlat()
    {
        var first = Node("task-a", []);
        var second = Node("task-b", ["task-a"]);
        var third = Node("task-c", ["task-a"]);

        var depths = MessageTaskGraphLayout.ResolveDepths([first, second, third]);

        Assert.Equal(0, depths["task-a"]);
        Assert.Equal(1, depths["task-b"]);
        Assert.Equal(1, depths["task-c"]);
    }

    [Fact]
    public void LayoutPreservesGreaterAuthoritativeDepth()
    {
        var root = Node("task-a", [], depth: 2);
        var dependent = Node("task-b", ["task-a"]);

        var depths = MessageTaskGraphLayout.ResolveDepths([root, dependent]);

        Assert.Equal(2, depths["task-a"]);
        Assert.Equal(3, depths["task-b"]);
    }

    private static MessageTaskGraphNodeItemViewModel Node(
        string id,
        IReadOnlyList<string> prerequisites,
        int depth = 0)
    {
        var task = TaskGraphServiceDouble.MakeTask(id, id, "pending", $"run-{id}") with
        {
            PrerequisiteTaskIds = prerequisites,
        };
        return new MessageTaskGraphNodeItemViewModel(
            new MessageTaskGraphNode(task, depth, depth == 0, true, []));
    }
}
