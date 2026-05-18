import init, { parse_markdown_to_json, schema_version } from '../wasm/mdview_core.js';

let hljs = null;

let tocOpen = true;            // 初期状態: 表示
let tocSelectedIndex = 0;      // TOC 開時の選択項目 index（Enter ジャンプ対象）
let currentToc = [];           // 最新の doc.toc を保持（keydown ハンドラから参照）
let focusedPane = 'content';   // 'toc' | 'content' | 'notes' — キー入力の移動対象ペイン
let currentFilePath = null;    // ステータスバー表示用（絶対パス）
let expectedSchemaVersion = null; // WASM 初期化後に schema_version() で設定

// ── パネル幅リサイズ ──────────────────────────────────────────────────────
// 制約は main.js の対応定数と同期させること。
const TOC_WIDTH_DEFAULT = 240;
const TOC_WIDTH_MIN = 120;
const TOC_WIDTH_MAX = 600;
const NOTES_WIDTH_DEFAULT = 280;
const NOTES_WIDTH_MIN = 160;
const NOTES_WIDTH_MAX = 800;
const RESIZE_STEP = 20;             // H/L キー 1 回あたりの幅変動 px
const LAYOUT_SAVE_DEBOUNCE_MS = 300;

let tocWidth = TOC_WIDTH_DEFAULT;
let notesWidth = NOTES_WIDTH_DEFAULT;
let layoutSaveTimer = null;

// ── 検索機能の状態 ────────────────────────────────────────────────────────

let searchMatches = [];              // span.search-match 要素の配列
let searchCursor = 0;               // 現在のマッチ index
let searchQuery = '';               // 現在の検索クエリ

// ── メモ機能の状態 ────────────────────────────────────────────────────────

let notesOpen = true;                // 右パネル開閉（config.notes.panel_open と同期）
let notesEntries = [];               // 現ファイル分の NoteEntry 配列（main から取得）
let currentHeadingKey = null;        // { heading_text, heading_level, occurrence_index } or null
let headingKeyMap = new WeakMap();   // HTMLElement → AnchorKey（DOM 属性に漏らさない）
let orderedHeadings = [];            // 出現順の HTMLElement 配列（scroll 中の topmost 判定用）
let notesSaveTimer = null;           // textarea input の debounce（500ms）
let pendingScrollFrame = false;      // requestAnimationFrame throttle フラグ

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
      // 検索ハイライト（WCAG AA 確認済み: 5.73:1 / 5.19:1）
      '--search-bg': '#264f78',
      '--search-fg': '#d4d4d4',
      '--search-current-bg': '#b58900',
      '--search-current-fg': '#1e1e1e',
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
      // 検索ハイライト（WCAG AA 確認済み: 10.99:1 / 5.07:1）
      // search_current_bg: #d7720e 3.33 FAIL → #bf4800 5.07 PASS に調整
      '--search-bg': '#add6ff',
      '--search-fg': '#1e1e1e',
      '--search-current-bg': '#bf4800',
      '--search-current-fg': '#ffffff',
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
      // 検索ハイライト（WCAG AA 確認済み: 7.49:1 / 5.59:1）
      '--search-bg': '#1c3a5e',
      '--search-fg': '#c9d1d9',
      '--search-current-bg': '#bb8009',
      '--search-current-fg': '#0d1117',
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
      // 検索ハイライト（WCAG AA 確認済み: 12.36:1 / 4.87:1）
      '--search-bg': '#faeacd',
      '--search-fg': '#24292f',
      '--search-current-bg': '#9a6700',
      '--search-current-fg': '#ffffff',
    },
    hljsCss: 'vendor/themes/hljs/github.css',
    background: '#ffffff',
  },
  'solarized-dark': {
    cssVars: {
      '--bg': '#002b36',
      '--bg-alt': '#073642',
      '--bg-surface': '#094654',
      '--border': '#586e75',
      '--text': '#839496',
      '--text-muted': '#657b83',
      '--blue': '#268bd2',
      '--cyan': '#2aa198',
      '--green': '#859900',
      '--mauve': '#6c71c4',
      '--red': '#dc322f',
      // 検索ハイライト（WCAG AA 確認済み: 4.86:1 / 4.68:1）
      '--search-bg': '#073642',
      '--search-fg': '#93a1a1',
      '--search-current-bg': '#b58900',
      '--search-current-fg': '#002b36',
    },
    hljsCss: 'vendor/themes/hljs/solarized-dark.css',
    background: '#002b36',
  },
  'solarized-light': {
    cssVars: {
      '--bg': '#fdf6e3',
      '--bg-alt': '#eee8d5',
      '--bg-surface': '#e5dfc5',
      '--border': '#93a1a1',
      '--text': '#657b83',
      '--text-muted': '#839496',
      '--blue': '#268bd2',
      '--cyan': '#2aa198',
      '--green': '#859900',
      '--mauve': '#6c71c4',
      '--red': '#dc322f',
      // 検索ハイライト（WCAG AA 確認済み: 6.47:1 / 6.04:1）
      // search_fg: #586e75 3.91 FAIL → #3a5560 6.47 PASS に調整
      // search_current_bg: #9a7000 4.15 FAIL → #7a5800 6.04 PASS に調整
      '--search-bg': '#eee8d5',
      '--search-fg': '#3a5560',
      '--search-current-bg': '#7a5800',
      '--search-current-fg': '#fdf6e3',
    },
    hljsCss: 'vendor/themes/hljs/solarized-light.css',
    background: '#fdf6e3',
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
  const effectiveId = THEME_REGISTRY[id] ? id : DEFAULT_THEME_ID;
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

  // mermaid テーマ更新 + 再レンダリング
  const mermaidTheme = MERMAID_THEME_MAP[effectiveId] || 'default';
  if (mermaidTheme !== currentMermaidTheme) {
    currentMermaidTheme = mermaidTheme;
    if (mermaid) {
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: 'strict',
        theme: currentMermaidTheme,
      });
      // fire-and-forget: テーマ切替時の失敗は warn のみ
      reRenderAllMermaid().catch((e) => console.warn('mermaid re-render failed:', e));
    }
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

