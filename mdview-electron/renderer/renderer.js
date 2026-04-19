import init, { parse_markdown_to_json, schema_version } from '../wasm/mdview_core.js';

let hljs = null;

let tocOpen = true;            // 初期状態: 表示
let tocSelectedIndex = 0;      // TOC 開時の選択項目 index（Enter ジャンプ対象）
let currentToc = [];           // 最新の doc.toc を保持（keydown ハンドラから参照）
let focusedPane = 'content';   // 'toc' | 'content' — キー入力の移動対象ペイン
let currentFilePath = null;    // ステータスバー表示用（絶対パス）
let expectedSchemaVersion = null; // WASM 初期化後に schema_version() で設定

// ── テーマ ────────────────────────────────────────────────────────────────

/**
 * テーマ ID → CSS 変数・hljs CSS・背景色のマッピング。
 * css 変数名は style.css の `:root` と完全一致させること。
 * main.js の THEME_BACKGROUNDS と背景色を同期させること。
 */
const THEME_REGISTRY = {
  'vscode-dark': {
    cssVars: {
      '--bg': '#1e1e1e',
      '--bg-alt': '#252526',
      '--bg-surface': '#3c3c3c',
      '--border': '#474747',
      '--text': '#d4d4d4',
      '--text-muted': '#858585',
      '--blue': '#569cd6',
      '--cyan': '#4ec9b0',
      '--green': '#6a9955',
      '--mauve': '#ce9178',
      '--red': '#f44747',
    },
    hljsCss: 'vendor/themes/hljs/vs2015.css',
    background: '#1e1e1e',
  },
  'vscode-light': {
    cssVars: {
      '--bg': '#ffffff',
      '--bg-alt': '#f3f3f3',
      '--bg-surface': '#e8e8e8',
      '--border': '#c8c8c8',
      '--text': '#1e1e1e',
      '--text-muted': '#717171',
      '--blue': '#0070c0',
      '--cyan': '#008080',
      '--green': '#267f00',
      '--mauve': '#a31515',
      '--red': '#cd3131',
    },
    hljsCss: 'vendor/themes/hljs/vs.css',
    background: '#ffffff',
  },
  'github-dark': {
    cssVars: {
      '--bg': '#0d1117',
      '--bg-alt': '#161b22',
      '--bg-surface': '#21262d',
      '--border': '#30363d',
      '--text': '#e6edf3',
      '--text-muted': '#8b949e',
      '--blue': '#58a6ff',
      '--cyan': '#39c5cf',
      '--green': '#3fb950',
      '--mauve': '#d2a8ff',
      '--red': '#ff7b72',
    },
    hljsCss: 'vendor/themes/hljs/github-dark.css',
    background: '#0d1117',
  },
  'github-light': {
    cssVars: {
      '--bg': '#ffffff',
      '--bg-alt': '#f6f8fa',
      '--bg-surface': '#eaeef2',
      '--border': '#d0d7de',
      '--text': '#24292f',
      '--text-muted': '#57606a',
      '--blue': '#005cc5',
      '--cyan': '#0598bc',
      '--green': '#28a745',
      '--mauve': '#6f42c1',
      '--red': '#d73a49',
    },
    hljsCss: 'vendor/themes/hljs/github.css',
    background: '#ffffff',
  },
};

const DEFAULT_THEME_ID = 'vscode-dark';

/**
 * 指定テーマを適用する。
 * - CSS 変数を document.documentElement.style.setProperty で上書き
 * - hljs テーマ CSS リンクの href を差し替え
 */
function applyTheme(id) {
  const theme = THEME_REGISTRY[id] || THEME_REGISTRY[DEFAULT_THEME_ID];
  if (!THEME_REGISTRY[id]) {
    console.warn(`mdview: unknown theme id "${id}", falling back to default.`);
  }

  // CSS 変数上書き
  const root = document.documentElement;
  for (const [varName, value] of Object.entries(theme.cssVars)) {
    root.style.setProperty(varName, value);
  }

  // hljs テーマ CSS 差し替え
  const hljsLink = document.getElementById('hljs-theme');
  if (hljsLink) {
    hljsLink.href = theme.hljsCss;
  }
}

// ── highlight.js ─────────────────────────────────────────────────────────

async function loadHighlightJs() {
  try {
    // @highlightjs/cdn-assets の自己完結 ESM バンドルを動的インポート
    // （highlight.js 本体の es/index.js は CJS lib/ に依存し Chromium ESM で動かないため）
    const hljsModule = await import('./vendor/highlight.min.js');
    hljs = hljsModule.default;
  } catch (e) {
    console.warn('highlight.js load failed, code highlighting disabled:', e);
    hljs = null;
  }
}

// ── SpanKind / Block ヘルパー ─────────────────────────────────────────────

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

// ── TOC ──────────────────────────────────────────────────────────────────

