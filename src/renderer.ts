import { IpcRendererEvent } from "electron";
import { UI } from "./ui";
import { latestVersionCheckObj } from "./kshell";
const { ipcRenderer } = require('electron');

const fwUpdateOnlineBtn = document.getElementById("btn-fw-update") as HTMLButtonElement;
const dbUpdateOnlineBtn = document.getElementById("btn-erc20-update") as HTMLButtonElement;
const fwUpdateLocalBtn = document.getElementById("fw-upload-local") as HTMLButtonElement;
const dbUpdateLocalBtn = document.getElementById("erc20-upload-local") as HTMLButtonElement;

export function handleUpdateProgress(event: string) : void {
  ipcRenderer.on(event, (_ : any, progress: number) => {
    let finished = UI.handleLoadProgress(progress);

    if(finished) {
      ipcRenderer.send("last-chunk");
    }
  });
}

function updateOnlineStatus() : void {
  ipcRenderer.send('online-status-changed', navigator.onLine ? 'online' : 'offline');
}

ipcRenderer.on("shell-connected", (_ : any, connected: boolean, isLatestVersions: {isFwLatest: boolean, isDBLatest: boolean}) => {
  UI.handleConnected(connected);
  fwUpdateOnlineBtn.disabled = isLatestVersions.isFwLatest;
  dbUpdateOnlineBtn.disabled = isLatestVersions.isDBLatest;
});

ipcRenderer.on("shell-disconnected", (_ : any, connected: boolean) => {
  UI.handleConnected(connected);
  UI.disableProgressBar();
});

ipcRenderer.on("set-version", (_: any, db_ver: number, fw_ver: string) => {
  UI.setOnlineUpdateLabel(db_ver.toString(), fw_ver);
});

ipcRenderer.on("disable-online-update", () => {
  fwUpdateOnlineBtn.disabled = true;
  dbUpdateOnlineBtn.disabled = true;
  UI.setOnlineUpdateLabel();
});

ipcRenderer.on("initialize-update", (_: any, length: number) => {
  UI.initializeProgressBar(length);
});

ipcRenderer.on("no-fw-update-needed", () => {
  UI.updateStatusMessage("Firmware up-to-date", "success");
  UI.enableBackBtn("Back to home");
});

ipcRenderer.on("no-db-update-needed", () => {
  UI.updateStatusMessage("Database up-to-date", "success");
  UI.enableBackBtn("Back to home");
});

ipcRenderer.on("updating-firmware", () => {
  UI.enableProgressBar();
});

ipcRenderer.on("firmware-updated", (_: any) => {
  UI.disableProgressBar();
  UI.enableBackBtn("Back to home");
  UI.updateStatusMessage("Successfully updated", "success");
});

ipcRenderer.on("updating-db", () => {
  UI.enableProgressBar();
});

ipcRenderer.on("db-updated", (_: any, isLatestDB: boolean) => {
  UI.disableProgressBar();
  UI.enableBackBtn("Back to home");
  UI.updateStatusMessage("Successfully updated", "success");
  dbUpdateOnlineBtn.disabled = isLatestDB;
});

ipcRenderer.on("update-error", (_: any, err: string) => {
    UI.updateStatusMessage(err, "error");
    UI.enableBackBtn("Start from beginning");
    UI.disableProgressBar();
});

ipcRenderer.on("fw-online-update-start", (_: any, updateVer: string, connected: boolean) => {
  UI.showUpdateLoadingScreen("firmware", updateVer, connected);
});

ipcRenderer.on("fw-local-update-start", (_: any, updateVer: string, connected: boolean) => {
  UI.showUpdateLoadingScreen("firmware", updateVer, connected);
});

ipcRenderer.on("db-online-update-start", (_: any, updateVer: string, connected: boolean) => {
  UI.showUpdateLoadingScreen("database", updateVer, connected);
});

ipcRenderer.on("db-local-update-start", (_: any, updateVer: string, connected: boolean) => {
  UI.showUpdateLoadingScreen("database", updateVer, connected);
});

ipcRenderer.on("card-exceptions", function (_ : any, err: any) {
  UI.updateStatusMessage(err, "error");
});

ipcRenderer.on("handle-online-update", function(e: IpcRendererEvent, versionCheck?: latestVersionCheckObj, fwVersion?: string, dbVersion?: string) {
    fwUpdateOnlineBtn.disabled = versionCheck ? versionCheck.isFwLatest : true;
    dbUpdateOnlineBtn.disabled = versionCheck ? versionCheck.isDBLatest : true;
    (dbVersion && fwVersion) ? UI.setOnlineUpdateLabel(dbVersion, fwVersion) : UI.setOnlineUpdateLabel();
});

fwUpdateOnlineBtn.addEventListener("click", (e) => {
  ipcRenderer.send("update-firmware");
  e.preventDefault();
});

fwUpdateLocalBtn.addEventListener("change",  async (e: any) => {
   ipcRenderer.send("update-firmware", await e.target.files[0].arrayBuffer());
   fwUpdateLocalBtn.value = "";
   e.preventDefault();
});

dbUpdateOnlineBtn.addEventListener("click", (e) => {
  ipcRenderer.send("update-erc20");
  e.preventDefault();
});

dbUpdateLocalBtn.addEventListener("change", async (e: any) => {
  ipcRenderer.send("update-erc20", await e.target.files[0].arrayBuffer());
  dbUpdateLocalBtn.value = "";
  e.preventDefault();
});

document.getElementById("shell-message-close")?.addEventListener("click", (e) => {
  UI.hideStatusMessage();
});

window.addEventListener('online', updateOnlineStatus);
window.addEventListener('offline', updateOnlineStatus);

handleUpdateProgress("chunk-loaded");

