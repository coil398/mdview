import init, { parse_markdown_to_json, schema_version } from '../wasm/mdview_core.js';

let hljs = null;

let tocOpen = true;            // 初期状態: 表示
let tocSelectedIndex = 0;      // TOC 開時の選択項目 index（Enter ジャンプ対象）
let currentToc = [];           // 最新の doc.toc を保持（keydown ハンドラから参照）
let focusedPane = 'content';   // 'toc' | 'content' — キー入力の移動対象ペイン
let currentFilePath = null;    // ステータスバー表示用（絶対パス）
let expectedSchemaVersion = null; // WASM 初期化後に schema_version() で設定

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
    case 'n':
      notesOpen = !notesOpen;
      if (!notesOpen) {
        // 閉じる前に現在のメモを保存
        flushPendingNote();
      }
      applyNotesVisibility();
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
