const { app, BrowserWindow, ipcMain, dialog, Menu, protocol, net } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const { pathToFileURL } = require('node:url');

// Electron 公式推奨: sandbox 下 renderer で file:// を避け、secure + supportFetchAPI 付きのカスタムスキームから配信する。
// standard: true → 相対 URL 解決、secure: true → CSP の 'self' 扱い・混合コンテンツ扱い回避、
// supportFetchAPI: true → fetch / WebAssembly.instantiateStreaming 対応
// この呼び出しは app の ready イベント前（トップレベル）に行う必要がある。
protocol.registerSchemesAsPrivileged([
  {
    scheme: 'app',
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
    },
  },
]);

// 各ウィンドウごとの監視状態。WebContents をキーにして複数ウィンドウをサポート
const watcherStates = new WeakMap();
// watcherStates.get(webContents) => { watcher: fs.FSWatcher, watchedPath: string, debounceTimer: NodeJS.Timeout }
const RELOAD_DEBOUNCE_MS = 80;   // TUI の notify_debouncer_full は 300ms だが、Electron/JS では軽いため 80ms に設定

// ── notes.json 管理 ─────────────────────────────────────────────────────────

const NOTES_SCHEMA_VERSION = 1;

/**
 * notes.json のパス解決。config.json と同じ XDG 規約。
 */
function getNotesPath() {
  const xdg = process.env.XDG_CONFIG_HOME;
  if (xdg && xdg.length > 0) {
    return path.join(xdg, 'mdview', 'notes.json');
  }
  return path.join(os.homedir(), '.config', 'mdview', 'notes.json');
}

// in-memory キャッシュ。app.whenReady() で loadNotesStore() を呼んで初期化する。
let notesStore = { schema_version: NOTES_SCHEMA_VERSION, notes_by_file: {} };

function loadNotesStore() {
  const p = getNotesPath();
  if (!fs.existsSync(p)) {
    notesStore = { schema_version: NOTES_SCHEMA_VERSION, notes_by_file: {} };
    return;
  }
  try {
    const text = fs.readFileSync(p, 'utf-8');
    const parsed = JSON.parse(text);
    if (typeof parsed !== 'object' || parsed === null) throw new Error('not an object');
    if (typeof parsed.notes_by_file !== 'object' || parsed.notes_by_file === null) {
      parsed.notes_by_file = {};
    }
    if (parsed.schema_version !== NOTES_SCHEMA_VERSION) {
      console.warn(`mdview: notes.json schema_version mismatch (got ${parsed.schema_version}, expected ${NOTES_SCHEMA_VERSION}), coercing.`);
      parsed.schema_version = NOTES_SCHEMA_VERSION;
    }
    notesStore = parsed;
  } catch (e) {
    console.warn(`mdview: failed to load notes (${p}): ${e}. using empty store.`);
    notesStore = { schema_version: NOTES_SCHEMA_VERSION, notes_by_file: {} };
  }
}

/**
 * atomic write: tmp に書いて rename で置換。
 * tmp 名は同ディレクトリに `.notes.json.tmp-<pid>-<rand>` を使う。
 * rename は POSIX 上では atomic、Windows でも NTFS 上は ReplaceFileW 経由で同等に扱われる。
 */
function saveNotesStore() {
  const p = getNotesPath();
  const dir = path.dirname(p);
  try {
    fs.mkdirSync(dir, { recursive: true });
    const tmp = path.join(dir, `.notes.json.tmp-${process.pid}-${Math.random().toString(36).slice(2, 8)}`);
    fs.writeFileSync(tmp, JSON.stringify(notesStore, null, 2), 'utf-8');
    fs.renameSync(tmp, p);
  } catch (e) {
    console.error(`mdview: failed to save notes (${p}): ${e}`);
  }
}

/**
 * entries バリデーション。renderer から渡る任意 JSON を検証。
 * - 配列であること
 * - 各要素が { heading_text: string, heading_level: number (1..6), occurrence_index: number>=0, note: string, created_at?: string, updated_at?: string }
 * 不正な要素は除外し、有効な要素のみを返す。
 */
