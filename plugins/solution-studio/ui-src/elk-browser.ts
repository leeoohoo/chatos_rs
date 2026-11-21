import ELK from 'elkjs/lib/elk-api.js';
import workerUrl from 'elkjs/lib/elk-worker.min.js?url';

export default class BrowserELK extends ELK {
  constructor() {
    super({ workerUrl });
  }
}
