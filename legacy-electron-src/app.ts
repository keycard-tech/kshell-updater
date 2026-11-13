import { app, BrowserWindow } from 'electron';
import { Main } from './main';

global.process.env['NODE_TLS_REJECT_UNAUTHORIZED'] = '1';

Main.main(app, BrowserWindow);