function validateNotesEntries(entries) {
  if (!Array.isArray(entries)) return [];
  return entries.filter((e) =>
    e !== null && typeof e === 'object'
    && typeof e.heading_text === 'string'
    && typeof e.heading_level === 'number' && e.heading_level >= 1 && e.heading_level <= 6
    && typeof e.occurrence_index === 'number' && e.occurrence_index >= 0
    && typeof e.note === 'string'
  );
}

// ── テーマ設定 ─────────────────────────────────────────────────────────────

/**
 * テーマ ID → BrowserWindow 背景色のマッピング（ウィンドウちらつき防止用）。
 * renderer.js の THEME_REGISTRY と同期させること。
 */
const THEME_BACKGROUNDS = {
  'vscode-dark': '#1e1e1e',
  'vscode-light': '#ffffff',
  'github-dark': '#0d1117',
  'github-light': '#ffffff',
};

const VALID_THEME_IDS = ['vscode-dark', 'vscode-light', 'github-dark', 'github-light'];
const DEFAULT_THEME_ID = 'vscode-dark';

// レイアウト（TOC / notes 幅）の制約。renderer.js の対応定数と同期させること。
const TOC_WIDTH_DEFAULT = 240;
const TOC_WIDTH_MIN = 120;
const TOC_WIDTH_MAX = 600;
const NOTES_WIDTH_DEFAULT = 280;
const NOTES_WIDTH_MIN = 160;
const NOTES_WIDTH_MAX = 800;

function clampWidth(value, min, max, fallback) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}

function defaultLayout() {
  return { toc_width: TOC_WIDTH_DEFAULT, notes_width: NOTES_WIDTH_DEFAULT };
}

/**
 * `$XDG_CONFIG_HOME/mdview/config.json` のパスを返す。
 * Rust 側（config.rs）と同一ロジック:
 *   1. $XDG_CONFIG_HOME
 *   2. $HOME/.config
 */
function getConfigPath() {
  const xdg = process.env.XDG_CONFIG_HOME;
  if (xdg && xdg.length > 0) {
    return path.join(xdg, 'mdview', 'config.json');
  }
  return path.join(os.homedir(), '.config', 'mdview', 'config.json');
}

/**
 * config.json を同期読み込みして返す。
 * ファイル不在・パースエラー時は default を返し、console.warn を出す。
 */
function loadConfig() {
  const configPath = getConfigPath();
  if (!fs.existsSync(configPath)) {
    return {
      schema_version: 2,
      theme: DEFAULT_THEME_ID,
      notes: { panel_open: true },
      layout: defaultLayout(),
    };
  }
  try {
    const text = fs.readFileSync(configPath, 'utf-8');
    const parsed = JSON.parse(text);
    if (!parsed.theme || !VALID_THEME_IDS.includes(parsed.theme)) {
      console.warn(`mdview: unknown or missing theme id "${parsed.theme}", using default.`);
      parsed.theme = DEFAULT_THEME_ID;
    }
    // schema_version v1 → v2 の後方互換: notes.panel_open がなければ追加
    if (parsed.schema_version !== 2) {
      parsed.schema_version = 2;
    }
    if (!parsed.notes || typeof parsed.notes !== 'object') {
      parsed.notes = { panel_open: true };
    }
    if (typeof parsed.notes.panel_open !== 'boolean') {
      parsed.notes.panel_open = true;
    }
    // layout フィールドは後方互換補完（schema bump なしで追加）
    if (!parsed.layout || typeof parsed.layout !== 'object') {
      parsed.layout = defaultLayout();
    }
    parsed.layout.toc_width = clampWidth(
      parsed.layout.toc_width, TOC_WIDTH_MIN, TOC_WIDTH_MAX, TOC_WIDTH_DEFAULT,
    );
    parsed.layout.notes_width = clampWidth(
      parsed.layout.notes_width, NOTES_WIDTH_MIN, NOTES_WIDTH_MAX, NOTES_WIDTH_DEFAULT,
    );
    return parsed;
  } catch (e) {
    console.warn(`mdview: failed to load config (${configPath}): ${e}. using default.`);
    return {
      schema_version: 2,
      theme: DEFAULT_THEME_ID,
      notes: { panel_open: true },
      layout: defaultLayout(),
    };
  }
}

