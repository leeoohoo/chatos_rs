type MarkdownNode =
  | { type: 'text'; text: string }
  | {
      name: string
      attrs?: Record<string, string>
      children?: MarkdownNode[]
    }

const text = (value: string): MarkdownNode => ({ type: 'text', text: value })

const element = (
  name: string,
  className: string,
  children: MarkdownNode[],
  attrs: Record<string, string> = {},
): MarkdownNode => ({
  name,
  attrs: { ...attrs, class: className },
  children,
})

function textWithBreaks(value: string): MarkdownNode[] {
  const parts = value.split('\n')
  const nodes: MarkdownNode[] = []
  parts.forEach((part, index) => {
    if (index > 0) nodes.push({ name: 'br' })
    if (part) nodes.push(text(part))
  })
  return nodes
}

const inlinePattern = /(`[^`\n]+`|\*\*[^\n]+?\*\*|__[^\n]+?__|~~[^\n]+?~~|\*[^*\n]+\*|\[[^\]\n]+\]\((?:https?:\/\/|mailto:)[^)\s]+\)|https?:\/\/[^\s<]+)/

function inlineNodes(source: string): MarkdownNode[] {
  const nodes: MarkdownNode[] = []
  let remaining = source

  while (remaining) {
    const match = remaining.match(inlinePattern)
    if (!match || match.index === undefined) {
      nodes.push(...textWithBreaks(remaining))
      break
    }
    if (match.index > 0) nodes.push(...textWithBreaks(remaining.slice(0, match.index)))

    const token = match[0]
    if (token.startsWith('`')) {
      nodes.push(element('code', 'md-inline-code', [text(token.slice(1, -1))]))
    } else if (token.startsWith('**') || token.startsWith('__')) {
      nodes.push(element('strong', 'md-strong', inlineNodes(token.slice(2, -2))))
    } else if (token.startsWith('~~')) {
      nodes.push(element('del', 'md-delete', inlineNodes(token.slice(2, -2))))
    } else if (token.startsWith('*')) {
      nodes.push(element('em', 'md-emphasis', inlineNodes(token.slice(1, -1))))
    } else if (token.startsWith('[')) {
      const link = token.match(/^\[([^\]]+)]\(([^)]+)\)$/)
      if (link) {
        nodes.push(element('a', 'md-link', inlineNodes(link[1]), { href: link[2] }))
      } else {
        nodes.push(text(token))
      }
    } else {
      nodes.push(element('a', 'md-link', [text(token)], { href: token }))
    }
    remaining = remaining.slice(match.index + token.length)
  }

  return nodes
}

