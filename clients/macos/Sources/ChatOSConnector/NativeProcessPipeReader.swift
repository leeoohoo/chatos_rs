import Foundation

enum NativeProcessPipeReader {
    static func install(
        on handle: FileHandle,
        onData: @escaping @Sendable (Data) -> Void = { _ in },
        onEOF: @escaping @Sendable () -> Void = {}
    ) {
        handle.readabilityHandler = { handle in
            let data = handle.availableData
            guard !data.isEmpty else {
                // FileHandle repeatedly reports a pipe at EOF as readable. The
                // handler must be removed here rather than relying on a process
                // termination callback that can be delayed across system sleep.
                handle.readabilityHandler = nil
                onEOF()
                return
            }
            onData(data)
        }
    }
}
