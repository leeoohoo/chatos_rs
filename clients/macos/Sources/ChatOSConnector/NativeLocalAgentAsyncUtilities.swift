import Foundation

enum NativeLocalAgentEventPollingPolicy {
    static let activeDelay = Duration.milliseconds(250)
    static let maximumIdleDelay = Duration.seconds(2)

    static func nextIdleDelay(after delay: Duration) -> Duration {
        min(delay * 2, maximumIdleDelay)
    }
}

enum NativeLocalAgentBoundedLoader {
    static func load<Input: Sendable, Output: Sendable>(
        _ inputs: [Input],
        maximumConcurrentTasks: Int,
        operation: @escaping @Sendable (Input) async throws -> Output
    ) async throws -> [Output] {
        guard !inputs.isEmpty else { return [] }
        let concurrency = max(1, min(maximumConcurrentTasks, inputs.count))
        return try await withThrowingTaskGroup(of: (Int, Output).self) { group in
            var nextIndex = 0
            var results = [Output?](repeating: nil, count: inputs.count)

            func submit(_ index: Int) {
                let input = inputs[index]
                group.addTask {
                    (index, try await operation(input))
                }
            }

            while nextIndex < concurrency {
                submit(nextIndex)
                nextIndex += 1
            }

            while let (index, output) = try await group.next() {
                results[index] = output
                if nextIndex < inputs.count {
                    submit(nextIndex)
                    nextIndex += 1
                }
            }

            return results.compactMap { $0 }
        }
    }
}
