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
    return { schema_version: 1, theme: DEFAULT_THEME_ID };
  }
  try {
    const text = fs.readFileSync(configPath, 'utf-8');
    const parsed = JSON.parse(text);
    if (!parsed.theme || !VALID_THEME_IDS.includes(parsed.theme)) {
      console.warn(`mdview: unknown or missing theme id "${parsed.theme}", using default.`);
      parsed.theme = DEFAULT_THEME_ID;
    }
    return parsed;
  } catch (e) {
    console.warn(`mdview: failed to load config (${configPath}): ${e}. using default.`);
    return { schema_version: 1, theme: DEFAULT_THEME_ID };
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
