# 模块控制参数使用指南

## 概述

AiChat 类支持通过构造函数参数来控制哪些管理模块在界面中显示，让开发者可以根据需要定制聊天界面的功能。

## 构造函数参数

```typescript
new AiChat(
  userId: string,                    // 用户ID
  projectId: string,                 // 项目ID  
  configUrl: string,                 // API基础URL
  className?: string,                // CSS类名
  showMcpManager?: boolean,          // 是否显示MCP服务管理 (默认: true)
  showAiModelManager?: boolean,      // 是否显示AI配置管理 (默认: true)
  showSystemContextEditor?: boolean  // 是否显示System Prompt编辑器 (默认: true)
)
```

## 模块说明

### 1. MCP服务管理 (showMcpManager)
- **功能**: 管理MCP (Model Context Protocol) 服务连接
- **包含**: 添加、删除、配置MCP服务器
- **适用场景**: 需要集成外部工具和服务的应用

### 2. AI配置管理 (showAiModelManager)  
- **功能**: 配置AI模型参数和设置
- **包含**: 模型选择、参数调整、API密钥配置
- **适用场景**: 需要让用户自定义AI行为的应用

### 3. System Prompt编辑器 (showSystemContextEditor)
- **功能**: 编辑系统提示词和上下文设置
- **包含**: 系统角色定义、行为指令设置
- **适用场景**: 需要定制AI角色和行为的应用

## 使用示例

### 完整功能版本
```typescript
const fullFeaturedChat = new AiChat(
  'user123', 'project456', 'http://localhost:8000/api', 'h-full w-full',
  true,  // 显示MCP服务管理
  true,  // 显示AI配置管理  
  true   // 显示System Prompt编辑器
);
```

### 简化聊天版本
```typescript
const simpleChatOnly = new AiChat(
  'user123', 'project456', 'http://localhost:8000/api', 'h-full w-full',
  false, // 隐藏MCP服务管理
  false, // 隐藏AI配置管理
  false  // 隐藏System Prompt编辑器
);
```

### 只显示AI配置管理
```typescript
const aiConfigOnly = new AiChat(
  'user123', 'project456', 'http://localhost:8000/api', 'h-full w-full',
  false, // 隐藏MCP服务管理
  true,  // 显示AI配置管理
  false  // 隐藏System Prompt编辑器
);
```

### 只显示MCP服务管理
```typescript
const mcpOnly = new AiChat(
  'user123', 'project456', 'http://localhost:8000/api', 'h-full w-full',
  true,  // 显示MCP服务管理
  false, // 隐藏AI配置管理
  false  // 隐藏System Prompt编辑器
);
```

### AI配置 + System Prompt编辑器
```typescript
const aiAndSystemOnly = new AiChat(
  'user123', 'project456', 'http://localhost:8000/api', 'h-full w-full',
  false, // 隐藏MCP服务管理
  true,  // 显示AI配置管理
  true   // 显示System Prompt编辑器
);
```

## 应用场景建议

### 1. 企业内部工具
- **推荐配置**: `(false, true, true)`
- **原因**: 企业通常不需要MCP服务，但需要AI配置和角色定制

### 2. 开发者工具
- **推荐配置**: `(true, true, true)`
- **原因**: 开发者需要完整的功能来测试和集成各种服务

### 3. 最终用户应用
- **推荐配置**: `(false, false, false)`
- **原因**: 最终用户只需要聊天功能，不需要复杂的配置界面

### 4. AI助手平台
- **推荐配置**: `(false, true, true)`
- **原因**: 需要AI配置和角色定制，但不需要MCP服务管理

### 5. 工具集成平台
- **推荐配置**: `(true, false, false)`
- **原因**: 主要关注工具和服务集成，AI配置相对固定

## 注意事项

1. **默认值**: 所有模块控制参数默认为 `true`，确保向后兼容性
2. **运行时修改**: 可以通过 `updateConfig()` 方法在运行时修改这些设置
3. **界面响应**: 隐藏的模块不会在界面上显示对应的按钮和功能
4. **功能完整性**: 即使隐藏了某些管理模块，核心聊天功能仍然完全可用

## 获取当前配置

```typescript
const currentConfig = aiChatInstance.getConfig();
console.log('MCP管理:', currentConfig.showMcpManager);
console.log('AI配置:', currentConfig.showAiModelManager);  
console.log('System Prompt:', currentConfig.showSystemContextEditor);
```

## 动态更新配置

```typescript
aiChatInstance.updateConfig({
  showMcpManager: false,
  showAiModelManager: true,
  showSystemContextEditor: true
});
```