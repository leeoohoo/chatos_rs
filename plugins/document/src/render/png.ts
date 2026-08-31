import { deflateSync, inflateSync } from 'node:zlib';
import { DocumentError } from '../errors.js';

const SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

const CRC_TABLE = Array.from({ length: 256 }, (_, value) => {
  let current = value;
  for (let bit = 0; bit < 8; bit += 1) {
    current = (current & 1) === 1 ? 0xedb88320 ^ (current >>> 1) : current >>> 1;
  }
  return current >>> 0;
});

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = (CRC_TABLE[(crc ^ byte) & 0xff] as number) ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Buffer): Buffer {
  const typeBytes = Buffer.from(type, 'ascii');
  const output = Buffer.allocUnsafe(12 + data.length);
  output.writeUInt32BE(data.length, 0);
  typeBytes.copy(output, 4);
  data.copy(output, 8);
  output.writeUInt32BE(crc32(output.subarray(4, 8 + data.length)), 8 + data.length);
  return output;
}

export function encodeRgbaPng(data: Uint8Array, width: number, height: number): Buffer {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1) {
    throw new DocumentError('VALIDATION_FAILED', 'Cannot encode a zero-sized PNG.');
  }
  const stride = width * 4;
  if (data.length !== stride * height) {
    throw new DocumentError('VALIDATION_FAILED', 'Rendered RGBA data has an unexpected size.');
  }
  const filtered = Buffer.allocUnsafe((stride + 1) * height);
  for (let row = 0; row < height; row += 1) {
    const targetOffset = row * (stride + 1);
    filtered[targetOffset] = 0;
    Buffer.from(data.buffer, data.byteOffset + row * stride, stride).copy(filtered, targetOffset + 1);
  }
  const header = Buffer.allocUnsafe(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  header[10] = 0;
  header[11] = 0;
  header[12] = 0;
  return Buffer.concat([
    SIGNATURE,
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(filtered, { level: 6 })),
    chunk('IEND', Buffer.alloc(0))
  ]);
}

export function inspectPng(bytes: Buffer): { width: number; height: number } {
  if (bytes.length < 45 || !bytes.subarray(0, 8).equals(SIGNATURE)) {
    throw new DocumentError('VALIDATION_FAILED', 'A rendered page is not a valid PNG.');
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let hasImageData = false;
  let hasEnd = false;
  while (offset + 12 <= bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const end = offset + 12 + length;
    if (end > bytes.length) throw new DocumentError('VALIDATION_FAILED', 'A rendered PNG is truncated.');
    const type = bytes.toString('ascii', offset + 4, offset + 8);
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    const expectedCrc = bytes.readUInt32BE(offset + 8 + length);
    if (crc32(bytes.subarray(offset + 4, offset + 8 + length)) !== expectedCrc) {
      throw new DocumentError('VALIDATION_FAILED', 'A rendered PNG failed its CRC check.');
    }
    if (type === 'IHDR') {
      if (length !== 13) throw new DocumentError('VALIDATION_FAILED', 'A rendered PNG has an invalid header.');
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
    } else if (type === 'IDAT' && length > 0) {
      hasImageData = true;
    } else if (type === 'IEND') {
      hasEnd = true;
      break;
    }
    offset = end;
  }
  if (width < 1 || height < 1 || !hasImageData || !hasEnd) {
    throw new DocumentError('VALIDATION_FAILED', 'A rendered page is not a valid non-empty PNG.');
  }
  return { width, height };
}

export interface DecodedPng {
  width: number;
  height: number;
  data: Buffer;
}

function paeth(left: number, above: number, upperLeft: number): number {
  const prediction = left + above - upperLeft;
  const leftDistance = Math.abs(prediction - left);
  const aboveDistance = Math.abs(prediction - above);
  const upperLeftDistance = Math.abs(prediction - upperLeft);
  if (leftDistance <= aboveDistance && leftDistance <= upperLeftDistance) return left;
  if (aboveDistance <= upperLeftDistance) return above;
  return upperLeft;
}

export function decodePng(bytes: Buffer): DecodedPng {
  inspectPng(bytes);
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = -1;
  let interlace = -1;
  const imageData: Buffer[] = [];
  while (offset + 12 <= bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('ascii', offset + 4, offset + 8);
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8] as number;
      colorType = data[9] as number;
      interlace = data[12] as number;
    } else if (type === 'IDAT') {
      imageData.push(data);
    } else if (type === 'IEND') {
      break;
    }
    offset += 12 + length;
  }
  if (bitDepth !== 8 || ![0, 2, 6].includes(colorType) || interlace !== 0) {
    throw new DocumentError('VALIDATION_FAILED', 'The rendered PNG uses an unsupported pixel format.');
  }
  const sourceChannels = colorType === 0 ? 1 : colorType === 2 ? 3 : 4;
  const stride = width * sourceChannels;
  let inflated: Buffer;
  try {
    inflated = inflateSync(Buffer.concat(imageData));
  } catch {
    throw new DocumentError('VALIDATION_FAILED', 'The rendered PNG image data could not be decompressed.');
  }
  if (inflated.length !== (stride + 1) * height) {
    throw new DocumentError('VALIDATION_FAILED', 'The rendered PNG image data has an unexpected size.');
  }
  const raw = Buffer.allocUnsafe(stride * height);
  for (let row = 0; row < height; row += 1) {
    const sourceOffset = row * (stride + 1);
    const targetOffset = row * stride;
    const filter = inflated[sourceOffset] as number;
    for (let index = 0; index < stride; index += 1) {
      const encoded = inflated[sourceOffset + 1 + index] as number;
      const left = index >= sourceChannels ? raw[targetOffset + index - sourceChannels] as number : 0;
      const above = row > 0 ? raw[targetOffset + index - stride] as number : 0;
      const upperLeft = row > 0 && index >= sourceChannels
        ? raw[targetOffset + index - stride - sourceChannels] as number
        : 0;
      let value: number;
      if (filter === 0) value = encoded;
      else if (filter === 1) value = encoded + left;
      else if (filter === 2) value = encoded + above;
      else if (filter === 3) value = encoded + Math.floor((left + above) / 2);
      else if (filter === 4) value = encoded + paeth(left, above, upperLeft);
      else throw new DocumentError('VALIDATION_FAILED', 'The rendered PNG uses an unsupported row filter.');
      raw[targetOffset + index] = value & 0xff;
    }
  }
  const rgba = Buffer.allocUnsafe(width * height * 4);
  for (let pixel = 0; pixel < width * height; pixel += 1) {
    const sourceOffset = pixel * sourceChannels;
    const targetOffset = pixel * 4;
    if (colorType === 0) {
      const gray = raw[sourceOffset] as number;
      rgba[targetOffset] = gray;
      rgba[targetOffset + 1] = gray;
      rgba[targetOffset + 2] = gray;
      rgba[targetOffset + 3] = 255;
    } else {
      rgba[targetOffset] = raw[sourceOffset] as number;
      rgba[targetOffset + 1] = raw[sourceOffset + 1] as number;
      rgba[targetOffset + 2] = raw[sourceOffset + 2] as number;
      rgba[targetOffset + 3] = colorType === 6 ? raw[sourceOffset + 3] as number : 255;
    }
  }
  return { width, height, data: rgba };
}