// ── mermaid ──────────────────────────────────────────────────────────────

let mermaid = null;
// diagram source 保持用。mermaid container 要素 → 元ソース文字列。
// WeakMap を使う理由: renderDocument() で innerHTML 全置換されるため古い container は GC される。
// notes 機能の headingKeyMap と同じ理由で DOM 属性に漏らさない。
const mermaidSources = new WeakMap();
// テーマ ID → mermaid theme 名のマッピング
const MERMAID_THEME_MAP = {
  'vscode-dark': 'dark',
  'vscode-light': 'default',
  'github-dark': 'dark',
  'github-light': 'base',
};
let currentMermaidTheme = 'default'; // applyTheme から更新する

async function loadMermaid() {
  try {
    const mod = await import('./vendor/mermaid.esm.min.mjs');
    mermaid = mod.default;
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: currentMermaidTheme,
    });
  } catch (e) {
    console.warn('mermaid load failed, diagrams disabled:', e);
    mermaid = null;
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

/**
 * Span 配列から heading アンカーキー用のプレーンテキストを抽出する。
 * 書式指定（Bold / Italic 等）と Link の URL は無視し、span.text のみ連結する。
 * HTML エスケープは不要（DOM 属性に出さず Map のキー内部で保持するのみ）。
 *
 * NOTE: 既存の spanToHtml は HTML 文字列を返すため流用不可。
 * 既存に同等プレーンテキスト抽出関数はないので新設する（spans.map(s => s.text).join('')）。
 */
function spansToPlainText(spans) {
  return spans.map((s) => s.text).join('');
}

/**
 * AnchorKey 同士の等価判定。
 */
function anchorKeyEquals(a, b) {
  if (a === null || b === null) return a === b;
  return a.heading_text === b.heading_text
    && a.heading_level === b.heading_level
    && a.occurrence_index === b.occurrence_index;
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
      const code = d.code;
      if (lang && lang.toLowerCase() === 'mermaid') {
        // プレースホルダのみを出力。ソースは後処理で WeakMap に保持する。
        // data-mermaid-source は esc 必須（改行・"・<> を含む mermaid syntax に対応）
        return `<div class="mermaid-container" data-mermaid-source="${esc(code)}"></div>`;
      }
      const codeEsc = esc(code);
      const langClass = lang ? ` class="language-${esc(lang)}"` : '';
      return `<pre><code${langClass}>${codeEsc}</code></pre>`;
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

/**
 * doc.blocks の走査中に、heading block のみを出現順に取り出し
 * `[{ heading_text, heading_level, occurrence_index }]` の配列を返す。
 * リスト内 / blockquote 内にネストした heading も拾う（pulldown-cmark は通常ここに入れないが念のため再帰する）。
 * occurrence_index は同 (level, text) 組合せ内での 0-origin 連番。
 */
function collectHeadingMeta(blocks) {
  const result = [];
  const occCounter = new Map(); // key: `${level}\x00${text}` → 次に割り当てる index

  function visit(block) {
    const t = blockType(block);
    const d = blockData(block);
    if (t === 'Heading') {
      const text = spansToPlainText(d.spans);
      const level = d.level;
      const mapKey = `${level}\x00${text}`;
      const occ = occCounter.get(mapKey) || 0;
      occCounter.set(mapKey, occ + 1);
      result.push({ heading_text: text, heading_level: level, occurrence_index: occ });
    } else if (t === 'List') {
      d.items.forEach((item) => item.blocks.forEach(visit));
    } else if (t === 'BlockQuote') {
      d.blocks.forEach(visit);
    }
    // Paragraph / CodeBlock / Table / Rule は heading を含まない
  }

  blocks.forEach(visit);
  return result;
}

/**
 * Document をレンダリングして #markdown-body に書き込み、
 * heading DOM 要素と AnchorKey の対応を構築する。
 */
function renderDocument(doc) {
  const body = document.getElementById('markdown-body');
  const headingIndexBox = { value: 0 };
  body.innerHTML = doc.blocks.map((b) => blockToHtml(b, headingIndexBox)).join('');

  // 新しい WeakMap / 配列を作成（前回の参照を破棄）
  headingKeyMap = new WeakMap();
  orderedHeadings = [];

  // heading の出現順メタデータを事前計算
  const meta = collectHeadingMeta(doc.blocks);

  // DOM 上の heading を出現順に拾い、meta と zip で対応付ける
  const elements = body.querySelectorAll('h1, h2, h3, h4, h5, h6');
  elements.forEach((el, i) => {
    if (i >= meta.length) return;  // 理論上起きないが防御
    const key = meta[i];
    headingKeyMap.set(el, key);
    orderedHeadings.push(el);
  });
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
  applyResizeHandleVisibility();
}

// フォーカスペインの視覚ハイライトを更新
function applyFocus() {
  document.getElementById('toc').classList.toggle('pane-focused', focusedPane === 'toc' && tocOpen);
  document.getElementById('content').classList.toggle('pane-focused', focusedPane === 'content');
  document.getElementById('notes').classList.toggle('pane-focused', focusedPane === 'notes' && notesOpen);
}

/**
 * 隣接ペインが閉じている場合、リサイズハンドルも非表示にする。
 * 例: TOC が閉じていれば #resize-toc は意味がないので display:none。
 */
function applyResizeHandleVisibility() {
  document.getElementById('resize-toc').classList.toggle('handle-hidden', !tocOpen);
  document.getElementById('resize-notes').classList.toggle('handle-hidden', !notesOpen);
}

/**
 * tocWidth / notesWidth を CSS 変数に書き戻す。
 * TOC / notes が閉じていても値は保持する（再表示時にそのまま復元される）。
 */
function applyPaneWidths() {
  const root = document.documentElement;
  root.style.setProperty('--toc-width', `${tocWidth}px`);
  root.style.setProperty('--notes-width', `${notesWidth}px`);
}

function clampPaneWidth(value, min, max) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, Math.round(value)));
}

