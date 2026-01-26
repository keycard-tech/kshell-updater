import { IpcMainEvent, WebContents, ipcMain } from "electron";
import ShellJS from "@choppu/shelljs";
import ShellJSNodeHID from "@choppu/shelljs-node-hid";
import TransportNodeHidSingleton from "@choppu/shelljs-node-hid/lib/transport-node-hid";
import { StatusCodes } from "@choppu/shelljs/lib/errors";
import fetch from 'node-fetch';
import { Utils } from "./utils";

const fwContextPath = "https://shell.keycard.tech/firmware/get-firmware";
const dbContextPath = "https://shell.keycard.tech/update/get-db";
const folderPath = "https://shell.keycard.tech/uploads/";

const fwVersionPosition = 652;
const dbVersionMagic = 0x4532;

export type latestVersionCheckObj = {
    isFwLatest: boolean; 
    isDBLatest: boolean;
}

export class KShell {
  window: WebContents;
  firmware_context?: { fw_path: string, hash: string, version: string } | undefined;
  db_context?: { db_path: string, version: number } | undefined;
  fw?: ArrayBuffer;
  db?: ArrayBuffer;
  deviceFound: boolean;
  versions: latestVersionCheckObj;

  constructor(window: WebContents) {
    this.window = window;
    this.deviceFound = false;
    this.versions = {
      isFwLatest: false, 
      isDBLatest: false
    } as latestVersionCheckObj;
    this.installEventHandlers();
  }

  async start(): Promise<void> {
    try {
        this.firmware_context = await fetch(fwContextPath).then((r: any) => r.json());
        this.db_context = await fetch(dbContextPath).then((r: any) => r.json());
        this.window.send("set-version", this.db_context?.version, this.firmware_context?.version);
    } catch (err) {
      this.window.send("disable-online-update");
    }

    ShellJSNodeHID.TransportNodeHid.default.listen({
      next: async (e) => {
        if (e.type === 'add') {
          this.deviceFound = true;

          let transport = await this.connect();
          let appEth = await new ShellJS.Commands(transport);
          let { fwVersion, dbVersion } = await appEth.getAppConfiguration();
          this.versions = Utils.checkLatestVersion(Utils.parseFirmwareVersion(fwVersion), dbVersion, this.firmware_context, this.db_context);  
          this.window.send("shell-connected", this.deviceFound, this.versions);
          transport.close();
        } else if (e.type === 'remove') {
          this.deviceFound = false;
          this.window.send("shell-disconnected", this.deviceFound);
        }
      },
      error: (error) => {
        if (error instanceof ShellJS.ShellError.TransportOpenUserCancelled) {
          throw ("Error connecting to device. Connect Keycard Shell");
        } else {
          throw ("Error");
        }
      },
      complete: () => { }
    });
  }

  async connect(): Promise<TransportNodeHidSingleton> {
    return ShellJSNodeHID.TransportNodeHid.default.open()
      .then(transport => {
        transport.on("chunk-loaded", (progress: any) => {
          this.window.send("chunk-loaded", progress);
        });
        return transport;
      }).catch((err: any) => {
        console.warn(err);
        return new Promise(s => setTimeout(s, 1000)).then(() => this.connect());
      });
  }

  async updateFirmware(fw?: ArrayBuffer): Promise<void> {
    let localUpdate = false;
    if (fw) {
      this.fw = fw;
      let fwVersionBuff = Buffer.from(fw.slice(fwVersionPosition, fwVersionPosition + 3));
      let fwVersion = `${fwVersionBuff[0]}.${fwVersionBuff[1]}.${fwVersionBuff[2]}`;
      this.window.send("fw-local-update-start", fwVersion, this.deviceFound);
      localUpdate = true;
    } else {
      this.fw = await fetch(folderPath + this.firmware_context?.fw_path).then((r: any) => r.arrayBuffer());
      this.window.send("fw-online-update-start", this.firmware_context?.version, this.deviceFound);
    }
    this.window.send("initialize-update", this.fw?.byteLength);

    if (this.deviceFound) {
      let transport = await this.connect();
      let appEth = await new ShellJS.Commands(transport);

      try {
        let { fwVersion } = await appEth.getAppConfiguration();
        
        if (this.firmware_context && (Utils.parseFirmwareVersion(fwVersion) >= Utils.parseFirmwareVersion(this.firmware_context!.version)) && !localUpdate) {
          this.window.send("no-fw-update-needed");
        } else {
          this.window.send("updating-firmware");
          await appEth.loadFirmware(this.fw as ArrayBuffer);
          this.window.send("firmware-updated");
        }
      } catch (err: any) {
        if (err.statusCode == StatusCodes.SECURITY_STATUS_NOT_SATISFIED) {
          this.window.send("update-error", "Firmware update canceled by user");
        } else {
          this.window.send("update-error", "Invalid data. Update failed.");
        }
      }

      transport.close();
    }
  }