// TOC を構築
function buildToc(toc) {
  currentToc = toc || [];
  const nav = document.getElementById('toc-nav');
  if (currentToc.length === 0) {
    tocSelectedIndex = 0;
    nav.innerHTML = '<p class="toc-empty">見出しなし</p>';
    return;
  }

  // tocSelectedIndex が TOC 項目数を超えていたら 0 に補正（TUI app.rs L155-158 相当）
  if (tocSelectedIndex >= currentToc.length) {
    tocSelectedIndex = 0;
  }

  const ul = document.createElement('ul');
  currentToc.forEach((entry, idx) => {
    const li = document.createElement('li');
    li.style.paddingLeft = `${(entry.level - 1) * 12}px`;
    li.dataset.tocIndex = idx;

    const a = document.createElement('a');
    a.textContent = entry.title;
    a.href = '#';
    a.addEventListener('click', (e) => {
      e.preventDefault();
      scrollToHeading(idx);
    });

    li.appendChild(a);
    ul.appendChild(li);
  });

  nav.innerHTML = '';
  nav.appendChild(ul);

  // 初回描画・再描画時にもカーソルハイライトを反映
  updateTocSelection();
}

// TOC カーソルのハイライト状態を更新（TUI toc.rs の ListState::select 相当）
function updateTocSelection() {
  if (currentToc.length === 0) return;
  const nav = document.getElementById('toc-nav');
  nav.querySelectorAll('li').forEach((li) => li.classList.remove('toc-item-active'));
  const target = nav.querySelector(`[data-toc-index="${tocSelectedIndex}"]`);
  if (target) {
    target.classList.add('toc-item-active');
    target.scrollIntoView({ block: 'nearest' });
  }
}

// TOC の表示/非表示を tocOpen 状態に合わせる
function applyTocVisibility() {
  document.getElementById('toc').classList.toggle('toc-hidden', !tocOpen);
}

// フォーカスペインの視覚ハイライトを更新
function applyFocus() {
  document.getElementById('toc').classList.toggle('pane-focused', focusedPane === 'toc' && tocOpen);
  document.getElementById('content').classList.toggle('pane-focused', focusedPane === 'content');
}

// ── ステータスバー ────────────────────────────────────────────────────────

// ステータスバーをスクロール位置・TOC 状態に合わせて更新
function updateStatusBar() {
  const contentEl = document.getElementById('content');
  const sbFile = document.getElementById('sb-file');
  const sbPos = document.getElementById('sb-position');
  const sbTocHint = document.getElementById('sb-toc-hint');
  const sb = document.getElementById('statusbar');

  sb.classList.remove('sb-error');
  sbFile.textContent = currentFilePath ? currentFilePath.split('/').pop() : '(no file)';

  const scrollTop = contentEl.scrollTop;
  const scrollHeight = contentEl.scrollHeight;
  const clientHeight = contentEl.clientHeight;
  const maxScroll = Math.max(1, scrollHeight - clientHeight);
  const pct = Math.min(100, Math.round((scrollTop / maxScroll) * 100));
  sbPos.textContent = `${pct}%`;

  sbTocHint.textContent = tocOpen ? '[t]close' : '[t]TOC';
}

// ステータスバーをエラー状態に更新
function setStatusBarError(msg) {
  const sb = document.getElementById('statusbar');
  const sbFile = document.getElementById('sb-file');
  const sbPos = document.getElementById('sb-position');
  const sbTocHint = document.getElementById('sb-toc-hint');
  sb.classList.add('sb-error');
  sbFile.textContent = `[ERROR] ${msg}`;
  sbPos.textContent = '';
  sbTocHint.textContent = '';
}

// ファイルパスを更新してステータスバー・ツールバーを同期
function setCurrentFile(filePath) {
  currentFilePath = filePath;
  document.getElementById('file-name').textContent =
    filePath ? filePath.split('/').pop() : 'ファイルが開かれていません';
  updateStatusBar();
}