/**
 * tocWidth / notesWidth を config.json に保存する。debounce で書き込み頻度を抑える
 * （ドラッグ中の連続更新でも 300ms 静止後に 1 回だけ書く）。
 */
function scheduleLayoutSave() {
  if (layoutSaveTimer !== null) clearTimeout(layoutSaveTimer);
  layoutSaveTimer = setTimeout(async () => {
    layoutSaveTimer = null;
    try {
      const cfg = await window.mdview.loadConfig();
      if (!cfg.layout || typeof cfg.layout !== 'object') cfg.layout = {};
      cfg.layout.toc_width = tocWidth;
      cfg.layout.notes_width = notesWidth;
      await window.mdview.saveConfig(cfg);
    } catch (e) {
      console.warn('mdview: failed to save layout:', e);
    }
  }, LAYOUT_SAVE_DEBOUNCE_MS);
}

/**
 * pane を絶対値 value にリサイズする。範囲外は clamp。
 * 値が変化したら CSS 変数を更新して保存をスケジュール。
 */
function setPaneWidth(pane, value) {
  if (pane === 'toc') {
    const next = clampPaneWidth(value, TOC_WIDTH_MIN, TOC_WIDTH_MAX);
    if (next === tocWidth) return;
    tocWidth = next;
  } else if (pane === 'notes') {
    const next = clampPaneWidth(value, NOTES_WIDTH_MIN, NOTES_WIDTH_MAX);
    if (next === notesWidth) return;
    notesWidth = next;
  } else {
    return;
  }
  applyPaneWidths();
  scheduleLayoutSave();
}

/**
 * pane を delta px 分広げる（負なら狭める）。
 */
function resizePane(pane, delta) {
  const base = pane === 'toc' ? tocWidth : pane === 'notes' ? notesWidth : null;
  if (base === null) return;
  setPaneWidth(pane, base + delta);
}

/**
 * リサイズハンドル（4px 幅の縦バー）にドラッグ用 pointer イベントを登録する。
 * setPointerCapture でハンドルから外れてもイベントを取り続ける（標準 Web パターン）。
 *
 * pane === 'toc' の場合: ハンドルは TOC の右側にあるので、右ドラッグで TOC を広げる
 *   → newWidth = startWidth + dx
 * pane === 'notes' の場合: ハンドルは notes の左側にあるので、右ドラッグで notes を狭める
 *   → newWidth = startWidth - dx
 */
