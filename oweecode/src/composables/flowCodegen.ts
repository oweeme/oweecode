// Turns a business flow (blocks + arrows) into a code skeleton in the language
// of the user's project: one function per block plus run() that follows the
// arrows (conditions become if/else on their Sí/No outputs).

export type CodeLang = 'js' | 'ts' | 'python' | 'php' | 'go' | 'rust' | 'kotlin' | 'dart' | 'qtcpp' | 'qml'

export const CODE_LANGS: { key: CodeLang; label: string; ext: string }[] = [
  { key: 'js', label: 'JavaScript', ext: 'js' },
  { key: 'ts', label: 'TypeScript', ext: 'ts' },
  { key: 'python', label: 'Python', ext: 'py' },
  { key: 'php', label: 'PHP', ext: 'php' },
  { key: 'go', label: 'Go', ext: 'go' },
  { key: 'rust', label: 'Rust', ext: 'rs' },
  { key: 'kotlin', label: 'Kotlin (Android)', ext: 'kt' },
  { key: 'dart', label: 'Dart / Flutter', ext: 'dart' },
  { key: 'qtcpp', label: 'Qt C++ (QtNetwork)', ext: 'cpp' },
  { key: 'qml', label: 'QML / JavaScript (Qt)', ext: 'js' },
]

interface FlowNode { id: string; data: { kind: string; label: string; desc?: string; http?: { method?: string; url?: string; headers?: string; body?: string } } }
interface FlowEdge { source: string; target: string; sourceHandle?: string | null }

type Stmt =
  | { t: 'call'; fn: string }
  | { t: 'if'; fn: string; yes: Stmt[]; no: Stmt[] }
  | { t: 'note'; text: string }

function words(label: string): string[] {
  const w = String(label ?? '').normalize('NFD').replace(/[̀-ͯ]/g, '').split(/[^a-zA-Z0-9]+/).filter(Boolean).map(x => x.toLowerCase())
  if (!w.length) return ['paso']
  if (/^\d/.test(w[0])) w.unshift('paso')
  return w
}
const camel = (w: string[]) => w[0] + w.slice(1).map(x => x[0].toUpperCase() + x.slice(1)).join('')
const snake = (w: string[]) => w.join('_')

function buildTree(nodes: FlowNode[], edges: FlowEdge[], name: (n: FlowNode) => string): { body: Stmt[]; steps: FlowNode[] } {
  const byId = new Map(nodes.map(n => [n.id, n]))
  const out = new Map<string, FlowEdge[]>()
  for (const e of edges) { if (!out.has(e.source)) out.set(e.source, []); out.get(e.source)!.push(e) }
  const seen = new Set<string>()
  const walk = (id: string, into: Stmt[]) => {
    const n = byId.get(id)
    if (!n || n.data.kind === 'note') return
    if (n.data.kind === 'end') { into.push({ t: 'note', text: /^(fin|end)$/i.test(n.data.label.trim()) ? 'Fin del flujo' : `Fin: ${n.data.label}` }); return }
    if (seen.has(id)) { into.push({ t: 'note', text: `continúa en "${n.data.label}"` }); return }
    seen.add(id)
    const outs = out.get(id) ?? []
    if (n.data.kind === 'condition') {
      const st: Stmt = { t: 'if', fn: name(n), yes: [], no: [] }
      outs.filter(e => e.sourceHandle !== 'no').forEach(e => walk(e.target, st.yes))
      outs.filter(e => e.sourceHandle === 'no').forEach(e => walk(e.target, st.no))
      into.push(st)
    } else {
      into.push({ t: 'call', fn: name(n) })
      outs.forEach(e => walk(e.target, into))
    }
  }
  const body: Stmt[] = []
  const starts = nodes.filter(n => n.data.kind === 'trigger')
  ;(starts.length ? starts : nodes.filter(n => !edges.some(e => e.target === n.id))).forEach(n => walk(n.id, body))
  return { body, steps: nodes.filter(n => n.data.kind !== 'note' && n.data.kind !== 'end') }
}