/**
 * config.json に書き込む。ディレクトリが無ければ作成する。
 */
function saveConfig(config) {
  const configPath = getConfigPath();
  try {
    fs.mkdirSync(path.dirname(configPath), { recursive: true });
    fs.writeFileSync(configPath, JSON.stringify(config, null, 2), 'utf-8');
  } catch (e) {
    console.error(`mdview: failed to save config (${configPath}): ${e}`);
  }
}

// ── ファイル監視 ───────────────────────────────────────────────────────────

function stopWatching(webContents) {
  const state = watcherStates.get(webContents);
  if (!state) return;
  if (state.watcher) state.watcher.close();
  clearTimeout(state.debounceTimer);
  watcherStates.delete(webContents);
}

function reloadAndSend(win, filePath) {
  if (win.isDestroyed()) return;
  if (!fs.existsSync(filePath)) {
    win.webContents.send('file:missing', { path: filePath });
    return;
  }
  try {
    const text = fs.readFileSync(filePath, 'utf-8');
    win.webContents.send('file:changed', { path: filePath, text });
  } catch (e) {
    win.webContents.send('file:error', { path: filePath, message: String(e) });
  }
}

function startWatching(win, filePath) {
  stopWatching(win.webContents);
  const dir = path.dirname(filePath);
  const basename = path.basename(filePath);
  try {
    const watcher = fs.watch(dir, { persistent: true, recursive: false }, (eventType, changedFilename) => {
      // changedFilename が null/undefined の場合（一部プラットフォーム）も処理する
      if (changedFilename !== null && changedFilename !== undefined && changedFilename !== basename) {
        return;
      }
      const state = watcherStates.get(win.webContents);
      if (!state) return;
      clearTimeout(state.debounceTimer);
      state.debounceTimer = setTimeout(() => reloadAndSend(win, filePath), RELOAD_DEBOUNCE_MS);
    });
    watcher.on('error', (e) => {
      console.error('File watcher error:', e);
    });
    watcherStates.set(win.webContents, { watcher, watchedPath: filePath, debounceTimer: null });
  } catch (e) {
    console.error('Failed to start file watcher:', e);
  }
}

// ── ウィンドウ生成 ─────────────────────────────────────────────────────────

function createWindow(filePath, config) {
  const themeId = (config && config.theme) || DEFAULT_THEME_ID;
  const bgColor = THEME_BACKGROUNDS[themeId] || THEME_BACKGROUNDS[DEFAULT_THEME_ID];

  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    backgroundColor: bgColor,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: true,
    },
  });
  // closed イベント後は win.webContents にアクセスできないため事前にキャプチャ
  const wc = win.webContents;

  win.loadURL('app://local/renderer/index.html');

  // renderer の初期化完了（WASM init 等）を待ってから初期ファイルを送る。
  // did-finish-load は DOM 読込完了時点で発火するが、そこでは onFileOpened リスナーが
  // まだ登録されておらず IPC を取りこぼすため、renderer から明示通知を受ける方式にする。
  const onRendererReady = (event) => {
    if (event.sender !== win.webContents) return;
    ipcMain.removeListener('renderer:ready', onRendererReady);
    if (!filePath) return;
    try {
      const text = fs.readFileSync(filePath, 'utf-8');
      win.setTitle(`mdview — ${path.basename(filePath)}`);
      win.webContents.send('file:opened', { path: filePath, text });
      startWatching(win, filePath);
    } catch (e) {
      console.error('Failed to read file:', e);
    }
  };
  ipcMain.on('renderer:ready', onRendererReady);

  win.on('closed', () => {
    stopWatching(wc);
  });

  return win;
}

// ── メニュー構築 ───────────────────────────────────────────────────────────

