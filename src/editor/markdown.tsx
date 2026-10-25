/**
 * A small Markdown renderer for hover text from language servers: fenced
 * code blocks, paragraphs, rules, headings, and inline code, emphasis and
 * links (shown as their text). It builds React elements, never HTML, so
 * server text cannot inject markup.
 */
import type { ReactNode } from 'react'

const INLINE = /(`+)([\s\S]*?)\1|\*\*([^*]+)\*\*|\[([^\]]*)\]\([^)]*\)|\\([\\`*_[\]#>.!-])/g

function inline(text: string, key: string): ReactNode[] {
  const parts: ReactNode[] = []
  let at = 0
  let index = 0
  for (const match of text.matchAll(INLINE)) {
    if (match.index > at) {
      parts.push(text.slice(at, match.index))
    }
    const [, ticks, code, bold, link, escaped] = match
    const id = `${key}-${index++}`
    if (ticks !== undefined) {
      parts.push(<code key={id}>{code?.trim()}</code>)
    } else if (bold !== undefined) {
      parts.push(<strong key={id}>{bold}</strong>)
    } else if (link !== undefined) {
      parts.push(link)
    } else {
      parts.push(escaped)
    }
    at = match.index + match[0].length
  }
  if (at < text.length) {
    parts.push(text.slice(at))
  }
  return parts
}

export function renderMarkdown(markdown: string): ReactNode[] {
  const blocks: ReactNode[] = []
  const lines = markdown.replace(/\r\n?/g, '\n').split('\n')
  let paragraph: string[] = []
  const flush = () => {
    if (paragraph.length > 0) {
      const key = `p${blocks.length}`
      blocks.push(<p key={key}>{inline(paragraph.join(' '), key)}</p>)
      paragraph = []
    }
  }
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!
    const fence = /^\s*(```+|~~~+)/.exec(line)
    if (fence) {
      flush()
      const code: string[] = []
      for (i++; i < lines.length && !lines[i]!.trimStart().startsWith(fence[1]!); i++) {
        code.push(lines[i]!)
      }
      blocks.push(<pre key={`c${blocks.length}`}>{code.join('\n')}</pre>)
    } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) {
      flush()
      blocks.push(<hr key={`r${blocks.length}`} />)
    } else if (/^#{1,6}\s/.test(line)) {
      flush()
      const key = `h${blocks.length}`
      blocks.push(
        <p key={key}>
          <strong>{inline(line.replace(/^#+\s*/, ''), key)}</strong>
        </p>,
      )
    } else if (line.trim() === '') {
      flush()
    } else {
      paragraph.push(line.trim())
    }
  }
  flush()
  return blocks
}