// 指定 index の見出しへスクロール
function scrollToHeading(idx) {
  const el = document.getElementById(`heading-${idx}`);
  if (el) el.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

// ── キーボードハンドラ ────────────────────────────────────────────────────

// キーボード操作ハンドラ
function handleKeyDown(e) {
  // <input> / <textarea> / contenteditable フォーカス中は無視
  const tag = e.target.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || e.target.isContentEditable) return;

  // Ctrl/Meta/Alt 修飾キー付きのショートカット（メニュー accelerator 経路）は無視
  // Shift のみは許可（'G' = Shift+g を拾うため）
  if (e.ctrlKey || e.metaKey || e.altKey) return;

  const contentEl = document.getElementById('content');
  switch (e.key) {
    case 'j':
    case 'ArrowDown':
      if (focusedPane === 'toc' && tocOpen) {
        if (currentToc.length > 0) {
          tocSelectedIndex = Math.min(tocSelectedIndex + 1, currentToc.length - 1);
          updateTocSelection();
        }
      } else {
        contentEl.scrollBy(0, 40);
      }
      e.preventDefault();
      break;
    case 'k':
    case 'ArrowUp':
      if (focusedPane === 'toc' && tocOpen) {
        if (currentToc.length > 0) {
          tocSelectedIndex = Math.max(tocSelectedIndex - 1, 0);
          updateTocSelection();
        }
      } else {
        contentEl.scrollBy(0, -40);
      }
      e.preventDefault();
      break;
    case 'h':
    case 'ArrowLeft':
      if (tocOpen) {
        focusedPane = 'toc';
        applyFocus();
      }
      e.preventDefault();
      break;
    case 'l':
    case 'ArrowRight':
      focusedPane = 'content';
      applyFocus();
      e.preventDefault();
      break;
    case 'PageDown':
      contentEl.scrollBy(0, contentEl.clientHeight * 0.9);
      e.preventDefault();
      break;
    case 'PageUp':
      contentEl.scrollBy(0, -contentEl.clientHeight * 0.9);
      e.preventDefault();
      break;
    case 'g':
      contentEl.scrollTo(0, 0);
      e.preventDefault();
      break;
    case 'G':
      contentEl.scrollTo(0, contentEl.scrollHeight);
      e.preventDefault();
      break;
    case 't':
      tocOpen = !tocOpen;
      if (tocOpen && currentToc.length > 0 && tocSelectedIndex >= currentToc.length) {
        tocSelectedIndex = 0;
      }
      // 閉じるとき TOC フォーカスなら本文に戻す
      if (!tocOpen && focusedPane === 'toc') {
        focusedPane = 'content';
      }
      applyTocVisibility();
      applyFocus();
      updateTocSelection();
      updateStatusBar();
      e.preventDefault();
      break;
    case 'Enter':
      if (focusedPane === 'toc' && tocOpen && currentToc.length > 0) {
        scrollToHeading(tocSelectedIndex);
        focusedPane = 'content';
        applyFocus();
        e.preventDefault();
      }
      break;
    case 'Escape':
      if (tocOpen) {
        tocOpen = false;
        focusedPane = 'content';
        applyTocVisibility();
        applyFocus();
        updateStatusBar();
        e.preventDefault();
      }
      break;
    case 'r':
      window.mdview.reloadCurrent();
      e.preventDefault();
      break;
    default:
      break;
  }
}

// ── Markdown レンダリング ──────────────────────────────────────────────────

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
  if (doc.schema_version !== expectedSchemaVersion) {
    document.getElementById('markdown-body').textContent =
      'Unsupported schema version: got ' + doc.schema_version +
      ', expected ' + expectedSchemaVersion;
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

  updateStatusBar();
}

// ── メイン ────────────────────────────────────────────────────────────────

// メイン処理
async function main() {
  // WASM 初期化
  await init();

  // WASM から schema_version を取得してキャッシュ（ハードコード排除）
  expectedSchemaVersion = schema_version();

  // highlight.js 読み込み
  await loadHighlightJs();

  // config を読み込んでテーマを適用
  try {
    const config = await window.mdview.loadConfig();
    const themeId = (config && config.theme) || DEFAULT_THEME_ID;
    applyTheme(themeId);
  } catch (e) {
    console.warn('mdview: failed to load config, using default theme:', e);
    applyTheme(DEFAULT_THEME_ID);
  }

  // メニュー「テーマ」変更通知を受信してテーマを切り替え
  window.mdview.onThemeChanged(({ id }) => {
    applyTheme(id);
  });

  // ファイルオープンボタン
  document.getElementById('open-btn').addEventListener('click', async () => {
    const result = await window.mdview.openFile();
    if (result) {
      setCurrentFile(result.path);
      await renderMarkdown(result.text);
    }
  });

  // Main プロセスからのファイル（CLI引数 or メニュー）
  window.mdview.onFileOpened(async (data) => {
    setCurrentFile(data.path);
    await renderMarkdown(data.text);
  });

  // ファイル変更検知（ホットリロード）
  window.mdview.onFileChanged(async (data) => {
    const contentEl = document.getElementById('content');
    const scrollY = contentEl.scrollTop;
    setCurrentFile(data.path);
    await renderMarkdown(data.text);
    contentEl.scrollTop = scrollY;
  });

  // ファイル削除検知
  window.mdview.onFileMissing((data) => {
    const body = document.getElementById('markdown-body');
    body.innerHTML =
      '<p class="placeholder">ファイルが見つかりません: ' +
      esc(data.path) +
      '</p>';
    setStatusBarError('ファイルが見つかりません: ' + data.path.split('/').pop());
  });

  // ファイル読み込みエラー
  window.mdview.onFileError((data) => {
    const body = document.getElementById('markdown-body');
    body.innerHTML =
      '<p class="placeholder">ファイル読み込みエラー: ' +
      esc(data.message) +
      '</p>';
    setStatusBarError('読み込みエラー: ' + data.message);
  });

  // キーボードハンドラを登録
  document.addEventListener('keydown', handleKeyDown);
  // 初期フォーカスハイライトを適用
  applyFocus();

  // スクロール位置変化をステータスバーに反映
  document.getElementById('content').addEventListener('scroll', () => {
    updateStatusBar();
  });

  // 初期ステータスバー表示
  updateStatusBar();

  // すべてのリスナー登録と WASM 初期化が終わったことを main に通知
  // （これを受けて main が CLI 引数のファイルを file:opened で送る）
  window.mdview.notifyReady();
}

main().catch(console.error);