function setupResizeHandle(handleId, pane) {
  const handle = document.getElementById(handleId);
  if (!handle) return;
  handle.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;  // 左クリックのみ
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = pane === 'toc' ? tocWidth : notesWidth;
    handle.classList.add('dragging');
    try { handle.setPointerCapture(e.pointerId); } catch (_) { /* noop */ }

    const onMove = (ev) => {
      const dx = ev.clientX - startX;
      const newWidth = pane === 'toc' ? startWidth + dx : startWidth - dx;
      setPaneWidth(pane, newWidth);
    };
    const onEnd = () => {
      handle.classList.remove('dragging');
      try { handle.releasePointerCapture(e.pointerId); } catch (_) { /* noop */ }
      handle.removeEventListener('pointermove', onMove);
      handle.removeEventListener('pointerup', onEnd);
      handle.removeEventListener('pointercancel', onEnd);
    };
    handle.addEventListener('pointermove', onMove);
    handle.addEventListener('pointerup', onEnd);
    handle.addEventListener('pointercancel', onEnd);
  });
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

// ── メモパネル ────────────────────────────────────────────────────────────

/**
 * スクロール位置に対応する「ビューポート最上部以下にあり最も近い heading」を特定する。
 * orderedHeadings を文書順に走査し、getBoundingClientRect().top が content 上端
 * （content.getBoundingClientRect().top + offset）以下のうち最も大きいものを選ぶ。
 * offset は 40px（toolbar 下の余白分）を見込む。
 *
 * heading が 1 つもない / まだ先頭より上にしかない場合は null を返し、
 * textarea は disabled 状態になる。
 */
function findCurrentHeadingElement() {
  if (orderedHeadings.length === 0) return null;
  const contentEl = document.getElementById('content');
  const contentTop = contentEl.getBoundingClientRect().top;
  const threshold = contentTop + 40;  // 余白 40px

  let candidate = null;
  for (const el of orderedHeadings) {
    const rect = el.getBoundingClientRect();
    if (rect.top <= threshold) {
      candidate = el;
    } else {
      break;  // 文書順に並んでいるので閾値超えたら以降も超える
    }
  }
  return candidate;
}

/**
 * 現在の topmost heading に応じて currentHeadingKey を更新する。
 * 変化があった場合: 前メモを強制保存 → textarea を新 heading のメモに切り替える。
 * 変化がなければ何もしない。
 */
function updateCurrentHeading() {
  const el = findCurrentHeadingElement();
  const newKey = el ? (headingKeyMap.get(el) || null) : null;
  if (anchorKeyEquals(newKey, currentHeadingKey)) return;

  // 遷移前のメモを強制保存（debounce をキャンセルして即 persist）
  flushPendingNote();

  currentHeadingKey = newKey;
  updateNotesPanel();
}

/**
 * currentHeadingKey に対応するメモを notesEntries から引いて textarea に反映する。
 * heading 未特定（null）時は textarea disabled + ラベル切替。
 */
function updateNotesPanel() {
  const ta = document.getElementById('notes-textarea');
  const label = document.getElementById('notes-heading-label');
  if (!ta || !label) return;

  if (!currentHeadingKey) {
    ta.value = '';
    ta.disabled = true;
    label.textContent = '（見出しにスクロール）';
    return;
  }

  ta.disabled = false;
  label.textContent = currentHeadingKey.heading_text || '(無題)';

  const entry = notesEntries.find((e) => anchorKeyEquals(
    { heading_text: e.heading_text, heading_level: e.heading_level, occurrence_index: e.occurrence_index },
    currentHeadingKey
  ));
  ta.value = entry ? entry.note : '';
}

/**
 * textarea の内容を notesEntries に反映し main へ送る（IPC）。
 * note が空文字なら対応 entry を削除（キー保持のコスト削減）。
 */
async function persistCurrentNote() {
  if (!currentHeadingKey || !currentFilePath) return;
  const ta = document.getElementById('notes-textarea');
  if (!ta) return;
  const value = ta.value;
  const now = new Date().toISOString();

  const idx = notesEntries.findIndex((e) => anchorKeyEquals(
    { heading_text: e.heading_text, heading_level: e.heading_level, occurrence_index: e.occurrence_index },
    currentHeadingKey
  ));

  if (value === '') {
    if (idx >= 0) notesEntries.splice(idx, 1);
  } else {
    if (idx >= 0) {
      notesEntries[idx] = { ...notesEntries[idx], note: value, updated_at: now };
    } else {
      notesEntries.push({
        heading_text: currentHeadingKey.heading_text,
        heading_level: currentHeadingKey.heading_level,
        occurrence_index: currentHeadingKey.occurrence_index,
        note: value,
        created_at: now,
        updated_at: now,
      });
    }
  }

  try {
    await window.mdview.notes.set(currentFilePath, notesEntries);
  } catch (e) {
    console.warn('mdview: failed to save notes:', e);
  }
}

