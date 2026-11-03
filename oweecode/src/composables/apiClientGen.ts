// Generates an API client (Qt C++, QML/JS or TypeScript) from a saved request collection.
// Each request becomes one method; {{placeholders}} in the URL, query, headers and body
// become the method's parameters.

export type ClientLang = 'qtcpp' | 'qml' | 'ts'

export const CLIENT_LANGS: { key: ClientLang; label: string; hint: string }[] = [
  { key: 'qtcpp', label: 'Qt C++ (QtNetwork, asíncrono)', hint: 'Un .h con una clase lista para usar; necesita Qt6::Network' },
  { key: 'qml', label: 'QML / JavaScript (Qt)', hint: 'Un módulo .js con XMLHttpRequest, usable desde QML' },
  { key: 'ts', label: 'TypeScript (fetch) — Capacitor / Vue / Quasar', hint: 'Una clase con fetch, para tus apps web y Capacitor' },
]

interface KV { key: string; value: string; enabled: boolean }
interface Req { name: string; method: string; url: string; headers: KV[]; params: KV[]; bodyType: string; body: string; auth: { type: string; apiKeyName?: string; apiKeyIn?: string } }
interface Coll { name: string; requests: Req[] }
export interface GenFile { path: string; content: string }

const words = (s: string) => {
  const w = String(s ?? '').normalize('NFD').replace(/[̀-ͯ]/g, '').split(/[^a-zA-Z0-9]+/).filter(Boolean)
  if (!w.length) return ['request']
  return /^\d/.test(w[0]) ? ['r', ...w] : w
}
const camel = (s: string) => { if (/^[a-z][A-Za-z0-9]*$/.test(s)) return s; const w = words(s).map(x => x.toLowerCase()); return w[0] + w.slice(1).map(x => x[0].toUpperCase() + x.slice(1)).join('') }
const pascal = (s: string) => words(s).map(x => x[0].toUpperCase() + x.slice(1).toLowerCase()).join('')
const q = (s: string) => JSON.stringify(s)
const varsOf = (t: string) => [...String(t ?? '').matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)].map(m => camel(m[1].split('.')[0]))

interface Part { lit?: string; v?: string }
function split(t: string): Part[] {
  const out: Part[] = []; let last = 0
  for (const m of String(t ?? '').matchAll(/\{\{\s*([\w.]+)\s*\}\}/g)) {
    if (m.index! > last) out.push({ lit: t.slice(last, m.index) })
    out.push({ v: camel(m[1].split('.')[0]) }); last = m.index! + m[0].length
  }
  if (last < t.length) out.push({ lit: t.slice(last) })
  return out
}

