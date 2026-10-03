import Foundation

enum NativeLocalAgentEventWaitPolicy {
    /// Host protocol maximum. The app's production request timeout is 75s,
    /// leaving 15s for framing and scheduling around an empty long poll.
    static let timeoutMilliseconds: UInt64 = 60_000
}

enum NativeRequiredOptionalParallelLoader {
    static func load<Required: Sendable, OptionalValue: Sendable>(
        required: @escaping @Sendable () async throws -> Required,
        optional: @escaping @Sendable () async throws -> OptionalValue
    ) async throws -> (required: Required, optional: OptionalValue?) {
        async let requiredValue = required()
        async let optionalValue = try? await optional()
        return try await (requiredValue, optionalValue)
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
