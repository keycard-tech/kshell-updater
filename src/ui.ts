const fs = require('fs');
const { shell } = require('electron');

export namespace UI {
  const connectDeviceContainer = document.getElementById("kshell-connect") as HTMLDivElement;
  const updateContainer = document.getElementById("keycard_shell__update-container") as HTMLDivElement;
  const progressBar = document.getElementById("progress-bar-container") as HTMLDivElement;
  const progressLoad = document.getElementById("progress-bar") as HTMLProgressElement;
  const messContainer = document.getElementById("shell-message-field") as HTMLDivElement;
  const message = document.getElementById('shell-message') as HTMLElement;
  const dbVersion = document.getElementById('db-version-label') as HTMLSpanElement;
  const fwVersion = document.getElementById('fw-version-label') as HTMLSpanElement;
  const updateProgressContainer = document.getElementById("update-progress") as HTMLDivElement;
  const updateHeading = document.getElementById("load-update-heading") as HTMLHeadingElement;
  const updateVersionLabel = document.getElementById("version-update-label") as HTMLSpanElement;
  const hideElementClass = "keycard_shell__display-none";
  const backBtn = document.getElementById("btn-back") as HTMLButtonElement;
  const errIcon = document.getElementById("error-image");

  const releaseNotesBtn = document.getElementById("btn-fw-changelog") as HTMLAnchorElement;
  
  const messageColor = '#FF6400';
  const successColor = '#FFFFFFF2';
  const errorColor = '#E95460';

  let pBarProgress: number;

  export function enableBackBtn(text: string) : void {
    backBtn.innerHTML = text;
    backBtn.classList.remove(hideElementClass);
  }

  export function enableProgressBar() : void {
    progressBar.classList.remove(hideElementClass);
  }

  export function setOnlineUpdateLabel(dbVers?: string, fwVers?: string) : void {
    dbVersion.innerHTML = dbVers ? `Database Version ${dbVers}`: 'Online update disabled';
    fwVersion.innerHTML = fwVers ? `Firmware Version ${fwVers}` : 'Online update disabled';
  }

  export function showUpdateLoadingScreen(updateType: string, version: string, connected: boolean) : void {
    updateStatusMessage();
    backBtn.classList.add(hideElementClass);
    if(updateType == "database") {
        updateHeading.innerHTML = 'Databse Update';
        updateVersionLabel.innerHTML = `Database version ${version}`;
    } else {
        updateHeading.innerHTML = 'Firmware Update';
        updateVersionLabel.innerHTML = `Firmware version ${version}`;
    }

    if(connected) {
        updateProgressContainer.classList.remove(hideElementClass);
        updateContainer.classList.add(hideElementClass);
        connectDeviceContainer.classList.add(hideElementClass);
    }

    backBtn.addEventListener("click", (e) => {
        updateProgressContainer.classList.add(hideElementClass);
        updateContainer.classList.remove(hideElementClass);
    });
  }

  export function disableProgressBar() : void {
    progressBar.classList.add(hideElementClass);
  }

  export function initializeProgressBar(l: number) : void {
    progressLoad.max = l;
    progressLoad.value = 0;
    pBarProgress = 0;
  }

  export function handleLoadProgress(progress: number) : boolean {
    if (pBarProgress < progressLoad.max) {
      pBarProgress += progress;
      progressLoad.value = pBarProgress;
      return false;
    }

    return true;
  }

  export function updateStatusMessage(messageText?: string, type?: string): void {
    messContainer.classList.remove(hideElementClass);
    if(messageText && type) {
        message.innerHTML = messageText;
        if(type == "success") {
            message.style.color = successColor;
            errIcon?.classList.add(hideElementClass);
        } else {
            message.style.color = errorColor;
            errIcon?.classList.remove(hideElementClass);
        }
    } else {
        message.innerHTML = "Please do not disconnect device.";
        message.style.color = messageColor;
        errIcon?.classList.add(hideElementClass);
    }
  }

  export function hideStatusMessage() : void {
    messContainer.classList.add(hideElementClass);
  }

  export function handleConnected(connected: boolean) : void {
    if(connected) { 
      handleChangelog();   
      updateContainer.classList.remove(hideElementClass);
      connectDeviceContainer?.classList.add(hideElementClass);
    } else {
      connectDeviceContainer?.classList.remove(hideElementClass);
      !updateContainer.classList.contains(hideElementClass) ? updateContainer.classList.add(hideElementClass) : null;
      !updateProgressContainer.classList.contains(hideElementClass) ? updateProgressContainer.classList.add(hideElementClass) : null;
    }
  }

  export function handleChangelog() : void {
    releaseNotesBtn.addEventListener("click", (e) => {
        e.preventDefault();
        shell.openExternal('https://shell.keycard.tech/firmware/release-notes');
    })
  }
}