  async handleConnection(connectionStatus: string): Promise<void> {
    if(connectionStatus == "online" && this.deviceFound) {
      let transport = await this.connect();
      let appEth = await new ShellJS.Commands(transport);
      let { fwVersion, dbVersion } = await appEth.getAppConfiguration();
      this.firmware_context = await fetch(fwContextPath).then((r: any) => r.json());
      this.db_context = await fetch(dbContextPath).then((r: any) => r.json());
      this.versions = Utils.checkLatestVersion(Utils.parseFirmwareVersion(fwVersion), dbVersion, this.firmware_context, this.db_context); 
      this.window.send("handle-online-update", this.versions, this.firmware_context?.version, this.db_context?.version);   
    } else {
      this.db_context = undefined;
      this.firmware_context = undefined;
      this.window.send("handle-online-update");
    }
  }

  async updateERC20(db?: ArrayBuffer): Promise<void> {
    let localUpdate = false;
    
    if (db) {
      this.db = db;
      let dbVMagic = Buffer.from(db.slice(0, 2)).readUInt16LE();
      let dbVersion = (dbVMagic != dbVersionMagic) ? '' : `${Buffer.from(db.slice(4, 8)).readUInt32LE().toString()}`;
      this.window.send("db-local-update-start", dbVersion, this.deviceFound);
      localUpdate = true;

      if(dbVMagic != dbVersionMagic) {
        this.window.send("update-error", "Invalid database file");
        return;
      }
    } else {
      this.db = await fetch(folderPath + this.db_context?.db_path).then((r: any) => r.arrayBuffer());
      this.window.send("db-online-update-start", this.db_context?.version, this.deviceFound);
    }

    this.window.send("initialize-update", this.db?.byteLength);

    if (this.deviceFound) {
      let transport = await this.connect();
      let appEth = await new ShellJS.Commands(transport);

      try {
        let { dbVersion } = await appEth.getAppConfiguration();

        if (this.db_context && (dbVersion >= this.db_context!.version) && !localUpdate) {
          this.window.send("no-db-update-needed");
        } else {
          this.window.send("updating-db");
          await appEth.loadDatabase(this.db as ArrayBuffer);
          let { dbVersion } = await appEth.getAppConfiguration();
          this.window.send("db-updated", dbVersion == this.db_context!.version);
        }
      } catch (err: any) {
        if (err.statusCode == StatusCodes.SECURITY_STATUS_NOT_SATISFIED) {
          this.window.send("update-error", "Database update canceled by user");
        } else {
          this.window.send("update-error", "Invalid data. Failed to update the database");
        }
      }

      transport.close();
    }
  }
  

  withErrorHandler(fn: (...args: any) => Promise<void>): (ev: IpcMainEvent) => void {
    return async (_: IpcMainEvent, ...args: any) => {
      try {
        await fn.call(this, ...args);
      } catch (err: any) {
        this.window.send("card-exceptions", err);
      }
    }
  }

  installEventHandlers(): void {
    ipcMain.on("update-firmware", this.withErrorHandler(this.updateFirmware));
    ipcMain.on("update-erc20", this.withErrorHandler(this.updateERC20));
    ipcMain.on("online-status-changed", this.withErrorHandler(async(status: string) => this.handleConnection(status)));
  }
}