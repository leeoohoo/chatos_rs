/**
 * TypeScript 类型定义 - Electron API
 *
 * 定义通过 preload 脚本暴露的 Electron API 接口
 * 这个文件也会被导出到 npm 包中，供使用者参考
 */

interface ElectronAPI {
  /**
   * 打开应用窗口
   * @param appData - 应用数据
   * @returns Promise 返回操作结果
   */
  openAppWindow: (appData: {
    id: string;
    name: string;
    url: string;
    iconUrl?: string;
  }) => Promise<{ success: boolean; error?: string }>;

  /**
   * 关闭应用窗口
   * @param appId - 应用ID
   * @returns Promise 返回操作结果
   */
  closeAppWindow: (appId: string) => Promise<{ success: boolean; error?: string }>;

  /**
   * 获取所有打开的应用窗口ID列表
   * @returns Promise 返回打开的应用ID数组
   */
  getOpenAppWindows: () => Promise<{ success: boolean; data?: string[]; error?: string }>;

  /**
   * 检查是否在 Electron 环境
   * @returns Promise 返回是否在 Electron 环境
   */
  isElectron: () => Promise<{ success: boolean; data?: boolean }>;

  /**
   * 监听应用窗口关闭事件
   * @param callback - 回调函数，接收应用ID参数
   * @returns 取消监听的函数
   */
  onAppWindowClosed: (callback: (appId: string) => void) => (() => void);
}

// 扩展 Window 接口，添加 electronAPI
declare global {
  interface Window {
    electronAPI?: ElectronAPI;
  }
}

export {};
