declare module 'pngjs' {
  export class PNG {
    constructor(options: { width: number; height: number });
    width: number;
    height: number;
    data: Buffer;
    static sync: {
      read(data: Buffer): PNG;
      write(png: PNG): Buffer;
    };
  }
}

declare module 'ws' {
  export default class WebSocket {
    constructor(url: string);
    once(event: string, listener: (...argumentsValue: any[]) => void): this;
    on(event: string, listener: (...argumentsValue: any[]) => void): this;
    send(data: string): void;
    terminate(): void;
  }
}
