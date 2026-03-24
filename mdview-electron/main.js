const { app, BrowserWindow, ipcMain, dialog, Menu } = require('electron');
const path = require('path');
const fs = require('fs');

function createWindow(filePath) {
  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    backgroundColor: '#1e1e2e',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      nodeIntegration: false,
      contextIsolation: true,
    },
  });

  win.loadFile(path.join(__dirname, 'renderer', 'index.html'));

  win.webContents.on('did-finish-load', () => {
    if (filePath) {
      try {
        const text = fs.readFileSync(filePath, 'utf-8');
        win.webContents.send('file:opened', { path: filePath, text });
      } catch (e) {
        console.error('Failed to read file:', e);
      }
    }
  });

  const menu = Menu.buildFromTemplate([
    {
      label: 'ファイル',
      submenu: [
        {
          label: 'ファイルを開く',
          accelerator: 'CmdOrCtrl+O',
          click: async () => {
            const result = await dialog.showOpenDialog(win, {
              properties: ['openFile'],
              filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }],
            });
            if (!result.canceled && result.filePaths.length > 0) {
              const fPath = result.filePaths[0];
              try {
                const text = fs.readFileSync(fPath, 'utf-8');
                win.webContents.send('file:opened', { path: fPath, text });
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
        { role: 'togglefullscreen', label: 'フルスクリーン' },
      ],
    },
  ]);
  Menu.setApplicationMenu(menu);

  return win;
}

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
    return { path: fPath, text };
  } catch (e) {
    console.error('Failed to read file:', e);
    return null;
  }
});

app.whenReady().then(() => {
  const filePath = process.argv[2] || null;
  createWindow(filePath);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow(null);
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
