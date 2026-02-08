// 简单的HTTP工具模块，用于替代外部依赖

interface HttpResponse<T = any> {
  data: T;
  status: number;
  statusText: string;
}

class HttpClient {
  private baseURL: string;
  private defaultHeaders: Record<string, string>;

  constructor(baseURL = '', defaultHeaders = {}) {
    this.baseURL = baseURL;
    this.defaultHeaders = {
      'Content-Type': 'application/json',
      ...defaultHeaders
    };
  }

  private async request<T>(
    url: string,
    options: RequestInit = {}
  ): Promise<HttpResponse<T>> {
    const fullUrl = this.baseURL + url;
    const config: RequestInit = {
      ...options,
      headers: {
        ...this.defaultHeaders,
        ...options.headers
      }
    };

    try {
      const response = await fetch(fullUrl, config);
      const data = await response.json();
      
      return {
        data,
        status: response.status,
        statusText: response.statusText
      };
    } catch (error) {
      console.error('HTTP request failed:', error);
      throw error;
    }
  }

  async get<T>(url: string, config?: RequestInit): Promise<HttpResponse<T>> {
    return this.request<T>(url, { ...config, method: 'GET' });
  }

  async post<T>(url: string, data?: any, config?: RequestInit): Promise<HttpResponse<T>> {
    return this.request<T>(url, {
      ...config,
      method: 'POST',
      body: data ? JSON.stringify(data) : undefined
    });
  }

  async put<T>(url: string, data?: any, config?: RequestInit): Promise<HttpResponse<T>> {
    return this.request<T>(url, {
      ...config,
      method: 'PUT',
      body: data ? JSON.stringify(data) : undefined
    });
  }

  async delete<T>(url: string, config?: RequestInit): Promise<HttpResponse<T>> {
    return this.request<T>(url, { ...config, method: 'DELETE' });
  }
}

// 创建默认实例
const http = new HttpClient();

export default http;
export { HttpClient };
export type { HttpResponse };