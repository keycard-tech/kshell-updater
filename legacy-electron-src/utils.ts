export namespace Utils {
    export function parseFirmwareVersion(version: string) : number {
        let verArr = Array.from(version.split('.'), Number);
        return (verArr[0] * 1000000) + (verArr[1] * 1000) + verArr[2];
    }

    export function checkLatestVersion(dFWVersion: number, dDBVersion: number, wFWV: any, wDB: any) : {isFwLatest: boolean, isDBLatest: boolean} {
        if(wFWV && wDB) {
            let fwLatest = dFWVersion >= parseFirmwareVersion(wFWV.version);
            let dbLatest = dDBVersion >= wDB.version;
            return {isFwLatest: fwLatest, isDBLatest: dbLatest}
        }

        return {isFwLatest: true, isDBLatest: true}
    }
}