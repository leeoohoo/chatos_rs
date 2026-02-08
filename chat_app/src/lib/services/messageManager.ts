import type { DatabaseService } from '../database';
import type { Message } from '../../types';

/**
 * 统一的消息保存管理器
 * 负责管理所有消息的保存逻辑，避免重复保存
 */
export class MessageManager {
  private pendingSaves = new Set<string>(); // 跟踪正在保存的消息ID
  private savedMessages = new Map<string, Message>(); // 缓存已保存的消息
  private databaseService: DatabaseService;

  constructor(databaseService: DatabaseService) {
    this.databaseService = databaseService;
  }

  /**
   * 保存用户消息
   */
  async saveUserMessage(data: Omit<Message, 'id'>): Promise<Message> {
    const messageKey = `${data.sessionId}-${data.role}-${data.content}-${data.createdAt?.getTime()}`;
    
    // 检查是否正在保存或已保存
    if (this.pendingSaves.has(messageKey)) {
      // 等待正在进行的保存完成
      while (this.pendingSaves.has(messageKey)) {
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      const saved = this.savedMessages.get(messageKey);
      if (saved) return saved;
    }

    this.pendingSaves.add(messageKey);
    
    try {
      const savedMessage = await this.databaseService.createMessage(data);
      this.savedMessages.set(messageKey, savedMessage);
      return savedMessage;
    } finally {
      this.pendingSaves.delete(messageKey);
    }
  }

  /**
   * 保存助手消息
   */
  async saveAssistantMessage(data: Omit<Message, 'id'>): Promise<Message> {
    const messageKey = `${data.sessionId}-${data.role}-${JSON.stringify(data.metadata?.toolCalls || [])}-${data.createdAt?.getTime()}`;
    
    // 检查是否正在保存或已保存
    if (this.pendingSaves.has(messageKey)) {
      // 等待正在进行的保存完成
      while (this.pendingSaves.has(messageKey)) {
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      const saved = this.savedMessages.get(messageKey);
      if (saved) return saved;
    }

    this.pendingSaves.add(messageKey);
    
    try {
      const savedMessage = await this.databaseService.createMessage(data);
      this.savedMessages.set(messageKey, savedMessage);
      return savedMessage;
    } finally {
      this.pendingSaves.delete(messageKey);
    }
  }

  /**
   * 保存工具调用结果消息
   */
  async saveToolMessage(data: Omit<Message, 'id'>): Promise<Message> {
    const messageKey = `${data.sessionId}-${data.role}-${data.content}-${data.createdAt?.getTime()}`;
    
    // 检查是否正在保存或已保存
    if (this.pendingSaves.has(messageKey)) {
      // 等待正在进行的保存完成
      while (this.pendingSaves.has(messageKey)) {
        await new Promise(resolve => setTimeout(resolve, 10));
      }
      const saved = this.savedMessages.get(messageKey);
      if (saved) return saved;
    }

    this.pendingSaves.add(messageKey);
    
    try {
      const savedMessage = await this.databaseService.createMessage(data);
      this.savedMessages.set(messageKey, savedMessage);
      return savedMessage;
    } finally {
      this.pendingSaves.delete(messageKey);
    }
  }

  /**
   * 通用保存消息方法
   */
  async saveMessage(data: Omit<Message, 'id'>): Promise<Message> {
    switch (data.role) {
      case 'user':
        return this.saveUserMessage(data);
      case 'assistant':
        return this.saveAssistantMessage(data);
      case 'tool':
        return this.saveToolMessage(data);
      default:
        // 对于其他角色，直接保存
        return this.databaseService.createMessage(data);
    }
  }

  /**
   * 清理缓存（可选，用于内存管理）
   */
  clearCache(): void {
    this.savedMessages.clear();
  }

  /**
   * 获取缓存统计信息
   */
  getCacheStats(): { pendingCount: number; cachedCount: number } {
    return {
      pendingCount: this.pendingSaves.size,
      cachedCount: this.savedMessages.size
    };
  }
}