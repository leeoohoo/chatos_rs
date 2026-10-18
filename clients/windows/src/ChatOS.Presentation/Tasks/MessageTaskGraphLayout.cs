namespace ChatOS.Presentation.Tasks;

public static class MessageTaskGraphLayout
{
    public static IReadOnlyDictionary<string, int> ResolveDepths(
        IReadOnlyCollection<MessageTaskGraphNodeItemViewModel> nodes)
    {
        var byId = nodes.ToDictionary(node => node.Id, StringComparer.Ordinal);
        var depths = new Dictionary<string, int>(StringComparer.Ordinal);
        var visiting = new HashSet<string>(StringComparer.Ordinal);

        int Visit(MessageTaskGraphNodeItemViewModel node)
        {
            if (depths.TryGetValue(node.Id, out var cached))
            {
                return cached;
            }
            if (!visiting.Add(node.Id))
            {
                return Math.Max(0, node.Depth);
            }

            var depth = Math.Max(0, node.Depth);
            var prerequisiteIds = node.Task.PrerequisiteTaskIds
                .Concat(node.Task.PrerequisiteTasks.Select(reference => reference.Id))
                .Distinct(StringComparer.Ordinal);
            foreach (var prerequisiteId in prerequisiteIds)
            {
                if (byId.TryGetValue(prerequisiteId, out var prerequisite))
                {
                    depth = Math.Max(depth, Visit(prerequisite) + 1);
                }
            }

            visiting.Remove(node.Id);
            depths[node.Id] = depth;
            return depth;
        }

        foreach (var node in nodes)
        {
            Visit(node);
        }
        return depths;
    }
}