function buildMenu(config) {
  const currentThemeId = (config && config.theme) || DEFAULT_THEME_ID;

  const themeSubmenu = [
    {
      label: 'VS Code Light',
      type: 'radio',
      checked: currentThemeId === 'vscode-light',
      click: () => applyThemeFromMenu('vscode-light'),
    },
    {
      label: 'VS Code Dark',
      type: 'radio',
      checked: currentThemeId === 'vscode-dark',
      click: () => applyThemeFromMenu('vscode-dark'),
    },
    {
      label: 'GitHub Light',
      type: 'radio',
      checked: currentThemeId === 'github-light',
      click: () => applyThemeFromMenu('github-light'),
    },
    {
      label: 'GitHub Dark',
      type: 'radio',
      checked: currentThemeId === 'github-dark',
      click: () => applyThemeFromMenu('github-dark'),
    },
  ];

  const menu = Menu.buildFromTemplate([
    {
      label: 'ファイル',
      submenu: [
        {
          label: 'ファイルを開く',
          accelerator: 'CmdOrCtrl+O',
          click: async () => {
            const win = BrowserWindow.getFocusedWindow();
            if (!win) return;
            const result = await dialog.showOpenDialog(win, {
              properties: ['openFile'],
              filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }],
            });
            if (!result.canceled && result.filePaths.length > 0) {
              const fPath = result.filePaths[0];
              try {
                const text = fs.readFileSync(fPath, 'utf-8');
                win.setTitle(`mdview — ${path.basename(fPath)}`);
                win.webContents.send('file:opened', { path: fPath, text });
                startWatching(win, fPath);
              } catch (e) {
                console.error('Failed to read file:', e);
              }
            }
          },
        },
        { type: 'separator' },
        { role: 'quit', label: '終了' },
      ],
    },
    // 編集メニュー: コピー / ペースト / 全選択などの標準ショートカットを有効化するため必須。
    // カスタム ApplicationMenu を設定するとデフォルトメニューが上書きされ、
    // editMenu ロールを含めないと Cmd+C / Cmd+A 等が動かなくなる。
    {
      label: '編集',
      submenu: [
        { role: 'undo', label: '元に戻す' },
        { role: 'redo', label: 'やり直し' },
        { type: 'separator' },
        { role: 'cut', label: '切り取り' },
        { role: 'copy', label: 'コピー' },
        { role: 'paste', label: '貼り付け' },
        { role: 'selectAll', label: 'すべて選択' },
      ],
    },
    {
      label: '表示',
      submenu: [
        { role: 'reload', label: '再読み込み' },
        { role: 'toggleDevTools', label: '開発者ツール' },
        { type: 'separator' },
        {
          label: 'テーマ',
          submenu: themeSubmenu,
        },
        { type: 'separator' },
        { role: 'togglefullscreen', label: 'フルスクリーン' },
      ],
    },
  ]);
  return menu;
}

/**
 * メニューからテーマを選択したとき:
 * 1. config.json に保存
 * 2. 全ウィンドウに theme:changed を送信
 * 3. メニューを再構築して radio の checked を更新
 */
function applyThemeFromMenu(themeId) {
  const config = loadConfig();
  config.theme = themeId;
  saveConfig(config);

  // 全ウィンドウに通知
  BrowserWindow.getAllWindows().forEach((win) => {
    if (!win.isDestroyed()) {
      win.webContents.send('theme:changed', { id: themeId });
    }
  });

  // メニューを再構築して radio 状態を更新
  const newMenu = buildMenu(config);
  Menu.setApplicationMenu(newMenu);
}

// ── IPC ───────────────────────────────────────────────────────────────────

ipcMain.handle('config:load', () => {
  return loadConfig();
});

ipcMain.handle('config:save', (_event, newConfig) => {
  // renderer から受け取った config を保存（保存先パスは main が自己解決、引数で受け取らない）
  saveConfig(newConfig);
  return { ok: true };
});

