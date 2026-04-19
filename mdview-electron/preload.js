const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('mdview', {
  openFile: () => ipcRenderer.invoke('dialog:openFile'),
  notifyReady: () => ipcRenderer.send('renderer:ready'),
  reloadCurrent: () => ipcRenderer.send('reload:current'),
  onFileOpened: (callback) => {
    ipcRenderer.removeAllListeners('file:opened');
    ipcRenderer.on('file:opened', (_event, data) => callback(data));
  },
  onFileChanged: (callback) => {
    ipcRenderer.removeAllListeners('file:changed');
    ipcRenderer.on('file:changed', (_event, data) => callback(data));
  },
  onFileMissing: (callback) => {
    ipcRenderer.removeAllListeners('file:missing');
    ipcRenderer.on('file:missing', (_event, data) => callback(data));
  },
  onFileError: (callback) => {
    ipcRenderer.removeAllListeners('file:error');
    ipcRenderer.on('file:error', (_event, data) => callback(data));
  },
  loadConfig: () => ipcRenderer.invoke('config:load'),
  saveConfig: (config) => ipcRenderer.invoke('config:save', config),
  onThemeChanged: (cb) => {
    ipcRenderer.removeAllListeners('theme:changed');
    ipcRenderer.on('theme:changed', (_e, data) => cb(data));
  },
});