// Common origin of all URLs (https://host) -> becomes the baseUrl property
function commonOrigin(reqs: Req[]): string {
  const origins = reqs.map(r => r.url.match(/^(https?:\/\/[^/{?#]+)/)?.[1]).filter(Boolean) as string[]
  return origins.length && origins.every(o => o === origins[0]) ? origins[0] : ''
}

function prep(c: Coll) {
  const base = commonOrigin(c.requests)
  const used = new Set<string>()
  const list = c.requests.map(r => {
    let name = camel(r.name || r.method + ' ' + r.url); let n = 2
    while (used.has(name)) name = camel(r.name) + n++
    used.add(name)
    const path = base && r.url.startsWith(base) ? r.url.slice(base.length) : r.url
    const query = (r.params ?? []).filter(p => p.enabled && p.key)
    const headers = (r.headers ?? []).filter(h => h.enabled && h.key && h.key.toLowerCase() !== 'content-type')
    const hasBody = r.bodyType !== 'none' && !!r.body?.trim()
    const params = [...new Set([...varsOf(path), ...query.flatMap(p => varsOf(p.value)), ...headers.flatMap(h => varsOf(h.value)), ...(hasBody ? varsOf(r.body) : [])])]
    return { r, name, path, query, headers, hasBody, params, method: (r.method || 'GET').toUpperCase() }
  })
  return { base, list }
}

// ── TypeScript ──
function genTs(c: Coll): GenFile[] {
  const { base, list } = prep(c)
  const tpl = (t: string) => { const p = split(t); return p.some(x => x.v) ? '`' + p.map(x => x.lit !== undefined ? x.lit.replace(/[`\\]|\$\{/g, m => '\\' + m) : '${' + x.v + '}').join('') + '`' : q(t) }
  const cls = pascal(c.name) + 'Client'
  const L = [`// Generado por OweeCode desde la colección "${c.name}".`, '',
    `export class ${cls} {`, `  constructor(public baseUrl = ${q(base)}, public token = '', public apiKey = '') {}`, '',
    '  private async request<T = any>(method: string, path: string, body?: string, headers: Record<string, string> = {}): Promise<T> {',
    "    const h: Record<string, string> = { 'Content-Type': 'application/json', ...headers }",
    "    if (this.token) h['Authorization'] = `Bearer ${this.token}`",
    '    const res = await fetch(this.baseUrl + path, { method, headers: h, body })',
    '    const text = await res.text()',
    '    let data: any = text',
    '    try { data = JSON.parse(text) } catch { /* texto plano */ }',
    '    if (!res.ok) throw Object.assign(new Error(`HTTP ${res.status}`), { status: res.status, data })',
    '    return data as T',
    '  }', '']
  for (const e of list) {
    const args = e.params.map(p => `${p}: string | number`).join(', ')
    const qs = e.query.length ? ' + "?" + [' + e.query.map(p => `\`${encodeURIComponent(p.key)}=\${encodeURIComponent(String(${p.value.match(/\{\{\s*([\w.]+)\s*\}\}/) ? camel(p.value.match(/\{\{\s*([\w.]+)\s*\}\}/)![1].split('.')[0]) : q(p.value)}))}\``).join(', ') + '].join("&")' : ''
    const hdr = e.headers.length ? '{ ' + e.headers.map(h => `${q(h.key)}: ${tpl(h.value)}`).join(', ') + ' }' : (e.r.auth.type === 'api-key' && e.r.auth.apiKeyIn === 'header' && e.r.auth.apiKeyName ? `{ ${q(e.r.auth.apiKeyName)}: this.apiKey }` : '{}')
    L.push(`  /** ${e.method} ${e.r.url} */`, `  ${e.name}(${args}) {`,
      `    return this.request(${q(e.method)}, ${tpl(e.path)}${qs}, ${e.hasBody ? tpl(e.r.body) : 'undefined'}, ${hdr})`, '  }', '')
  }
  L.push('}', '')
  return [{ path: `src/api/${cls}.ts`, content: L.join('\n') }]
}

// ── QML / JS ──
function genQml(c: Coll): GenFile[] {
  const { base, list } = prep(c)
  const tpl = (t: string) => { const p = split(t); return p.some(x => x.v) ? '`' + p.map(x => x.lit !== undefined ? x.lit.replace(/[`\\]|\$\{/g, m => '\\' + m) : '${' + x.v + '}').join('') + '`' : q(t) }
  const cls = pascal(c.name)
  const L = [`// Generado por OweeCode desde la colección "${c.name}".`, `// Uso en QML:  import "api/${cls}Client.js" as Api   ->   Api.${list[0]?.name ?? 'metodo'}(..., function(err, res) { })`,
    '.pragma library', '', `var baseUrl = ${q(base)}`, 'var token = ""', 'var apiKey = ""', '',
    'function request(method, path, body, headers, cb) {', '    var xhr = new XMLHttpRequest()', '    xhr.onreadystatechange = function() {',
    '        if (xhr.readyState !== XMLHttpRequest.DONE) return', '        var data = xhr.responseText', '        try { data = JSON.parse(data) } catch (e) { /* texto */ }',
    '        if (xhr.status >= 200 && xhr.status < 300) cb(null, data, xhr.status)', '        else cb({ status: xhr.status, data: data }, null, xhr.status)', '    }',
    '    xhr.open(method, baseUrl + path)', '    xhr.setRequestHeader("Content-Type", "application/json")', '    if (token) xhr.setRequestHeader("Authorization", "Bearer " + token)',
    '    for (var k in headers) xhr.setRequestHeader(k, headers[k])', '    xhr.send(body)', '}', '']
  for (const e of list) {
    const args = [...e.params, 'cb'].join(', ')
    const qs = e.query.length ? ' + "?" + [' + e.query.map(p => { const m = p.value.match(/\{\{\s*([\w.]+)\s*\}\}/); return `"${encodeURIComponent(p.key)}=" + encodeURIComponent(${m ? camel(m[1].split('.')[0]) : q(p.value)})` }).join(', ') + '].join("&")' : ''
    const hdr = e.headers.length ? '{ ' + e.headers.map(h => `${q(h.key)}: ${tpl(h.value)}`).join(', ') + ' }' : (e.r.auth.type === 'api-key' && e.r.auth.apiKeyIn === 'header' && e.r.auth.apiKeyName ? `{ ${q(e.r.auth.apiKeyName)}: apiKey }` : '{}')
    L.push(`// ${e.method} ${e.r.url}`, `function ${e.name}(${args}) {`, `    request(${q(e.method)}, ${tpl(e.path)}${qs}, ${e.hasBody ? tpl(e.r.body) : 'null'}, ${hdr}, cb)`, '}', '')
  }
  return [{ path: `qml/api/${cls}Client.js`, content: L.join('\n') }]
}

// ── Qt C++ ──
function genQtCpp(c: Coll): GenFile[] {
  const { base, list } = prep(c)
  const cls = pascal(c.name) + 'Client'
  const qs = (t: string) => { const p = split(t); return p.some(x => x.v) ? '(' + p.map(x => x.lit !== undefined ? `QStringLiteral(${q(x.lit)})` : x.v).join(' + ') + ')' : `QStringLiteral(${q(t)})` }
  const L = [`// Generado por OweeCode desde la colección "${c.name}".`, '// CMake: find_package(Qt6 REQUIRED COMPONENTS Core Network)  +  target_link_libraries(app PRIVATE Qt6::Core Qt6::Network)', '#pragma once', '',
    '#include <QJsonDocument>', '#include <QNetworkAccessManager>', '#include <QNetworkReply>', '#include <QNetworkRequest>', '#include <QObject>', '#include <QUrl>', '#include <functional>', '',
    `class ${cls} : public QObject {`, 'public:',
    '    // status HTTP, JSON de la respuesta (si lo es) y texto de error ("" si todo salió bien)',
    '    using Callback = std::function<void(int status, const QJsonDocument &json, const QString &error)>;', '',
    `    explicit ${cls}(QObject *parent = nullptr) : QObject(parent) {}`, '',
    `    QString baseUrl = QStringLiteral(${q(base)});`, '    QString token;     // se envía como "Authorization: Bearer <token>"', '    QString apiKey;', '']
  for (const e of list) {
    const args = [...e.params.map(p => `const QString &${p}`), 'Callback cb'].join(', ')
    const qsPart = e.query.length ? ' + QStringLiteral("?") + ' + e.query.map(p => { const m = p.value.match(/\{\{\s*([\w.]+)\s*\}\}/); return `QStringLiteral(${q(encodeURIComponent(p.key) + '=')}) + QString::fromUtf8(QUrl::toPercentEncoding(${m ? camel(m[1].split('.')[0]) : `QStringLiteral(${q(p.value)})`}))` }).join(' + QStringLiteral("&") + ') : ''
    const hdrs = e.headers.map(h => `{${q(h.key)}, ${qs(h.value)}.toUtf8()}`)
    if (e.r.auth.type === 'api-key' && e.r.auth.apiKeyIn === 'header' && e.r.auth.apiKeyName) hdrs.push(`{${q(e.r.auth.apiKeyName)}, apiKey.toUtf8()}`)
    L.push(`    // ${e.method} ${e.r.url}`, `    void ${e.name}(${args}) {`,
      `        request(${q(e.method)}, ${qs(e.path)}${qsPart}, ${e.hasBody ? `${qs(e.r.body)}.toUtf8()` : 'QByteArray()'}, {${hdrs.join(', ')}}, std::move(cb));`, '    }', '')
  }
  L.push('private:',
    '    void request(const QString &method, const QString &path, const QByteArray &body,',
    '                 const QList<QPair<QByteArray, QByteArray>> &headers, Callback cb) {',
    '        QNetworkRequest req{QUrl(baseUrl + path)};', '        req.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");',
    '        if (!token.isEmpty()) req.setRawHeader("Authorization", "Bearer " + token.toUtf8());',
    '        for (const auto &h : headers) req.setRawHeader(h.first, h.second);',
    '        QNetworkReply *reply = m_net.sendCustomRequest(req, method.toUtf8(), body);',
    '        QObject::connect(reply, &QNetworkReply::finished, this, [reply, cb]() {',
    '            const int status = reply->attribute(QNetworkRequest::HttpStatusCodeAttribute).toInt();',
    '            const QByteArray data = reply->readAll();',
    '            const QString error = reply->error() == QNetworkReply::NoError ? QString() : reply->errorString();',
    '            cb(status, QJsonDocument::fromJson(data), error);', '            reply->deleteLater();', '        });', '    }', '',
    '    QNetworkAccessManager m_net;', '};', '')
  return [{ path: `src/api/${cls}.h`, content: L.join('\n') }]
}

export function generateClient(c: Coll, lang: ClientLang): GenFile[] {
  if (!c.requests.length) return []
  return lang === 'ts' ? genTs(c) : lang === 'qml' ? genQml(c) : genQtCpp(c)
}
