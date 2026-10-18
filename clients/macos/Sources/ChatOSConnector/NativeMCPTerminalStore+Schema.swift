extension NativeMCPTerminalStore {
    static var toolNames: Set<String> {
        [
            "execute_command", "get_recent_logs", "process_list", "process_poll",
            "process_log", "process_wait", "process_write", "process_kill", "process",
        ]
    }

    static var toolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "execute_command",
                description: "在当前项目中执行 command 指定的完整 shell 命令。purpose 必须用面向用户的自然语言说明本次运行目的，供审批界面优先展示；不要在 purpose 中重复粘贴命令。前台默认最多等待 300 秒；长时间运行的命令应使用 background=true，再通过 process_poll 或 process_wait 跟踪。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "purpose": .object([
                        "type": .string("string"),
                        "minLength": .number(1),
                        "maxLength": .number(Double(NativeLocalAgentTerminalCommandResolver.maximumPurposeCharacters)),
                        "description": .string("向用户说明为什么需要运行这条命令，以及预期得到什么结果。使用简短自然语言，不要复制 shell 命令。"),
                    ]),
                    "command": .object([
                        "type": .string("string"),
                        "minLength": .number(1),
                        "description": .string("要实际执行的完整 shell 命令或脚本正文；不要填写自然语言摘要。"),
                    ]),
                    "background": .object(["type": .string("boolean"), "default": .bool(false)]),
                    "timeout_ms": integerSchema(
                        minimum: Self.minimumForegroundTimeoutMilliseconds,
                        maximum: Self.maximumForegroundTimeoutMilliseconds
                    ),
                    "timeout": integerSchema(
                        minimum: Self.minimumForegroundTimeoutMilliseconds / 1_000,
                        maximum: Self.maximumForegroundTimeoutMilliseconds / 1_000
                    ),
                ],
                required: ["purpose", "command"]
            ),
            definition(
                name: "get_recent_logs",
                description: "读取当前项目最近执行的命令日志。",
                properties: [
                    "per_terminal_limit": integerSchema(minimum: 1, maximum: 50),
                    "terminal_limit": integerSchema(minimum: 1, maximum: 20),
                ],
                required: []
            ),
            definition(
                name: "process_list",
                description: "列出当前项目中的命令进程。",
                properties: [
                    "include_exited": .object(["type": .string("boolean"), "default": .bool(false)]),
                    "limit": integerSchema(minimum: 1, maximum: 100),
                ],
                required: []
            ),
            definition(
                name: "process_poll",
                description: "轮询命令进程的状态和增量日志。",
                properties: processReadProperties(defaultLimit: 80),
                required: ["terminal_id"]
            ),
            definition(
                name: "process_log",
                description: "分页读取命令进程日志。",
                properties: processReadProperties(defaultLimit: 200),
                required: ["terminal_id"]
            ),
            definition(
                name: "process_wait",
                description: "等待命令进程结束或等待超时。",
                properties: [
                    "terminal_id": .object(["type": .string("string")]),
                    "timeout_ms": integerSchema(minimum: 1_000, maximum: Self.maximumWaitMilliseconds),
                    "timeout": integerSchema(minimum: 1, maximum: Self.maximumWaitMilliseconds / 1_000),
                ],
                required: ["terminal_id"]
            ),
            definition(
                name: "process_write",
                description: "向命令进程写入标准输入。",
                properties: [
                    "terminal_id": .object(["type": .string("string")]),
                    "data": .object(["type": .string("string")]),
                    "submit": .object(["type": .string("boolean"), "default": .bool(false)]),
                ],
                required: ["terminal_id", "data"]
            ),
            definition(
                name: "process_kill",
                description: "终止命令进程。",
                properties: ["terminal_id": .object(["type": .string("string")])],
                required: ["terminal_id"]
            ),
            definition(
                name: "process",
                description: "兼容的进程管理入口。支持 list、poll、log、wait、kill、write、submit、close。",
                properties: [
                    "action": .object([
                        "type": .string("string"),
                        "enum": .array(["list", "poll", "log", "wait", "kill", "write", "submit", "close"].map(NativeJSONValue.string)),
                    ]),
                    "terminal_id": .object(["type": .string("string")]),
                    "include_exited": .object(["type": .string("boolean")]),
                    "offset": integerSchema(minimum: 0, maximum: nil),
                    "limit": integerSchema(minimum: 1, maximum: 200),
                    "timeout_ms": integerSchema(minimum: 1_000, maximum: Self.maximumWaitMilliseconds),
                    "timeout": integerSchema(minimum: 1, maximum: Self.maximumWaitMilliseconds / 1_000),
                    "data": .object(["type": .string("string")]),
                ],
                required: ["action"]
            ),
        ]
    }
}
