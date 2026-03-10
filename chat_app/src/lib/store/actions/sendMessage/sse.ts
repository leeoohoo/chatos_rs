export const extractSseDataEvents = (source: string): { events: string[]; rest: string } => {
  const events: string[] = [];
  let cursor = 0;

  while (cursor < source.length) {
    const crlfIdx = source.indexOf('\r\n\r\n', cursor);
    const lfIdx = source.indexOf('\n\n', cursor);

    if (crlfIdx === -1 && lfIdx === -1) {
      break;
    }

    let boundary = -1;
    let separatorLength = 0;
    if (crlfIdx !== -1 && (lfIdx === -1 || crlfIdx < lfIdx)) {
      boundary = crlfIdx;
      separatorLength = 4;
    } else {
      boundary = lfIdx;
      separatorLength = 2;
    }

    const rawEvent = source.slice(cursor, boundary);
    cursor = boundary + separatorLength;

    const dataLines = rawEvent
      .split(/\r?\n/)
      .map((line) => line.trimStart())
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart());

    if (dataLines.length > 0) {
      events.push(dataLines.join('\n').trim());
    }
  }

  return { events, rest: source.slice(cursor) };
};