/**
 * debounce タイマーが動いていれば即時実行してキャンセル。
 * heading 遷移時・blur 時・外部 reload 時に呼ぶ。
 */
function flushPendingNote() {
  if (notesSaveTimer !== null) {
    clearTimeout(notesSaveTimer);
    notesSaveTimer = null;
    // 同期的に保存（async だが await しない: 遷移処理を止めない）
    persistCurrentNote();
  }
}

/**
 * textarea input のハンドラ。500ms debounce で persist。
 */
function onNotesInput() {
  if (notesSaveTimer !== null) clearTimeout(notesSaveTimer);
  notesSaveTimer = setTimeout(() => {
    notesSaveTimer = null;
    persistCurrentNote();
  }, 500);
}

/**
 * textarea blur のハンドラ。即 persist（debounce をキャンセルして同期実行）。
 */
function onNotesBlur() {
  flushPendingNote();
}

/**
 * notes パネルの表示/非表示を notesOpen 状態に合わせる。
 */
function applyNotesVisibility() {
  document.getElementById('notes').classList.toggle('notes-hidden', !notesOpen);
  applyResizeHandleVisibility();
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

// ── 検索機能 ─────────────────────────────────────────────────────────────

/**
 * smartcase 判定: クエリに大文字が含まれる場合は case-sensitive。
 */
function isSmartcaseSensitive(pattern) {
  return /[A-Z]/.test(pattern);
}

/**
 * 検索クエリから RegExp を構築する。
 * 不正な正規表現の場合は null を返す（例外を外に伝播させない）。
 */
function buildSearchRegex(pattern) {
  const flags = isSmartcaseSensitive(pattern) ? 'g' : 'gi';
  try {
    return new RegExp(pattern, flags);
  } catch (e) {
    return null;
  }
}

/**
 * 既存の検索ハイライト span を全て unwrap し、テキストノードを正規化する。
 */
function clearSearchHighlights() {
  const body = document.getElementById('markdown-body');
  if (!body) return;
  const marks = body.querySelectorAll('span.search-match');
  for (const mark of marks) {
    // span の子ノードを親に移動してから span を削除（unwrap）
    const parent = mark.parentNode;
    if (!parent) continue;
    while (mark.firstChild) {
      parent.insertBefore(mark.firstChild, mark);
    }
    parent.removeChild(mark);
  }
  // テキストノードの断片化を解消
  body.normalize();
  searchMatches = [];
  const countEl = document.getElementById('search-count');
  if (countEl) countEl.textContent = '';
}

/**
 * `#markdown-body` 配下のテキストノードを TreeWalker で走査し、
 * クエリにマッチする箇所を `<span class="search-match">` で wrap する。
 */
function runSearch(query) {
  clearSearchHighlights();
  searchCursor = 0;

  if (!query) {
    updateSearchHighlightState();
    return;
  }

  const re = buildSearchRegex(query);
  const inputEl = document.getElementById('search-input');

  if (!re) {
    // 不正な正規表現: input に .invalid class を付与
    if (inputEl) inputEl.classList.add('invalid');
    const countEl = document.getElementById('search-count');
    if (countEl) countEl.textContent = '';
    return;
  }

  // 正規表現が有効: .invalid を除去
  if (inputEl) inputEl.classList.remove('invalid');

  const body = document.getElementById('markdown-body');
  if (!body) return;

  // TreeWalker でテキストノードを走査
  const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT, null);
  const textNodes = [];
  let node;
  while ((node = walker.nextNode())) {
    textNodes.push(node);
  }

  // 各テキストノードでマッチを検索して span で wrap
  for (const textNode of textNodes) {
    const text = textNode.nodeValue;
    if (!text) continue;

    re.lastIndex = 0;
    const matches = [];
    let m;
    while ((m = re.exec(text)) !== null) {
      matches.push({ start: m.index, end: m.index + m[0].length });
      if (m[0].length === 0) re.lastIndex++; // ゼロ幅マッチで無限ループ回避
    }

    if (matches.length === 0) continue;

    // マッチ箇所を分割して span で wrap する
    const parent = textNode.parentNode;
    if (!parent) continue;

    // フラグメントに分割して挿入
    let lastIndex = 0;
    const fragment = document.createDocumentFragment();
    for (const { start, end } of matches) {
      // マッチ前のテキスト
      if (start > lastIndex) {
        fragment.appendChild(document.createTextNode(text.slice(lastIndex, start)));
      }
      // マッチ部分を span で wrap
      const span = document.createElement('span');
      span.className = 'search-match';
      span.textContent = text.slice(start, end);
      fragment.appendChild(span);
      searchMatches.push(span);
      lastIndex = end;
    }
    // マッチ後の残りテキスト
    if (lastIndex < text.length) {
      fragment.appendChild(document.createTextNode(text.slice(lastIndex)));
    }

    parent.replaceChild(fragment, textNode);
  }

  updateSearchHighlightState();
}