const KIND_HINT: Record<string, string> = {
  trigger: 'Datos de entrada del evento (llegan en ctx)',
  http: 'Llamada a una API externa',
  db: 'Consulta / escritura en la base de datos',
  email: 'Enviar notificación (correo, WhatsApp, push)',
  condition: 'Devolver verdadero (Sí) o falso (No)',
  function: 'Lógica de negocio',
}


// "https://api/x/{{id}}?q={{a.b}}" -> literal and variable parts
function splitTemplate(t: string): { lit?: string; v?: string[] }[] {
  const out: { lit?: string; v?: string[] }[] = []
  let last = 0
  for (const m of String(t ?? '').matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)) {
    if (m.index! > last) out.push({ lit: t.slice(last, m.index) })
    out.push({ v: m[1].replace(/^ctx\./, '').split('.') })
    last = m.index! + m[0].length
  }
  if (last < t.length) out.push({ lit: t.slice(last) })
  return out
}
const q = (x: string) => JSON.stringify(x)   // C++/JS string literal

// QString expression for a template, reading variables from a QJsonObject `ctx`
// (plain concatenation: QString::arg would misread a literal "%20" in a URL)
function qtString(t: string): string {
  const parts = splitTemplate(t)
  if (!parts.some(p => p.v)) return `QStringLiteral(${q(t)})`
  const exprs = parts.map(p => p.lit !== undefined
    ? `QStringLiteral(${q(p.lit)})`
    : p.v!.map((k, i) => (i === 0 ? `ctx.value(${q(k)})` : `.toObject().value(${q(k)})`)).join('') + '.toVariant().toString()')
  return `(${exprs.join(' + ')})`
}
// JS expression (template literal) for QML/JS
function jsString(t: string): string {
  const parts = splitTemplate(t)
  if (!parts.some(p => p.v)) return q(t)
  return '`' + parts.map(p => p.lit !== undefined ? p.lit.replace(/[`\\]|\$\{/g, m => '\\' + m) : '${ctx.' + p.v!.join('.') + '}').join('') + '`'
}
function parseHeaders(raw?: string): [string, string][] {
  try { const o = JSON.parse(raw ?? ''); return Object.entries(o).map(([k, v]) => [k, String(v)]) } catch { return [] }
}

export function buildCode(nodes: FlowNode[], edges: FlowEdge[], lang: CodeLang, flowName: string): string {
  const style = lang === 'python' || lang === 'rust' ? snake : camel
  const name = (n: FlowNode) => style(words(n.data.label))
  const { body, steps } = buildTree(nodes, edges, name)
  const header = `Generado por OweeCode — flujo "${flowName}". Completá los TODO.`
  const hints = (n: FlowNode, c: string) => {
    const l: string[] = []
    if (n.data.desc) String(n.data.desc).split('\n').filter(Boolean).forEach(x => l.push(x))
    l.push(`TODO: ${KIND_HINT[n.data.kind] ?? 'lógica'}`)
    return l.map(x => `${c}${x}`)
  }
  const ind = (lines: string[], n: number) => lines.map(l => (l ? ' '.repeat(n) + l : l))
  const L: string[] = []

  // ── per-language rendering of the orchestration tree ──
  const unit = lang === 'js' || lang === 'ts' || lang === 'dart' ? 2 : 4
  const render = (stmts: Stmt[], _depth: number, f: {
    call: (fn: string) => string[]; ifOpen: (fn: string) => string[]; elseOpen: string[]; close: string[]; note: (t: string) => string; pass?: string
  }): string[] => {
    const res: string[] = []
    for (const s of stmts) {
      if (s.t === 'note') res.push(f.note(s.text))
      else if (s.t === 'call') res.push(...f.call(s.fn))
      else {
        res.push(...f.ifOpen(s.fn))
        // Python needs a real statement in every block (a comment alone is a syntax error)
        const block = (b: Stmt[]) => { const r = render(b, 1, f); return f.pass && !r.some(l => !l.trim().startsWith('#')) ? [...r, f.pass] : r }
        res.push(...ind(block(s.yes), unit))
        if (s.no.length || f.pass) {
          res.push(...f.elseOpen)
          res.push(...ind(block(s.no), unit))
        }
        res.push(...f.close)
      }
    }
    return res
  }

  if (lang === 'js' || lang === 'ts') {
    const ts = lang === 'ts'
    L.push(`// ${header}`, '')
    if (ts) L.push('type Ctx = Record<string, any>', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`async function ${name(n)}(ctx${ts ? ': Ctx' : ''})${ts ? `: Promise<${cond ? 'boolean' : 'Ctx'}>` : ''} {`)
      L.push(...ind(hints(n, '// '), 2), ...ind([cond ? 'return false' : 'return ctx'], 2), '}', '')
    }
    L.push(`export async function run(input${ts ? ': Ctx' : ''} = {})${ts ? ': Promise<Ctx>' : ''} {`, '  let ctx = { ...input }')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = await ${fn}(ctx)`], ifOpen: fn => [`if (await ${fn}(ctx)) {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 2), '  return ctx', '}')
  } else if (lang === 'python') {
    L.push(`# ${header}`, 'import asyncio', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push('', `async def ${name(n)}(ctx: dict) -> ${cond ? 'bool' : 'dict'}:`)
      L.push(...ind(hints(n, '# '), 4), ...ind([cond ? 'return False' : 'return ctx'], 4))
    }
    L.push('', '', 'async def run(input: dict | None = None) -> dict:', '    ctx = dict(input or {})')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = await ${fn}(ctx)`], ifOpen: fn => [`if await ${fn}(ctx):`], elseOpen: ['else:'], close: [], note: t => `# ${t}`, pass: 'pass',
    }), 4), '    return ctx', '', '', 'if __name__ == "__main__":', '    print(asyncio.run(run()))')
  } else if (lang === 'php') {
    L.push('<?php', `// ${header}`, '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`function ${name(n)}(array $ctx): ${cond ? 'bool' : 'array'} {`)
      L.push(...ind(hints(n, '// '), 4), ...ind([cond ? 'return false;' : 'return $ctx;'], 4), '}', '')
    }
    L.push('function run(array $input = []): array {', '    $ctx = $input;')
    L.push(...ind(render(body, 0, {
      call: fn => [`$ctx = ${fn}($ctx);`], ifOpen: fn => [`if (${fn}($ctx)) {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 4), '    return $ctx;', '}')
  } else if (lang === 'go') {
    L.push(`// ${header}`, 'package flows', '', 'type Ctx = map[string]any', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`func ${name(n)}(ctx Ctx) (${cond ? 'bool' : 'Ctx'}, error) {`)
      L.push(...ind(hints(n, '// '), 1).map(x => x.replace(/^ /, '\t')), cond ? '\treturn false, nil' : '\treturn ctx, nil', '}', '')
    }
    L.push('func Run(input Ctx) (Ctx, error) {', '\tctx := Ctx{}', '\tfor k, v := range input {', '\t\tctx[k] = v', '\t}', '\tvar err error', '\t_ = err')
    const goBody = render(body, 0, {
      call: fn => [`if ctx, err = ${fn}(ctx); err != nil {`, '    return ctx, err', '}'],
      ifOpen: fn => [`if ok, err := ${fn}(ctx); err != nil {`, '    return ctx, err', `} else if ok {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    })
    L.push(...goBody.map(l => '\t' + l.replace(/^( {4})+/, m => '\t'.repeat(m.length / 4))), '\treturn ctx, nil', '}')
  } else if (lang === 'kotlin') {
    L.push(`// ${header}`, 'typealias Ctx = MutableMap<String, Any?>', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`suspend fun ${name(n)}(ctx: Ctx): ${cond ? 'Boolean' : 'Ctx'} {`)
      L.push(...ind(hints(n, '// '), 4), ...ind([cond ? 'return false' : 'return ctx'], 4), '}', '')
    }
    L.push('suspend fun run(input: Ctx = mutableMapOf()): Ctx {', '    var ctx: Ctx = input.toMutableMap()')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = ${fn}(ctx)`], ifOpen: fn => [`if (${fn}(ctx)) {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 4), '    return ctx', '}')
  } else if (lang === 'dart') {
    L.push(`// ${header}`, 'typedef Ctx = Map<String, dynamic>;', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`Future<${cond ? 'bool' : 'Ctx'}> ${name(n)}(Ctx ctx) async {`)
      L.push(...ind(hints(n, '// '), 2), ...ind([cond ? 'return false;' : 'return ctx;'], 2), '}', '')
    }
    L.push('Future<Ctx> run([Ctx input = const {}]) async {', '  Ctx ctx = {...input};')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = await ${fn}(ctx);`], ifOpen: fn => [`if (await ${fn}(ctx)) {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 2), '  return ctx;', '}')
  } else if (lang === 'qtcpp') {
    L.push(`// ${header}`, '// CMake: find_package(Qt6 REQUIRED COMPONENTS Core Network)  y  target_link_libraries(app PRIVATE Qt6::Core Qt6::Network)', '',
      '#include <QEventLoop>', '#include <QJsonArray>', '#include <QJsonDocument>', '#include <QJsonObject>', '#include <QNetworkAccessManager>',
      '#include <QNetworkReply>', '#include <QNetworkRequest>', '#include <QUrl>', '', 'using Ctx = QJsonObject;', '',
      '// Petición HTTP bloqueante (usa un QEventLoop interno). En una app con interfaz, llamá a run() desde un hilo de trabajo (QtConcurrent / QThread)',
      '// o pasá a la versión asíncrona con señales si no querés bloquear la interfaz.',
      'static QJsonObject httpRequest(const QString &method, const QUrl &url, const QByteArray &body = {},',
      '                               const QList<QPair<QByteArray, QByteArray>> &headers = {}) {',
      '    QNetworkAccessManager mgr;', '    QNetworkRequest req(url);', '    req.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");',
      '    for (const auto &h : headers) req.setRawHeader(h.first, h.second);',
      '    QNetworkReply *reply = mgr.sendCustomRequest(req, method.toUtf8(), body);', '    QEventLoop loop;',
      '    QObject::connect(reply, &QNetworkReply::finished, &loop, &QEventLoop::quit);', '    loop.exec();',
      '    const QByteArray data = reply->readAll();', '    QJsonObject out;', '    out["status"] = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();',
      '    QJsonParseError err;', '    const QJsonDocument doc = QJsonDocument::fromJson(data, &err);',
      '    if (err.error == QJsonParseError::NoError) out["body"] = doc.isObject() ? QJsonValue(doc.object()) : QJsonValue(doc.array());',
      '    else out["body"] = QString::fromUtf8(data);', '    reply->deleteLater();', '    return out;', '}', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`static ${cond ? 'bool' : 'Ctx'} ${name(n)}(${cond ? 'const Ctx &ctx' : 'Ctx ctx'}) {`)
      if (n.data.kind === 'http' && n.data.http?.url) {
        const h = n.data.http, url = h.url!
        L.push(...ind(hints(n, '// ').slice(0, -1), 4))
        const hdrs = parseHeaders(h.headers).map(([k, v]) => `{${q(k)}, ${qtString(v)}.toUtf8()}`)
        L.push(`    ctx["http"] = httpRequest(${q((h.method || 'GET').toUpperCase())}, QUrl(${qtString(url)}),`,
          `                              ${h.body ? `${qtString(h.body)}.toUtf8()` : 'QByteArray()'}, {${hdrs.join(', ')}});`)
        if (h.headers && !hdrs.length) L.push(`    // TODO: headers: ${h.headers}`)
      } else L.push(...ind(hints(n, '// '), 4))
      L.push(cond ? '    return false;' : '    return ctx;', '}', '')
    }
    L.push('Ctx run(Ctx input = {}) {', '    Ctx ctx = input;')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = ${fn}(ctx);`], ifOpen: fn => [`if (${fn}(ctx)) {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 4), '    return ctx;', '}')
  } else if (lang === 'qml') {
    L.push(`// ${header}`, '// Usalo desde QML:  import "flows/' + flowName + '.js" as Flow   ->   Flow.run({}, function(ctx) { console.log(JSON.stringify(ctx)) })', '.pragma library', '',
      'function http(method, url, body, headers, cb) {', '    var xhr = new XMLHttpRequest()', '    xhr.onreadystatechange = function() {',
      '        if (xhr.readyState !== XMLHttpRequest.DONE) return', '        var b = xhr.responseText', '        try { b = JSON.parse(b) } catch (e) { /* texto */ }',
      '        cb({ status: xhr.status, body: b })', '    }', '    xhr.open(method, url)', '    xhr.setRequestHeader("Content-Type", "application/json")',
      '    for (var k in headers) xhr.setRequestHeader(k, headers[k])', '    xhr.send(body)', '}', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(cond ? `function ${name(n)}(ctx) {` : `function ${name(n)}(ctx, done) {`)
      if (n.data.kind === 'http' && n.data.http?.url) {
        const h = n.data.http, url = h.url!
        L.push(...ind(hints(n, '// ').slice(0, -1), 4))
        const hdr = parseHeaders(h.headers).map(([k, v]) => `${q(k)}: ${jsString(v)}`).join(', ')
        L.push(`    http(${q((h.method || 'GET').toUpperCase())}, ${jsString(url)}, ${h.body ? jsString(h.body) : 'null'}, {${hdr}}, function(res) {`, '        ctx.http = res', '        done(ctx)', '    })')
      } else { L.push(...ind(hints(n, '// '), 4)); L.push(cond ? '    return false' : '    done(ctx)') }
      L.push('}', '')
    }
    L.push('function run(input, done) {', '    var ctx = JSON.parse(JSON.stringify(input || {}))')
    const cps = (stmts: Stmt[], i: number): string[] => {
      if (i >= stmts.length) return ['done(ctx)']
      const st = stmts[i]
      if (st.t === 'note') return [`// ${st.text}`, ...cps(stmts, i + 1)]
      if (st.t === 'call') return [`${st.fn}(ctx, function(ctx) {`, ...ind(cps(stmts, i + 1), 4), '})']
      return [`if (${st.fn}(ctx)) {`, ...ind(cps(st.yes, 0), 4), '} else {', ...ind(cps(st.no, 0), 4), '}', ...(i + 1 < stmts.length ? cps(stmts, i + 1) : [])]
    }
    L.push(...ind(cps(body, 0), 4), '}')
  } else {
    L.push(`// ${header}`, '// Cargo.toml: serde_json = "1"', 'use serde_json::Value as Ctx;', 'type Res<T> = Result<T, Box<dyn std::error::Error>>;', '')
    for (const n of steps) {
      const cond = n.data.kind === 'condition'
      L.push(`fn ${name(n)}(${cond ? 'ctx: &Ctx' : 'ctx: Ctx'}) -> Res<${cond ? 'bool' : 'Ctx'}> {`)
      L.push(...ind(hints(n, '// '), 4), ...ind([cond ? 'let _ = ctx;' : '', cond ? 'Ok(false)' : 'Ok(ctx)'].filter(Boolean), 4), '}', '')
    }
    L.push('pub fn run(input: Ctx) -> Res<Ctx> {', '    let mut ctx = input;')
    L.push(...ind(render(body, 0, {
      call: fn => [`ctx = ${fn}(ctx)?;`], ifOpen: fn => [`if ${fn}(&ctx)? {`], elseOpen: ['} else {'], close: ['}'], note: t => `// ${t}`,
    }), 4), '    Ok(ctx)', '}')
  }
  return L.join('\n') + '\n'
}
