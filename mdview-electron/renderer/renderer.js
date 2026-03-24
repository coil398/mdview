import init, { parse_markdown_to_json } from '../wasm/mdview_core.js';

let hljs = null;

async function loadHighlightJs() {
  try {
    // highlight.js の ESM ビルドを動的インポート
    const hljsModule = await import('../node_modules/highlight.js/es/index.js');
    hljs = hljsModule.default;
  } catch (e) {
    console.warn('highlight.js load failed, code highlighting disabled:', e);
    hljs = null;
  }
}

// SpanKind の判定ヘルパー
function kindType(kind) {
  if (typeof kind === 'string') return kind;
  return Object.keys(kind)[0];
}

function kindData(kind) {
  if (typeof kind === 'string') return null;
  return Object.values(kind)[0];
}

// テキストを HTML エスケープ
function esc(text) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// span を HTML に変換
function spanToHtml(span) {
  const text = esc(span.text);
  const type = kindType(span.kind);
  const data = kindData(span.kind);

  switch (type) {
    case 'Normal': return `<span>${text}</span>`;
    case 'Bold': return `<strong>${text}</strong>`;
    case 'Italic': return `<em>${text}</em>`;
    case 'BoldItalic': return `<strong><em>${text}</em></strong>`;
    case 'CodeInline': return `<code class="inline">${text}</code>`;
    case 'ListMarker': return `<span class="list-marker">${text}</span>`;
    case 'BlockQuote': return `<span class="blockquote-text">${text}</span>`;
    case 'Rule': return '';
    case 'Heading': return text;
    case 'CodeBlock': return text;
    case 'Link': {
      const rawUrl = data.url;
      const safeUrl = /^https?:\/\//i.test(rawUrl) || /^mailto:/i.test(rawUrl) ? rawUrl : '#';
      const url = esc(safeUrl);
      return `<a href="${url}" target="_blank" rel="noopener noreferrer">${text}</a>`;
    }
    default: return text;
  }
}

// 行の主要な SpanKind を判定
function getLineKind(line) {
  if (line.length === 0) return 'Empty';
  const first = line[0];
  const type = kindType(first.kind);
  if (type === 'Heading') return 'Heading';
  if (type === 'Rule') return 'Rule';
  if (type === 'CodeBlock') return 'CodeBlock';
  if (type === 'BlockQuote') return 'BlockQuote';
  if (type === 'ListMarker') return 'ListMarker';
  return 'Normal';
}

// Document を HTML 文字列に変換
function documentToHtml(doc) {
  const lines = doc.lines;
  let html = '';
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const lineKind = getLineKind(line);

    if (lineKind === 'Empty') {
      html += '<div class="empty-line"></div>';
      i++;
      continue;
    }

    if (lineKind === 'Rule') {
      html += '<hr />';
      i++;
      continue;
    }

    if (lineKind === 'Heading') {
      const level = kindData(line[0].kind);
      const id = `heading-${i}`;
      const content = line.map(spanToHtml).join('');
      html += `<h${level} id="${id}">${content}</h${level}>`;
      i++;
      continue;
    }

    if (lineKind === 'CodeBlock') {
      // 連続する CodeBlock 行をグループ化
      const lang = kindData(line[0].kind)?.lang || '';
      const codeLines = [];
      while (i < lines.length && getLineKind(lines[i]) === 'CodeBlock') {
        codeLines.push(lines[i].map(s => s.text).join(''));
        i++;
      }
      const codeText = esc(codeLines.join('\n'));
      const langClass = lang ? ` class="language-${esc(lang)}"` : '';
      html += `<pre><code${langClass}>${codeText}</code></pre>`;
      continue;
    }

    if (lineKind === 'BlockQuote') {
      // 連続する BlockQuote 行をグループ化
      const quoteLines = [];
      while (i < lines.length && getLineKind(lines[i]) === 'BlockQuote') {
        quoteLines.push(lines[i].map(spanToHtml).join(''));
        i++;
      }
      html += `<blockquote>${quoteLines.join('<br />')}</blockquote>`;
      continue;
    }

    if (lineKind === 'ListMarker') {
      // 連続するリスト行をグループ化
      html += '<ul>';
      while (i < lines.length && getLineKind(lines[i]) === 'ListMarker') {
        const content = lines[i].map(spanToHtml).join('');
        html += `<li>${content}</li>`;
        i++;
      }
      html += '</ul>';
      continue;
    }

    // Normal: 段落
    html += `<p>${line.map(spanToHtml).join('')}</p>`;
    i++;
  }

  return html;
}

// TOC を構築
function buildToc(toc, headingIds) {
  const nav = document.getElementById('toc-nav');
  if (!toc || toc.length === 0) {
    nav.innerHTML = '<p class="toc-empty">見出しなし</p>';
    return;
  }

  const ul = document.createElement('ul');
  toc.forEach((entry, idx) => {
    const li = document.createElement('li');
    li.style.paddingLeft = `${(entry.level - 1) * 12}px`;

    const a = document.createElement('a');
    a.textContent = entry.title;
    a.href = '#';
    a.addEventListener('click', (e) => {
      e.preventDefault();
      const id = headingIds[idx];
      const el = document.getElementById(id);
      if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    });

    li.appendChild(a);
    ul.appendChild(li);
  });

  nav.innerHTML = '';
  nav.appendChild(ul);
}

// Markdown をレンダリング
async function renderMarkdown(text) {
  const jsonStr = parse_markdown_to_json(text);
  let doc;
  try {
    doc = JSON.parse(jsonStr);
  } catch (e) {
    document.getElementById('markdown-body').textContent = 'Parse error: ' + e.message;
    return;
  }
  if (doc.error) {
    document.getElementById('markdown-body').textContent = 'Error: ' + doc.error;
    return;
  }

  const body = document.getElementById('markdown-body');
  body.innerHTML = documentToHtml(doc);

  // TOC の headingId マップを構築（heading-{line_index}）
  const headingIds = doc.toc.map(entry => `heading-${entry.line_index}`);
  buildToc(doc.toc, headingIds);

  // highlight.js でコードブロックをハイライト
  if (hljs) {
    body.querySelectorAll('pre code').forEach(el => {
      hljs.highlightElement(el);
    });
  }
}

// メイン処理
async function main() {
  // WASM 初期化
  await init();

  // highlight.js 読み込み
  await loadHighlightJs();

  // ファイルオープンボタン
  document.getElementById('open-btn').addEventListener('click', async () => {
    const result = await window.mdview.openFile();
    if (result) {
      document.getElementById('file-name').textContent = result.path.split('/').pop();
      await renderMarkdown(result.text);
    }
  });

  // Main プロセスからのファイル（CLI引数 or メニュー）
  window.mdview.onFileOpened(async (data) => {
    document.getElementById('file-name').textContent = data.path.split('/').pop();
    await renderMarkdown(data.text);
  });
}

main().catch(console.error);