function isFence(line: string): boolean {
  return /^\s*```/.test(line)
}

function isHeading(line: string): boolean {
  return /^\s{0,3}#{1,6}\s+/.test(line)
}

function isRule(line: string): boolean {
  return /^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$/.test(line)
}

function isQuote(line: string): boolean {
  return /^\s{0,3}>\s?/.test(line)
}

function isUnordered(line: string): boolean {
  return /^\s*[-+*]\s+/.test(line)
}

function isOrdered(line: string): boolean {
  return /^\s*\d+[.)]\s+/.test(line)
}

function isTableSeparator(line: string): boolean {
  const cells = splitTableRow(line)
  return cells.length > 0 && cells.every((cell) => /^:?-{3,}:?$/.test(cell))
}

function splitTableRow(line: string): string[] {
  const normalized = line.trim().replace(/^\|/, '').replace(/\|$/, '')
  if (!normalized.includes('|')) return []
  return normalized.split('|').map((cell) => cell.trim())
}

function isBlockStart(lines: string[], index: number): boolean {
  const line = lines[index] ?? ''
  return !line.trim() || isFence(line) || isHeading(line) || isRule(line) ||
    isQuote(line) || isUnordered(line) || isOrdered(line) ||
    (index + 1 < lines.length && splitTableRow(line).length > 0 && isTableSeparator(lines[index + 1]))
}

export function parseMarkdown(markdown: string): MarkdownNode[] {
  const lines = String(markdown ?? '').replace(/\r\n?/g, '\n').split('\n')
  const nodes: MarkdownNode[] = []
  let index = 0

  while (index < lines.length) {
    const line = lines[index]
    if (!line.trim()) {
      index += 1
      continue
    }

    const fence = line.match(/^\s*```\s*([^`]*)$/)
    if (fence) {
      const language = fence[1].trim()
      const code: string[] = []
      index += 1
      while (index < lines.length && !/^\s*```\s*$/.test(lines[index])) {
        code.push(lines[index])
        index += 1
      }
      if (index < lines.length) index += 1
      const children: MarkdownNode[] = []
      if (language) children.push(element('div', 'md-code-language', [text(language)]))
      children.push(element('code', 'md-code-content', [text(code.join('\n'))]))
      nodes.push(element('pre', 'md-code-block', children))
      continue
    }

    const heading = line.match(/^\s{0,3}(#{1,6})\s+(.+?)\s*#*\s*$/)
    if (heading) {
      const level = Math.min(heading[1].length, 4)
      nodes.push(element(`h${level}`, `md-heading md-h${level}`, inlineNodes(heading[2])))
      index += 1
      continue
    }

    if (isRule(line)) {
      nodes.push(element('hr', 'md-rule', []))
      index += 1
      continue
    }

    if (index + 1 < lines.length && splitTableRow(line).length > 0 && isTableSeparator(lines[index + 1])) {
      const headers = splitTableRow(line)
      index += 2
      const rows: string[][] = []
      while (index < lines.length) {
        const cells = splitTableRow(lines[index])
        if (!cells.length) break
        rows.push(cells)
        index += 1
      }
      const head = element('thead', 'md-table-head', [
        element('tr', 'md-table-row', headers.map((cell) =>
          element('th', 'md-table-cell md-table-header-cell', inlineNodes(cell)))),
      ])
      const body = element('tbody', 'md-table-body', rows.map((row) =>
        element('tr', 'md-table-row', headers.map((_, cellIndex) =>
          element('td', 'md-table-cell', inlineNodes(row[cellIndex] ?? ''))))))
      nodes.push(element('table', 'md-table', [head, body]))
      continue
    }

    if (isQuote(line)) {
      const quote: string[] = []
      while (index < lines.length && isQuote(lines[index])) {
        quote.push(lines[index].replace(/^\s{0,3}>\s?/, ''))
        index += 1
      }
      nodes.push(element('blockquote', 'md-quote', inlineNodes(quote.join('\n'))))
      continue
    }

    if (isUnordered(line) || isOrdered(line)) {
      const ordered = isOrdered(line)
      const initialNumber = ordered
        ? Number(line.match(/^\s*(\d+)[.)]\s+/)?.[1] ?? 1)
        : 1
      const items: MarkdownNode[] = []
      while (index < lines.length && (ordered ? isOrdered(lines[index]) : isUnordered(lines[index]))) {
        const content = lines[index].replace(ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-+*]\s+/, '')
        const task = !ordered ? content.match(/^\[([ xX])]\s+(.*)$/) : null
        const itemChildren = task
          ? [element('span', `md-check ${task[1].trim() ? 'md-check-done' : ''}`, [text(task[1].trim() ? '✓' : '')]), ...inlineNodes(task[2])]
          : inlineNodes(content)
        items.push(element('li', task ? 'md-list-item md-task-item' : 'md-list-item', itemChildren))
        index += 1
      }
      nodes.push(element(
        ordered ? 'ol' : 'ul',
        `md-list md-${ordered ? 'ordered' : 'unordered'}`,
        items,
        ordered && initialNumber > 1 ? { start: String(initialNumber) } : {},
      ))
      continue
    }

    const paragraph: string[] = [line]
    index += 1
    while (index < lines.length && !isBlockStart(lines, index)) {
      paragraph.push(lines[index])
      index += 1
    }
    nodes.push(element('p', 'md-paragraph', inlineNodes(paragraph.join('\n'))))
  }

  return nodes
}

Component({
  properties: {
    content: { type: String, value: '' },
    variant: { type: String, value: 'default' },
    compact: { type: Boolean, value: false },
  },
  data: {
    nodes: [] as MarkdownNode[],
  },
  observers: {
    content(value: string) {
      this.setData({ nodes: parseMarkdown(value) })
    },
  },
})
