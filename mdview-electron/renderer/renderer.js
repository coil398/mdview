import init, { parse_markdown_to_json } from '../wasm/mdview_core.js';

const EXPECTED_SCHEMA_VERSION = 2;

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

// SpanKind の判定ヘルパー（"Normal"/"Bold" 等の文字列、または {Link:{url:...}} のオブジェクト）
function kindType(kind) {
  if (typeof kind === 'string') return kind;
  return Object.keys(kind)[0];
}

function kindData(kind) {
  if (typeof kind === 'string') return null;
  return Object.values(kind)[0];
}

// Block の判定ヘルパー（"Rule" の文字列、または {Heading:{...}} のオブジェクト）
function blockType(block) {
  if (typeof block === 'string') return block;
  return Object.keys(block)[0];
}

function blockData(block) {
  if (typeof block === 'string') return null;
  return Object.values(block)[0];
}

// テキストを HTML エスケープ
function esc(text) {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Span を HTML に変換
function spanToHtml(span) {
  const text = esc(span.text);
  const type = kindType(span.kind);
  const data = kindData(span.kind);

  switch (type) {
    case 'Normal':
      return `<span>${text}</span>`;
    case 'Bold':
      return `<strong>${text}</strong>`;
    case 'Italic':
      return `<em>${text}</em>`;
    case 'BoldItalic':
      return `<strong><em>${text}</em></strong>`;
    case 'CodeInline':
      return `<code class="inline">${text}</code>`;
    case 'Link': {
      const rawUrl = data.url;
      const safeUrl = /^https?:\/\//i.test(rawUrl) || /^mailto:/i.test(rawUrl) ? rawUrl : '#';
      const url = esc(safeUrl);
      return `<a href="${url}" target="_blank" rel="noopener noreferrer">${text}</a>`;
    }
    default:
      return text;
  }
}

// Span 列 → HTML
function spansToHtml(spans) {
  return spans.map(spanToHtml).join('');
}

// Alignment → CSS の text-align 値
function alignToCss(align) {
  switch (align) {
    case 'Left':
      return 'left';
    case 'Center':
      return 'center';
    case 'Right':
      return 'right';
    case 'None':
    default:
      return null;
  }
}

// Block を HTML に変換。`headingIndex` は出現順の見出し index（id 用）。
function blockToHtml(block, headingIndexBox) {
  const t = blockType(block);
  const d = blockData(block);

  switch (t) {
    case 'Paragraph': {
      // lines: Vec<Vec<Span>>。HardBreak 区切りは <br /> で表現
      const lineHtmls = d.lines.map(spansToHtml);
      return `<p>${lineHtmls.join('<br />')}</p>`;
    }
    case 'Heading': {
      const level = d.level;
      const id = `heading-${headingIndexBox.value}`;
      headingIndexBox.value += 1;
      const inner = spansToHtml(d.spans);
      return `<h${level} id="${id}">${inner}</h${level}>`;
    }
    case 'List': {
      const tag = d.ordered ? 'ol' : 'ul';
      const startAttr = d.ordered && d.start !== null && d.start !== 1 ? ` start="${d.start}"` : '';
      const items = d.items
        .map((item) => {
          const inner = item.blocks.map((b) => blockToHtml(b, headingIndexBox)).join('');
          return `<li>${inner}</li>`;
        })
        .join('');
      return `<${tag}${startAttr}>${items}</${tag}>`;
    }
    case 'BlockQuote': {
      const inner = d.blocks.map((b) => blockToHtml(b, headingIndexBox)).join('');
      return `<blockquote>${inner}</blockquote>`;
    }
    case 'CodeBlock': {
      const lang = d.lang;
      const code = esc(d.code);
      const langClass = lang ? ` class="language-${esc(lang)}"` : '';
      return `<pre><code${langClass}>${code}</code></pre>`;
    }
    case 'Table': {
      const aligns = d.align || [];
      const cellAlign = (i) => {
        const css = alignToCss(aligns[i]);
        return css ? ` style="text-align:${css}"` : '';
      };
      const headerHtml = d.header
        .map((cell, i) => `<th${cellAlign(i)}>${spansToHtml(cell.spans)}</th>`)
        .join('');
      const rowsHtml = d.rows
        .map((row) => {
          const cellsHtml = row
            .map((cell, i) => `<td${cellAlign(i)}>${spansToHtml(cell.spans)}</td>`)
            .join('');
          return `<tr>${cellsHtml}</tr>`;
        })
        .join('');
      return `<table><thead><tr>${headerHtml}</tr></thead><tbody>${rowsHtml}</tbody></table>`;
    }
    case 'Rule':
      return '<hr />';
    default:
      console.warn('unknown block type:', t);
      return '';
  }
}

// Document を HTML 文字列に変換
function documentToHtml(doc) {
  const headingIndexBox = { value: 0 };
  return doc.blocks.map((b) => blockToHtml(b, headingIndexBox)).join('');
}

// TOC を構築
function buildToc(toc) {
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
      // toc[idx] は出現順 idx 番目の見出しに対応する
      const id = `heading-${idx}`;
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
  let result;
  try {
    result = JSON.parse(jsonStr);
  } catch (e) {
    document.getElementById('markdown-body').textContent = 'Parse error: ' + e.message;
    return;
  }
  if (result.error) {
    const { kind, message } = result.error;
    document.getElementById('markdown-body').textContent =
      'Error (' + kind + '): ' + message;
    return;
  }
  const doc = result.ok;
  if (!doc || typeof doc.schema_version !== 'number') {
    document.getElementById('markdown-body').textContent =
      'Unsupported response: missing schema_version';
    return;
  }
  if (doc.schema_version !== EXPECTED_SCHEMA_VERSION) {
    document.getElementById('markdown-body').textContent =
      'Unsupported schema version: got ' + doc.schema_version +
      ', expected ' + EXPECTED_SCHEMA_VERSION;
    return;
  }

  const body = document.getElementById('markdown-body');
  body.innerHTML = documentToHtml(doc);

  buildToc(doc.toc);

  // highlight.js でコードブロックをハイライト
  if (hljs) {
    body.querySelectorAll('pre code').forEach((el) => {
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