ipcMain.handle('dialog:openFile', async () => {
  const win = BrowserWindow.getFocusedWindow();
  const result = await dialog.showOpenDialog(win, {
    properties: ['openFile'],
    filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }],
  });
  if (result.canceled || result.filePaths.length === 0) return null;
  try {
    const fPath = result.filePaths[0];
    const text = fs.readFileSync(fPath, 'utf-8');
    if (win) {
      win.setTitle(`mdview — ${path.basename(fPath)}`);
      startWatching(win, fPath);
    }
    return { path: fPath, text };
  } catch (e) {
    console.error('Failed to read file:', e);
    return null;
  }
});

/**
 * renderer から受け取った filePath を「現在そのウィンドウが監視しているファイル」と照合する。
 * 一致しない場合は null を返し、呼び出し元で空配列/エラーにフォールバックさせる。
 * watcherStates.get(event.sender)?.watchedPath と strict equal で比較する。
 */
function authorizeNotesAccess(event, filePath) {
  if (typeof filePath !== 'string' || filePath.length === 0) return null;
  const state = watcherStates.get(event.sender);
  if (!state || state.watchedPath !== filePath) return null;
  return filePath;
}

ipcMain.handle('notes:get', (event, filePath) => {
  const authed = authorizeNotesAccess(event, filePath);
  if (!authed) return { entries: [] };
  const bucket = notesStore.notes_by_file[authed];
  if (!bucket || !Array.isArray(bucket.entries)) return { entries: [] };
  return { entries: bucket.entries };
});

ipcMain.handle('notes:set', (event, payload) => {
  if (!payload || typeof payload !== 'object') return { ok: false };
  const authed = authorizeNotesAccess(event, payload.filePath);
  if (!authed) return { ok: false };
  if (!Array.isArray(payload.entries)) return { ok: false };
  const entries = validateNotesEntries(payload.entries);
  const now = new Date().toISOString();
  if (entries.length === 0) {
    // 空配列は bucket ごと削除（肥大化防止）
    delete notesStore.notes_by_file[authed];
  } else {
    notesStore.notes_by_file[authed] = {
      updated_at: now,
      entries,
    };
  }
  saveNotesStore();
  return { ok: true };
});

ipcMain.on('reload:current', (event) => {
  const win = BrowserWindow.fromWebContents(event.sender);
  if (!win || win.isDestroyed()) return;
  const state = watcherStates.get(event.sender);
  if (!state || !state.watchedPath) return;
  reloadAndSend(win, state.watchedPath);
});

// ── アプリ初期化 ───────────────────────────────────────────────────────────

app.whenReady().then(() => {
  // app://local/<relative-path> を mdview-electron/ 配下に解決する。
  // sandbox 下 renderer でも fetch / WebAssembly.instantiateStreaming が動作する。
  protocol.handle('app', (request) => {
    const url = new URL(request.url);
    // host は 'local' 固定。他のオリジンと混同されないよう 404 を返す。
    if (url.host !== 'local') {
      return new Response('Not Found', { status: 404 });
    }
    // pathname の先頭 '/' を削って __dirname 相対に解決
    const relPath = decodeURIComponent(url.pathname).replace(/^\/+/, '');
    // path traversal 防止: path.normalize 後に __dirname + sep で prefix check
    const resolved = path.normalize(path.join(__dirname, relPath));
    if (!resolved.startsWith(path.normalize(__dirname) + path.sep) && resolved !== path.normalize(__dirname)) {
      return new Response('Forbidden', { status: 403 });
    }
    return net.fetch(pathToFileURL(resolved).toString());
  });

  // notes ストアを同期読み込み（最初のウィンドウ表示前に済ませる）
  loadNotesStore();

  // 起動時に config を同期読み込み（ウィンドウ生成前に themeId が必要）
  const config = loadConfig();

  const filePath = process.argv[2] || null;
  const win = createWindow(filePath, config);

  // メニューを config の theme を反映して構築
  const menu = buildMenu(config);
  Menu.setApplicationMenu(menu);

  // renderer:ready は createWindow 内で登録済みだが、
  // メニュー構築後に発火する場合も想定してここでは追加処理なし

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      const cfg = loadConfig();
      createWindow(null, cfg);
      Menu.setApplicationMenu(buildMenu(cfg));
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