export function cropPng(image: DecodedPng, left: number, top: number, width: number, height: number): Buffer {
  if (left < 0 || top < 0 || width < 1 || height < 1 || left + width > image.width || top + height > image.height) {
    throw new DocumentError('VALIDATION_FAILED', 'A rendered page crop is outside the source PNG.');
  }
  const cropped = Buffer.allocUnsafe(width * height * 4);
  for (let row = 0; row < height; row += 1) {
    const sourceStart = ((top + row) * image.width + left) * 4;
    image.data.copy(cropped, row * width * 4, sourceStart, sourceStart + width * 4);
  }
  return encodeRgbaPng(cropped, width, height);
}

function differsFromBackground(data: Buffer, offset: number, background: readonly number[]): boolean {
  return Math.abs((data[offset] as number) - (background[0] as number))
    + Math.abs((data[offset + 1] as number) - (background[1] as number))
    + Math.abs((data[offset + 2] as number) - (background[2] as number)) > 18;
}

export function splitOfficeDocumentPageStack(bytes: Buffer): Buffer[] {
  const image = decodePng(bytes);
  const background = [image.data[0] as number, image.data[1] as number, image.data[2] as number] as const;
  const activeRows: boolean[] = [];
  const sampleStep = Math.max(1, Math.floor(image.width / 400));
  const minimumSamples = Math.max(8, Math.floor(image.width / sampleStep / 20));
  for (let row = 0; row < image.height; row += 1) {
    let changed = 0;
    for (let column = 0; column < image.width; column += sampleStep) {
      if (differsFromBackground(image.data, (row * image.width + column) * 4, background)) changed += 1;
    }
    activeRows[row] = changed >= minimumSamples;
  }
  const runs: Array<{ top: number; bottom: number }> = [];
  let start: number | undefined;
  for (let row = 0; row <= image.height; row += 1) {
    if (row < image.height && activeRows[row]) {
      start ??= row;
    } else if (start !== undefined) {
      if (row - start >= 100) runs.push({ top: start, bottom: row - 1 });
      start = undefined;
    }
  }
  return runs.map((run) => {
    let left = image.width;
    let right = -1;
    for (let row = run.top; row <= run.bottom; row += Math.max(1, Math.floor((run.bottom - run.top + 1) / 200))) {
      for (let column = 0; column < image.width; column += 1) {
        if (differsFromBackground(image.data, (row * image.width + column) * 4, background)) {
          left = Math.min(left, column);
          right = Math.max(right, column);
        }
      }
    }
    if (right < left) throw new DocumentError('VALIDATION_FAILED', 'A rendered DOCX page could not be isolated.');
    return cropPng(image, left, run.top, right - left + 1, run.bottom - run.top + 1);
  });
}
