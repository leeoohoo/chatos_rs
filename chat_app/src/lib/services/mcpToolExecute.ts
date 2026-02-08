import axios from 'axios';
import { debugLog } from '@/lib/utils';

interface McpServer {
    name: string;
    url: string;
}

interface ToolCall {
    id?: string;
    name?: string;
    function?: {
        name: string;
        arguments: any;
    };
    arguments?: any;
}

interface ToolInfo {
    originalName: string;
    serverName: string;
    serverUrl: string;
    supportsStreaming?: boolean;
}

interface Tool {
    type: string;
    function: {
        name: string;
        description?: string;
        parameters: any;
    };
}

// 创建专用于 MCP 的 HTTP 客户端，不包含认证头
const mcpHttp = axios.create({
    timeout: 86400000,
    headers: {
        'Content-Type': 'application/json'
    }
});

class McpToolExecute {
    private mcpServers: McpServer[];
    private tools: Tool[];
    private toolMetadata: Map<string, ToolInfo>;

    constructor(mcpServers: McpServer[]) {
        this.mcpServers = mcpServers
        this.tools = []
        this.toolMetadata = new Map() // 存储工具元数据，不发送给后端
    }

    async init(): Promise<void> {
        await this.buildTools();
    }


    /**
     * 执行流式工具调用
     * @param {Object} toolCall - 工具调用对象
     * @param {Function} onChunk - 接收流式数据的回调函数
     * @param {Function} onComplete - 完成时的回调函数
     * @param {Function} onError - 错误时的回调函数
     * @returns {Promise<void>}
     */
    async executeStream(toolCall: ToolCall, onChunk: (chunk: string) => void, onComplete: () => void, onError: (error: Error) => void): Promise<void> {
        return new Promise(async (resolve, reject) => {
            try {
                const toolName = toolCall.function?.name || toolCall.name!;
                const toolArgs = toolCall.function?.arguments || toolCall.arguments || {};

                // 解析参数
                let parsedArgs = toolArgs;
                if (typeof toolArgs === 'string') {
                    try {
                        parsedArgs = JSON.parse(toolArgs);
                    } catch (error) {
                        console.warn('Failed to parse tool arguments:', error);
                        parsedArgs = {};
                    }
                }

                // 查找工具信息
                const toolInfo = this.findToolInfo(toolName!);
                if (!toolInfo) {
                    throw new Error(`Tool not found: ${toolName}`);
                }
                
                // 检查工具是否支持流式输出
                const supportsStreaming = this.toolSupportsStreaming(toolName);
                debugLog("开始调用工具：", supportsStreaming)

                if (supportsStreaming) {
                    // 使用 SSE 进行流式调用
                    await this.callMcpToolStream(
                        toolInfo.serverUrl,
                        toolInfo.originalName,
                        parsedArgs,
                        onChunk,
                        // 包装onComplete回调，在完成时resolve Promise
                        () => {
                            onComplete();
                            resolve();
                        },
                        // 包装onError回调，在错误时reject Promise
                        (error: any) => {
                            onError(new Error(error.message || 'Tool execution failed'));
                            reject(error);
                        }
                    );
                } else {
                    // 回退到普通调用
                    const result = await this.callMcpTool(
                        toolInfo.serverUrl,
                        toolInfo.originalName,
                        parsedArgs
                    );
                    onChunk(JSON.stringify(result));
                    onComplete();
                    resolve();
                }

            } catch (error: any) {
                // 检查是否是用户中断错误
                if (error.message === 'Stream aborted by user' || error.name === 'AbortError') {
                    debugLog(`Tool ${toolCall.function?.name || toolCall.name} stream aborted by user`);
                    resolve();
                    return;
                }
                console.error(`Failed to execute stream tool ${toolCall.function?.name || toolCall.name}:`, error);
                onError(new Error(error.message || 'Tool execution failed'));
                reject(error);
            }
        });
    }

