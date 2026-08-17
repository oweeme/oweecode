// Tiny markdown-ish renderer shared by every place that shows raw AI output
// (AiPanel's chat, AgentTab's step list) — escapes untrusted text first, then
// only re-opens a few whitelisted tags, since a reply can echo back file/URL
// content the user asked it to read.
export function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

export function renderContent(text: string): string {
  return escHtml(text)
    .replace(/```(\w*)\n?([\s\S]*?)```/g, (_, lang, code) =>
      `<pre class="ai-code-block" data-lang="${lang || 'code'}"><div class="ai-code-lang">${lang || 'code'}</div><code>${code.trim()}</code></pre>`)
    .replace(/`([^`\n]+)`/g, '<code class="ai-inline-code">$1</code>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/^#{1,3} (.+)$/gm, '<p class="ai-heading">$1</p>')
    .replace(/\n/g, '<br>')
}
