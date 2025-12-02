import { KShell } from "./kshell"

export namespace Main {
  let mainWindow: Electron.BrowserWindow;
  let application: Electron.App;
  let BrowserWindow: any;
  let kshell: KShell;

  export function onWindowAllClosed() {
    application.quit();
  }

  export function onClose(): void {
    mainWindow.destroy();
  }

  export function onReady(): void {
    mainWindow = new BrowserWindow({
      width: 650, height: 560, 
      minWidth: 650, minHeight: 560, 
      maxWidth: 650, maxHeight: 560,
      maximizable: false, resizable: false, 
      webPreferences: {
        nodeIntegration: true,
        contextIsolation: false,
        enableRemoteModule: true,
      }
    });
    mainWindow.removeMenu();
    mainWindow.loadFile(`${__dirname}/../index.html`);
    kshell = new KShell(mainWindow.webContents);
    mainWindow.webContents.once("dom-ready", async () => {
      await kshell.start();
    });
    //mainWindow.webContents.openDevTools();
    mainWindow.on('closed', Main.onClose);
  }

  export function main(app: Electron.App, browserWindow: typeof BrowserWindow): void {
    BrowserWindow = browserWindow;
    application = app;
    application.setName("Keycard Shell Updater");
    application.on('window-all-closed', Main.onWindowAllClosed);
    application.on('ready', Main.onReady);
  }
}