    async execute(tool_calls: ToolCall[]): Promise<any[]> {
        // 1. 根据tool_calls 中的name 来定位mcpServer 和 tool
        // 需要注意 之前 我们是把 mcpServer 的name 和 tool 的那么拼在一起的 看一下 buildTools
        //2. 定位到 mcp 的 tool 后调用 并返回结果
        debugLog("开始调用工具，但是没有用流式的方式")

        const results = [];

        for (const toolCall of tool_calls) {
            try {
                // 解析工具调用
                const toolName = toolCall.function?.name || toolCall.name;
                const toolArgs = toolCall.function?.arguments || toolCall.arguments || {};

                // 检查工具名称是否存在
                if (!toolName) {
                    throw new Error('Tool name is required');
                }

                // 解析参数（如果是字符串格式）
                let parsedArgs = toolArgs;
                if (typeof toolArgs === 'string') {
                    try {
                        parsedArgs = JSON.parse(toolArgs);
                    } catch (error) {
                        console.warn('Failed to parse tool arguments:', error);
                        parsedArgs = {};
                    }
                }

                // 从工具名称中提取服务器名称和原始工具名称
                // 格式: serverName_toolName
                const toolInfo = this.findToolInfo(toolName);
                if (!toolInfo) {
                    throw new Error(`Tool not found: ${toolName}`);
                }

                // 调用 MCP 服务器的工具
                const result = await this.callMcpTool(
                    toolInfo.serverUrl,
                    toolInfo.originalName,
                    parsedArgs || {}
                );

                results.push({
                    tool_call_id: toolCall.id || `call_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                    role: 'tool',
                    name: toolName,
                    content: JSON.stringify(result),
                    createdAt: new Date() // 确保工具调用结果有正确的时间戳
                });

            } catch (error: any) {
                // 检查是否是用户中断错误
                if (error.message === 'Stream aborted by user' || error.name === 'AbortError') {
                    debugLog(`Tool ${toolCall.function?.name || toolCall.name} execution aborted by user`);
                    results.push({
                        tool_call_id: toolCall.id || `call_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                        role: 'tool',
                        name: toolCall.function?.name || toolCall.name,
                        content: JSON.stringify({
                            result: 'Tool execution aborted by user'
                        })
                    });
                    continue;
                }
                
                console.error(`Failed to execute tool ${toolCall.function?.name || toolCall.name}:`, error);

                results.push({
                    tool_call_id: toolCall.id || `call_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                    role: 'tool',
                    name: toolCall.function?.name || toolCall.name,
                    content: JSON.stringify({
                        error: error.message || 'Tool execution failed'
                    })
                });
            }
        }

        return results;
    }

    /**
     * 根据工具名称查找工具信息
     * @param {string} toolName - 工具名称（包含服务器前缀）
     * @returns {Object|null} 工具信息
     */
    findToolInfo(toolName: string): ToolInfo | null {
        // 从元数据映射中获取工具信息
        if (this.toolMetadata && this.toolMetadata.has(toolName)) {
            return this.toolMetadata.get(toolName) ?? null;
        }

        // 如果元数据映射中没有，尝试从工具列表中查找（向后兼容）
        const tool = this.tools.find(t => t.function?.name === toolName);
        if (tool && (tool as any).originalName) {
            return {
                originalName: (tool as any).originalName,
                serverName: (tool as any).serverName,
                serverUrl: (tool as any).serverUrl
            };
        }

        return null;
    }

    /**
     * 检查工具是否支持流式输出
     * @param {string} toolName - 工具名称
     * @returns {boolean}
     */
    toolSupportsStreaming(_toolName: string): boolean {
        // 新框架中所有工具都支持流式输出
        return true;
    }

    /**
     * 使用 SSE 调用 MCP 工具的流式方法
     * @param {string} serverUrl - MCP 服务器 URL
     * @param {string} toolName - 原始工具名称
     * @param {Object} arguments_ - 工具参数
     * @param {Function} onChunk - 接收流式数据的回调
     * @param {Function} onComplete - 完成回调
     * @param {Function} onError - 错误回调
     */
    async callMcpToolStream(serverUrl: string, toolName: string, arguments_: any, onChunk: (chunk: string) => void, onComplete: () => void, onError: (error: Error) => void): Promise<void> {
        try {
            // 构建 SSE URL - 使用OpenAI格式的端点
            const sseUrl = `${serverUrl}/sse/openai/tool/call`;

            debugLog('MCP Stream Tool Call URL:', sseUrl);

            // 创建 AbortController 用于超时控制
            const controller = new AbortController();
            // 不设置总体超时，让流式调用可以持续进行
            // 只在读取层面设置超时检查

            // 创建 fetch 的流式响应
            const response = await fetch(sseUrl, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'Accept': 'text/event-stream',
                    'Cache-Control': 'no-cache'
                },
                body: JSON.stringify({
                    tool_name: toolName,
                    arguments: arguments_
                }),
                signal: controller.signal
            });

            if (!response.ok) {
                throw new Error(`HTTP error! status: ${response.status}`);
            }

            const reader = response.body!.getReader();
            const decoder = new TextDecoder();
            let buffer = '';
            let lastActivityTime = Date.now();

            // 设置读取超时检查 - 更宽松的超时设置
            const readTimeoutId = setInterval(() => {
                const now = Date.now();
                if (now - lastActivityTime > 120000) { // 2分钟无数据则超时
                    clearInterval(readTimeoutId);
                    reader.cancel();
                    onError(new Error('Stream read timeout: no data received for 2 minutes'));
                    return;
                }
            }, 10000); // 每10秒检查一次

            try {
                while (true) {
                    const {done, value} = await reader.read();

                    if (done) {
                        break;
                    }

                    lastActivityTime = Date.now(); // 更新活动时间
                    buffer += decoder.decode(value, {stream: true});

                    // 处理 SSE 事件
                    const events = this.parseSSEEvents(buffer);
                    buffer = events.remaining;

                    for (const event of events.parsed) {
                        const shouldEnd = await this.handleSSEEvent(event, onChunk, onComplete, onError);
                        if (shouldEnd) {
                            // 收到end事件或error事件，主动结束流式读取
                            clearInterval(readTimeoutId);
                            reader.cancel(); // 取消reader
                            return;
                        }
                    }
                }
            } finally {
                clearInterval(readTimeoutId);
                reader.releaseLock();
            }

        } catch (error: any) {
            // 检查是否是用户中断错误
            if (error.name === 'AbortError' || error.message === 'Stream aborted by user') {
                debugLog('MCP stream request aborted by user');
                return;
            }
            
            console.error(`Failed to call MCP stream tool ${toolName}:`, error);
            
            if (error.message && error.message.includes('Stream read timeout')) {
                onError(error);
            } else if (error.message && error.message.includes('fetch')) {
                onError(new Error(`Network error during stream request: ${error.message}`));
            } else {
                onError(new Error(error.message || 'Stream error'));
            }
        }
    }

    /**
     * 解析 SSE 事件
     * @param {string} buffer - 缓冲区数据
     * @returns {Object} 解析结果
     */
    parseSSEEvents(buffer: string): { parsed: any[], remaining: string } {
        const events = [];
        const lines = buffer.split('\n');
        let currentEvent = {};
        let i = 0;

        while (i < lines.length) {
            const line = lines[i].trim();

            if (line === '') {
                // 空行表示事件结束
                if ((currentEvent as any).data) {
                    // 对于OpenAI格式，可能没有event字段，只有data字段
                    if (!(currentEvent as any).event) {
                        (currentEvent as any).event = 'data'; // 默认为data事件
                    }
                    events.push(currentEvent);
                }
                currentEvent = {};
            } else if (line.startsWith('event:')) {
                (currentEvent as any).event = line.substring(6).trim();
            } else if (line.startsWith('data:')) {
                const dataContent = line.substring(5).trim();
                if (dataContent === '[DONE]') {
                    // OpenAI格式的结束标记
                    events.push({ event: 'end', data: '{}' });
                } else {
                    (currentEvent as any).data = dataContent;
                }
            }

            i++;
        }

        // 返回未完成的缓冲区部分
        const lastCompleteEventIndex = buffer.lastIndexOf('\n\n');
        const remaining = lastCompleteEventIndex >= 0 ?
            buffer.substring(lastCompleteEventIndex + 2) : buffer;

        return {parsed: events, remaining};
    }

    /**
     * 处理 SSE 事件
     * @param {Object} event - SSE 事件
     * @param {Function} onChunk - 数据回调
     * @param {Function} onComplete - 完成回调
     * @param {Function} onError - 错误回调
     * @returns {boolean} 是否应该结束流式读取
     */
    async handleSSEEvent(event: any, onChunk: (chunk: string) => void, onComplete: () => void, onError: (error: Error) => void): Promise<boolean> {
        try {
            const data = JSON.parse((event as any).data);
            switch ((event as any).event) {

                case 'start':
                    debugLog('Stream started:', data);
                    return false;

                case 'data':
                    debugLog('Stream data:', data);
                    // 检查是否是OpenAI格式的响应
                    if (data.choices && data.choices.length > 0) {
                        // OpenAI 格式
                        const choice = data.choices[0];
                        if (choice.delta) {
                            const delta = choice.delta;
                            if (delta.content && delta.content) {
                                onChunk(delta.content);
                            } else if (delta.function_call && delta.function_call.arguments) {
                                // 处理函数调用
                                onChunk(delta.function_call.arguments);
                            }
                        }
                    }
                    // 兼容旧格式：查找 chunk 字段
                    else if (data.chunk) {
                        onChunk(data.chunk);
                    }
                    return false;

                case 'end':
                    debugLog('Stream completed:', data);
                    onComplete();
                    return true; // 返回true表示应该结束流式读取

                case 'error':
                    console.error('Stream error:', data);
                    onError(new Error(data.error || 'Stream error'));
                    return true; // 返回true表示应该结束流式读取

                default:
                    console.warn('Unknown SSE event:', (event as any).event, data);
                    return false;
            }
        } catch (error: any) {
            console.error('Failed to parse SSE event:', error);
            onError(new Error(error.message || 'SSE parse error'));
            return true; // 解析错误时也应该结束流式读取
        }
    }

    /**
     * 调用 MCP 服务器的工具
     * @param {string} serverUrl - MCP 服务器 URL
     * @param {string} toolName - 原始工具名称
     * @param {Object} arguments_ - 工具参数
     * @returns {Promise<Object>} 工具执行结果
     */
    async callMcpTool(serverUrl: string, toolName: string, arguments_: any): Promise<any> {
        try {
            // 构建 JSON-RPC 请求
            const request = {
                jsonrpc: '2.0',
                id: `req_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                method: 'tools/call',
                params: {
                    name: toolName,
                    arguments: arguments_ || {}
                }
            };

            // 在开发环境使用代理，生产环境使用直接地址
            const mcpUrl = `${serverUrl}`

            debugLog('MCP Tool Call URL:', mcpUrl);
            // 发送请求到 MCP 服务器
            const response = await mcpHttp.post(mcpUrl, request);

            if (response.data.error) {
                throw new Error(`MCP tool call failed: ${response.data.error.message}`);
            }

            return response.data.result;

        } catch (error: any) {
            console.error(`Failed to call MCP tool ${toolName}:`, error);
            throw error;
        }
    }


    async buildTools(): Promise<void> {
        try {
            this.tools = [];

            // 1. 首先循环调用mcp 服务获取每个服务的tools
            for (const mcpServer of this.mcpServers) {
                try {
                    // 调用MCP服务获取tools - 使用标准的 MCP 协议
                    const request = {
                        jsonrpc: '2.0',
                        id: `req_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`,
                        method: 'tools/list',
                        params: {}
                    };

                    // 在开发环境使用代理，生产环境使用直接地址
                    const mcpUrl = `${mcpServer.url}`

                    debugLog('MCP Tools List URL:', mcpUrl);
                    const toolsResponse = await mcpHttp.post(mcpUrl, request);

                    if (toolsResponse.data.error) {
                        throw new Error(`MCP tools/list failed: ${toolsResponse.data.error.message}`);
                    }

                    const serverTools = toolsResponse.data.result?.tools || [];

                    // 2. 拼接成一个列表，但是有一个注意的点，因为每个mcpServer 中的 tools 的名称可能重复，
                    // 所以在拼接成我们自己的 tools 列表的时候，需要用 mcpServer的名称 + mcpServer 中tool 的名称
                    const prefixedTools = serverTools.map((tool: any) => {
                        // 转换为 OpenAI 工具格式（只包含标准字段）
                        const openaiTool = {
                            type: "function",
                            function: {
                                name: `${mcpServer.name}_${tool.name}`,
                                description: tool.description,
                                parameters: tool.input_schema || tool.inputSchema || tool.parameters || {}
                            }
                        };

                        // 将元数据存储在单独的映射中，不发送给后端
                        this.toolMetadata = this.toolMetadata || new Map();
                        this.toolMetadata.set(`${mcpServer.name}_${tool.name}`, {
                            originalName: tool.name,
                            serverName: mcpServer.name,
                            serverUrl: mcpServer.url,
                            supportsStreaming: true  // 新框架中所有工具都支持流式输出
                        });

                        return openaiTool;
                    });

                    this.tools.push(...prefixedTools);
                } catch (error: any) {
                    console.error(`Failed to get tools from MCP server ${mcpServer.name}:`, error);
                    // 继续处理其他服务器，不因为一个服务器失败而中断
                }
            }
        } catch (error: any) {
            console.error('buildTools failed:', error);
            throw error;
        }
    }

    getTools(): Tool[] {
        return this.tools;
    }
}

export default McpToolExecute;