/**
 * 全マッチから `search-current` class を外し、現在のカーソル位置にのみ付与する。
 * `#search-count` のテキストも更新する。
 */
function updateSearchHighlightState() {
  for (const el of searchMatches) {
    el.classList.remove('search-current');
  }
  if (searchMatches.length > 0 && searchMatches[searchCursor]) {
    searchMatches[searchCursor].classList.add('search-current');
  }
  const countEl = document.getElementById('search-count');
  if (countEl) {
    countEl.textContent = searchMatches.length > 0
      ? `${searchCursor + 1}/${searchMatches.length}`
      : '0/0';
  }
}

/**
 * 現在カーソル位置のマッチ要素へスムーズスクロールする。
 */
function scrollToCurrentMatch() {
  const el = searchMatches[searchCursor];
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }
}

/**
 * 次のマッチへ移動する。
 */
function nextMatch() {
  if (searchMatches.length === 0) return;
  searchCursor = (searchCursor + 1) % searchMatches.length;
  updateSearchHighlightState();
  scrollToCurrentMatch();
}

/**
 * 前のマッチへ移動する。
 */
function prevMatch() {
  if (searchMatches.length === 0) return;
  searchCursor = (searchCursor + searchMatches.length - 1) % searchMatches.length;
  updateSearchHighlightState();
  scrollToCurrentMatch();
}

/**
 * 検索バーを開き、入力欄にフォーカスする。
 */
function openSearchBar() {
  const bar = document.getElementById('search-bar');
  if (bar) bar.removeAttribute('hidden');
  document.body.classList.add('search-open');
  const input = document.getElementById('search-input');
  if (input) {
    input.focus();
    input.select();
  }
}

/**
 * 検索バーを閉じ、ハイライトをクリアして本文にフォーカスを戻す。
 */
function closeSearchBar() {
  const bar = document.getElementById('search-bar');
  if (bar) bar.setAttribute('hidden', '');
  document.body.classList.remove('search-open');
  clearSearchHighlights();
  searchQuery = '';
  const countEl = document.getElementById('search-count');
  if (countEl) countEl.textContent = '';
  // 本文へフォーカスを戻す
  const content = document.getElementById('content');
  if (content) content.focus();
}

// ── キーボードハンドラ ────────────────────────────────────────────────────

