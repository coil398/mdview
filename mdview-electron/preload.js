const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('mdview', {
  openFile: () => ipcRenderer.invoke('dialog:openFile'),
  onFileOpened: (callback) => {
    ipcRenderer.removeAllListeners('file:opened');
    ipcRenderer.on('file:opened', (_event, data) => callback(data));
  },
});