// キーボード操作ハンドラ
function handleKeyDown(e) {
  // Cmd/Ctrl+F: 検索バーを開く（input フォーカス中・メニュー accelerator check より前に置く）
  if ((e.metaKey || e.ctrlKey) && (e.key === 'f' || e.key === 'F')) {
    e.preventDefault();
    openSearchBar();
    return;
  }

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
      // 1 ペイン左へ移動: notes → content → toc
      if (focusedPane === 'notes') {
        focusedPane = 'content';
      } else if (focusedPane === 'content' && tocOpen) {
        focusedPane = 'toc';
      }
      applyFocus();
      e.preventDefault();
      break;
    case 'l':
    case 'ArrowRight':
      // 1 ペイン右へ移動: toc → content → notes
      if (focusedPane === 'toc') {
        focusedPane = 'content';
      } else if (focusedPane === 'content' && notesOpen) {
        focusedPane = 'notes';
      }
      applyFocus();
      e.preventDefault();
      break;
    case 'H':
      // 現フォーカスのサイドパネルを「左方向に動かす」
      // toc は左端なので狭める、notes は左に動かすと広がる
      if (focusedPane === 'toc' && tocOpen) {
        resizePane('toc', -RESIZE_STEP);
      } else if (focusedPane === 'notes' && notesOpen) {
        resizePane('notes', RESIZE_STEP);
      }
      e.preventDefault();
      break;
    case 'L':
      // 現フォーカスのサイドパネルを「右方向に動かす」
      // toc は右に動かすと広がる、notes は右端なので狭める
      if (focusedPane === 'toc' && tocOpen) {
        resizePane('toc', RESIZE_STEP);
      } else if (focusedPane === 'notes' && notesOpen) {
        resizePane('notes', -RESIZE_STEP);
      }
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
    case 'n':
      notesOpen = !notesOpen;
      if (!notesOpen) {
        // 閉じる前に現在のメモを保存
        flushPendingNote();
        // 閉じるとき notes フォーカスなら本文に戻す
        if (focusedPane === 'notes') {
          focusedPane = 'content';
        }
      }
      applyNotesVisibility();
      applyFocus();
      // config.json に即保存（既存 loadConfig + saveConfig パターン）
      window.mdview.loadConfig().then((cfg) => {
        if (!cfg.notes || typeof cfg.notes !== 'object') cfg.notes = {};
        cfg.notes.panel_open = notesOpen;
        window.mdview.saveConfig(cfg);
      });
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

/**
 * #markdown-body 内の .mermaid-container 全件に対し mermaid.render() を呼び、
 * 返却された SVG 文字列を innerHTML に代入する。
 * ソースは data-mermaid-source 属性から読み取り、WeakMap に移して属性を削除する。
 * parse エラー時は <pre><code class="language-mermaid">元ソース</code></pre> にフォールバック。
 */
async function renderMermaidBlocks() {
  if (!mermaid) return;
  const body = document.getElementById('markdown-body');
  const containers = body.querySelectorAll('.mermaid-container');
  // ID 衝突回避のためカウンタを使う
  let i = 0;
  for (const el of containers) {
    const source = el.dataset.mermaidSource;
    // 属性は削除してソースは WeakMap のみに保持
    el.removeAttribute('data-mermaid-source');
    if (typeof source !== 'string') continue;
    mermaidSources.set(el, source);
    const id = `mermaid-svg-${i++}`;
    try {
      const { svg } = await mermaid.render(id, source);
      el.innerHTML = svg;
    } catch (err) {
      console.warn('mermaid render failed:', err);
      // フォールバック: コードブロックとして表示
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      code.className = 'language-mermaid';
      code.textContent = source;
      pre.appendChild(code);
      el.replaceWith(pre);
    }
  }
}

/**
 * テーマ切替時に既存の mermaid container を全再レンダリングする。
 * ソースは WeakMap に保持されているので data 属性に書き戻してから renderMermaidBlocks() を呼ぶ。
 */
async function reRenderAllMermaid() {
  if (!mermaid) return;
  const body = document.getElementById('markdown-body');
  // .mermaid-container は Step 7 で innerHTML が SVG に置換されているか、
  // フォールバック時は <pre> に replaceWith されて .mermaid-container 自体が消滅している。
  // 前者のみを対象に、WeakMap からソースを復元して data 属性に戻し、再レンダリング。
  const containers = body.querySelectorAll('.mermaid-container');
  for (const el of containers) {
    const source = mermaidSources.get(el);
    if (typeof source === 'string') {
      el.setAttribute('data-mermaid-source', source);
      el.innerHTML = '';
    }
  }
  await renderMermaidBlocks();
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
  if (doc.schema_version !== expectedSchemaVersion) {
    document.getElementById('markdown-body').textContent =
      'Unsupported schema version: got ' + doc.schema_version +
      ', expected ' + expectedSchemaVersion;
    return;
  }

  renderDocument(doc);

  buildToc(doc.toc);

  // mermaid 図を先に SVG 化（parse 失敗時は <pre><code class="language-mermaid"> にフォールバックされる）
  await renderMermaidBlocks();

  // highlight.js でコードブロックをハイライト
  // （mermaid 成功ブロックは <pre><code> を含まない SVG に置換済みなのでハイライト対象外）
  if (hljs) {
    document.getElementById('markdown-body').querySelectorAll('pre code').forEach((el) => {
      hljs.highlightElement(el);
    });
  }

  // DOM 再構築後に stale な searchMatches 参照を除去（reload/ファイル切替後の count 誤表示防止）
  clearSearchHighlights();

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

  // mermaid 読み込み（applyTheme 呼び出し前にロードし初回テーマ適用時に即座に反映）
  await loadMermaid();

  // config を読み込んでテーマを適用
  let config = null;
  try {
    config = await window.mdview.loadConfig();
    const themeId = (config && config.theme) || DEFAULT_THEME_ID;
    applyTheme(themeId);
  } catch (e) {
    console.warn('mdview: failed to load config, using default theme:', e);
    applyTheme(DEFAULT_THEME_ID);
  }

  // config から notesOpen 初期値を設定
  notesOpen = config?.notes?.panel_open !== false;  // undefined / null / true → true
  applyNotesVisibility();

  // config からパネル幅初期値を設定（main.js 側で clamp 済みだが念のため再 clamp）
  if (config?.layout) {
    tocWidth = clampPaneWidth(config.layout.toc_width, TOC_WIDTH_MIN, TOC_WIDTH_MAX);
    notesWidth = clampPaneWidth(config.layout.notes_width, NOTES_WIDTH_MIN, NOTES_WIDTH_MAX);
  }
  applyPaneWidths();
  applyResizeHandleVisibility();
  setupResizeHandle('resize-toc', 'toc');
  setupResizeHandle('resize-notes', 'notes');

  // メニュー「テーマ」変更通知を受信してテーマを切り替え
  window.mdview.onThemeChanged(({ id }) => {
    applyTheme(id);
  });

  // ファイルオープンボタン
  document.getElementById('open-btn').addEventListener('click', async () => {
    const result = await window.mdview.openFile();
    if (result) {
      flushPendingNote();
      setCurrentFile(result.path);
      try {
        const res = await window.mdview.notes.get(result.path);
        notesEntries = (res && Array.isArray(res.entries)) ? res.entries : [];
      } catch (e) {
        console.warn('mdview: failed to load notes:', e);
        notesEntries = [];
      }
      await renderMarkdown(result.text);
      currentHeadingKey = null;
      updateCurrentHeading();
    }
  });

  // Main プロセスからのファイル（CLI引数 or メニュー）
  window.mdview.onFileOpened(async (data) => {
    // 前ファイルのメモが未保存なら強制保存（debounce キャンセル）
    flushPendingNote();

    setCurrentFile(data.path);
    // notes を main から取得してから render（render 中の updateCurrentHeading で参照するため）
    try {
      const res = await window.mdview.notes.get(data.path);
      notesEntries = (res && Array.isArray(res.entries)) ? res.entries : [];
    } catch (e) {
      console.warn('mdview: failed to load notes:', e);
      notesEntries = [];
    }
    await renderMarkdown(data.text);
    // render 後に currentHeadingKey を初期化（scroll 位置 0 で最初の heading を拾う）
    currentHeadingKey = null;
    updateCurrentHeading();
  });

  // ファイル変更検知（ホットリロード）
  window.mdview.onFileChanged(async (data) => {
    // 外部編集前のメモを保存
    flushPendingNote();

    const contentEl = document.getElementById('content');
    const scrollY = contentEl.scrollTop;
    setCurrentFile(data.path);
    try {
      const res = await window.mdview.notes.get(data.path);
      notesEntries = (res && Array.isArray(res.entries)) ? res.entries : [];
    } catch (e) {
      console.warn('mdview: failed to load notes:', e);
      notesEntries = [];
    }
    await renderMarkdown(data.text);
    contentEl.scrollTop = scrollY;
    currentHeadingKey = null;
    updateCurrentHeading();
  });

  // ファイル削除検知
  window.mdview.onFileMissing((data) => {
    flushPendingNote();  // 直前の編集は保存を試みる（filePath は直前のまま有効）
    notesEntries = [];
    currentHeadingKey = null;
    orderedHeadings = [];
    headingKeyMap = new WeakMap();
    updateNotesPanel();

    const body = document.getElementById('markdown-body');
    body.innerHTML =
      '<p class="placeholder">ファイルが見つかりません: ' +
      esc(data.path) +
      '</p>';
    setStatusBarError('ファイルが見つかりません: ' + data.path.split('/').pop());
  });

  // ファイル読み込みエラー
  window.mdview.onFileError((data) => {
    flushPendingNote();
    notesEntries = [];
    currentHeadingKey = null;
    orderedHeadings = [];
    headingKeyMap = new WeakMap();
    updateNotesPanel();

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

  // 検索バーのリスナー登録（初期化 1 回のみ）
  const searchInput = document.getElementById('search-input');
  if (searchInput) {
    searchInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        // incremental search 済みなので cursor を進めてジャンプ
        if (searchMatches.length > 0) {
          nextMatch();
        } else {
          // 初回 Enter: まだ runSearch していない場合に備えて実行
          runSearch(searchInput.value);
          scrollToCurrentMatch();
        }
      } else if (e.key === 'Enter' && e.shiftKey) {
        e.preventDefault();
        prevMatch();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        closeSearchBar();
      }
    });
    // incremental search: 入力のたびに runSearch を実行
    searchInput.addEventListener('input', (e) => {
      searchQuery = e.target.value;
      runSearch(searchQuery);
    });
  }

  const searchPrevBtn = document.getElementById('search-prev');
  if (searchPrevBtn) {
    searchPrevBtn.addEventListener('click', () => prevMatch());
  }

  const searchNextBtn = document.getElementById('search-next');
  if (searchNextBtn) {
    searchNextBtn.addEventListener('click', () => nextMatch());
  }

  const searchCloseBtn = document.getElementById('search-close');
  if (searchCloseBtn) {
    searchCloseBtn.addEventListener('click', () => closeSearchBar());
  }

  // textarea のリスナー
  const notesTa = document.getElementById('notes-textarea');
  if (notesTa) {
    notesTa.addEventListener('input', onNotesInput);
    notesTa.addEventListener('blur', onNotesBlur);
  }

  // スクロール位置変化をステータスバーとメモパネルに反映
  document.getElementById('content').addEventListener('scroll', () => {
    updateStatusBar();
    // requestAnimationFrame で throttle（毎フレーム1回以下に抑制）
    if (!pendingScrollFrame) {
      pendingScrollFrame = true;
      requestAnimationFrame(() => {
        pendingScrollFrame = false;
        updateCurrentHeading();
      });
    }
  });

  // 初期ステータスバー表示
  updateStatusBar();

  // すべてのリスナー登録と WASM 初期化が終わったことを main に通知
  // （これを受けて main が CLI 引数のファイルを file:opened で送る）
  window.mdview.notifyReady();
}

main().catch(console.error);
