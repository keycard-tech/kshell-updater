import json
import functools
import http.server
import os
import socketserver
import sys
import threading

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FRONTEND = f'{ROOT}/frontend'
# WebKitGTK is the engine the app ships with, so its pictures are the ones in wip/.
OUT = f'{ROOT}/shots/wip'
FIXTURES = json.load(open(f'{ROOT}/tests/fixtures/releases.json'))
# Written by `cargo run --example units`; a hand-drawn square here would hide a QR that WebKitGTK
# refuses to paint, which is the one thing about it worth looking at.
QR_SVG = open(f'{ROOT}/tests/fixtures/challenge.svg').read()
WINDOW = json.load(open(f'{ROOT}/src-tauri/tauri.conf.json'))['app']['windows'][0]
JPEG = ('/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAA0JCgsKCA0LCgsODg0PEyAVExISEyccHhcgLikxMC4pLSwzOko'
        '+MzZGNywtQFdBRkxOUlNSMj5aYVpQYEpRUk//2wBDAQ4ODhMREyYVFSZPNS01T09PT09PT09PT09PT09PT09P'
        'T09PT09PT09PT09PT09PT09PT09PT09PT09PT09PT0//wAARCAACAAIDASIAAhEBAxEB/8QAFQABAQAAAAAA'
        'AAAAAAAAAAAb/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAwT/xAAUEQEAAAAA'
        'AAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCTAOlf/9k=')
SCALE = float(open(f'{FRONTEND}/css/tokens.css').read().split('--scale:')[1].split(';')[0])

_served = None


def base_uri():
    """ES modules are refused over file://, so the frontend is served from a loopback port."""
    global _served
    if _served is None:
        class Quiet(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args):
                pass

        server = socketserver.ThreadingTCPServer(('127.0.0.1', 0), functools.partial(Quiet, directory=FRONTEND))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        _served = f'http://127.0.0.1:{server.server_address[1]}/index.html'
    return _served


SERIAL = '320028000151333136343433000802'
FW = FIXTURES['firmware']['files'][0]
DB = FIXTURES['database']['files'][0]
DEVICE = {'productName': 'Keycard Shell', 'path': '/dev/hidraw0', 'serialNumber': SERIAL}
BEHIND = {
    'fwVersion': '1.5.2',
    'dbVersion': int(DB['date']) - 30,
    'serialNumber': SERIAL,
    'publicKey': '03cf27be82a236298b017642eac35cc058fe8cef21979f8bc332f8a4d41a97605c',
    'fwVariant': FW['variant'],
    'fwBeta': False,
}
CURRENT = dict(BEHIND, fwVersion=FW['version'], dbVersion=int(DB['date']))
BETA = dict(BEHIND, fwVersion='1.6.0', fwBeta=True)
AHEAD = dict(BEHIND, fwVersion='1.6.0', dbVersion=int(DB['date']))
CONFIGS = {'current': CURRENT, 'beta': BETA, 'ahead': AHEAD}

MEASURE = """JSON.stringify((() => {
  const shown = (n) => !n.hidden && getComputedStyle(n).display !== 'none';
  const boxes = [...document.querySelector('.ksh').children].filter(shown);
  const rect = (sel) => { const n = document.querySelector(sel); return n && shown(n) ? n.getBoundingClientRect() : null; };
  const rows = rect('.ksh__rows');
  const chips = rect('.ksh__next');
  const rowCards = [...document.querySelectorAll('.ksh__row')].map(n => n.getBoundingClientRect());
  const overlaps = [];
  const all = [['rows', rows], ['chips', chips], ['footer', rect('.ksh__footer')]]
    .filter(([, r]) => r)
    .concat(rowCards.map((r, i) => ['row' + i, r]))
    .concat([['device', rect('.ksh__device')]]);
  const nested = (a, b) => a.contains(b) || b.contains(a);
  const node = { rows: document.querySelector('.ksh__rows'), chips: document.querySelector('.ksh__next'),
    footer: document.querySelector('.ksh__footer'), device: document.querySelector('.ksh__device') };
  [...document.querySelectorAll('.ksh__row')].forEach((n, i) => { node['row' + i] = n; });
  for (const [a, ra] of all) {
    for (const [b, rb] of all) {
      if (a >= b || nested(node[a], node[b])) continue;
      const vertical = Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top);
      const horizontal = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left);
      if (vertical > 1 && horizontal > 1) overlaps.push(`${a}/${b}`);
    }
  }
  // Everything the visible screen stacks, one and two levels deep: the verify column nests, and an
  // overflow there paints onto the intro above and the status bar below, which is how a two-line
  // failure message ended up on top of step 1 in a screenshot from the running app.
  const shownScreen = [...document.querySelector('.ksh').children]
    .find((n) => n.id.startsWith('screen-') && shown(n));
  const stack = shownScreen
    ? [...shownScreen.children, ...[...shownScreen.children].flatMap((n) => [...n.children])]
      .filter((n) => shown(n) && n.getBoundingClientRect().height > 1)
    : [];
  const stackOverlaps = [];
  for (const [i, a] of stack.entries()) {
    for (const [j, b] of stack.entries()) {
      if (j <= i || a.contains(b) || b.contains(a)) continue;
      const ra = a.getBoundingClientRect();
      const rb = b.getBoundingClientRect();
      if (Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top) > 1
        && Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left) > 1) {
        stackOverlaps.push(`${a.id || a.className} / ${b.id || b.className}`);
      }
    }
  }
  const outsideWindow = stack
    .filter((n) => {
      const r = n.getBoundingClientRect();
      return r.bottom > document.documentElement.clientHeight + 1
        || r.right > document.documentElement.clientWidth + 1;
    })
    .map((n) => n.id || n.className);
  const check = document.querySelector('.ksh__row input[type=checkbox]');
  const tile = rect('.ksh__qr-tile') || rect('.ksh__video');
  const frame = document.documentElement;
  const title = document.querySelector('.ksh__row .ksh__h2');
  const icon = document.querySelector('.ksh__device-icon');
  const heading = document.querySelector('.ksh__device .ksh__h1');
  const centre = (n) => (n ? +(n.getBoundingClientRect().top + n.getBoundingClientRect().bottom).toFixed(1) / 2 : null);
  return {
    // Measuring a screen that is not the one being tested proves nothing, and a passing line here
    // once did exactly that: the camera click had not landed and the scan png showed the challenge.
    // What the screens actually say. A node that a simplification deleted reports null, so a state
    // that expects a line cannot pass because the line was quietly taken away.
    words: Object.fromEntries(['home-sub', 'status-note', 'status-net-word', 'failed-title',
                               'failed-detail', 'verdict-sub',
                               'verify-qr-note', 'scan-problem', 'transfer-bytes', 'transfer-step',
                               'verify-list-line', 'device-line', 'ag-guide', 'ag-title'].map((id) => {
      const n = document.getElementById(id);
      return [id, !n ? null : n.hidden ? '' : (n.textContent || '').trim()];
    }).concat([['pickLabel',
      (document.querySelector('[data-kind=firmware] [data-slot=pick]') || {}).textContent || null],
      ['rowHint', (document.querySelector('[data-kind=firmware] [data-slot=hint]') || {}).textContent || null],
      ['rowStatus', (document.querySelector('[data-kind=firmware] [data-slot=status]') || {}).textContent || null],
      ['rowSha', (document.querySelector('[data-kind=firmware] [data-slot=sha]') || {}).textContent || null],
      ['rowVersions',
       [...document.querySelectorAll('[data-kind=firmware] option')].map((o) => o.textContent.trim()).join(' / ') || null],
      ['databaseChoice',
       (document.querySelector('[data-kind=database] [data-slot=static]') || {}).textContent || null],
      // Which air-gapped affordances are up: counted, so a row that quietly keeps its button after
      // a file resolves cannot pass.
      ['codeBtns', String([...document.querySelectorAll('.ksh__row .ksh__agbtn')].filter(shown).length)],
      ['unitsCode', shown(document.getElementById('btn-units-code') || { hidden: true }) ? 'shown' : ''],
      ['agModal', shown(document.getElementById('ag-modal') || { hidden: true }) ? 'shown' : ''],
      ['homeAirgap', shown(document.getElementById('home-airgap') || { hidden: true }) ? 'shown' : ''],
      // How many times a code was minted, and how many cameras the list offers after repeats of the
      // same device are dropped: both were runaway numbers no screenshot showed.
      ['challengeMints', String(window.__challengeMints || 0)],
      ['challengeText', String(window.kshell.verify.challengeText || '')],
      ['cameraCount', String(document.querySelectorAll('#scan-camera option').length)],
      ['cameraChoice', document.querySelector('#scan-camera option:checked')?.textContent.trim() ?? ''],
      ['transferButton', ((n) => n
        ? `${(n.textContent || '').trim()}${n.disabled ? ' \u00b7 disabled' : ' \u00b7 enabled'}`
        : null)(document.getElementById('btn-transfer'))],
      // An offline machine has no online source, so the online half of the list switch is out.
      ['onlineSeg', ((n) => n
        ? `${(n.textContent || '').trim()}${n.disabled ? ' \u00b7 disabled' : ' \u00b7 enabled'}`
        : null)(document.querySelector('[data-kind=firmware] [data-mode=online]'))],
      ['unitsSeg', ((n) => n
        ? `${(n.textContent || '').trim()}${n.disabled ? ' \u00b7 disabled' : ' \u00b7 enabled'}`
        : null)(document.getElementById('verify-list-online'))],
      ['cancelButton', ((n) => n
        ? `${(n.textContent || '').trim()}${n.disabled ? ' \u00b7 disabled' : ' \u00b7 enabled'}`
        : null)(document.getElementById('btn-cancel'))],
      ['cameraButton', ((n) => n
        ? `${(n.textContent || '').trim()}${n.disabled ? ' \u00b7 disabled' : ''}`
        : null)(document.getElementById('btn-camera'))],
      // Which note the app is showing: a stuck verify screen says why in this box, and so does a
      // bridge double that throws - which is how every verify screen once showed a ReferenceError
      // while the harness printed ok for all of them.
      ['note', (document.getElementById('verify-note-text') || {}).textContent || ''],
      ['homeTitle', (document.querySelector('#card-update .ksh__h1') || {}).textContent || ''],
      // The panel's picture is chosen once, in the markup: it used to change with whether a Shell was
      // plugged in, which made the landing card two different screens.
      ['homeImg', (document.getElementById('home-img') || {getAttribute: () => ''})
        .getAttribute('src').split('/').pop()],
      // A message that is laid out at zero height says nothing to anyone: the status bar was once
      // hidden on the very screen whose list error it carried.
      ['noteShown', (() => {
        const note = document.getElementById('status-note');
        if (!note || note.hidden || !(note.textContent || '').trim()) return 'empty';
        const r = note.getBoundingClientRect();
        return r.height > 0 && r.width > 0 && r.bottom <= innerHeight ? 'shown' : 'invisible';
      })()],
      // The offline line shares the strip with a serial number forty-odd characters long, so being
      // in the document is not enough: it has to still fit beside it.
      ['netShown', (() => {
        const net = document.getElementById('status-net');
        if (!net || net.hidden || !(net.textContent || '').trim()) return 'empty';
        const r = net.getBoundingClientRect();
        return r.height > 0 && r.width > 0 && r.bottom <= innerHeight && r.right <= innerWidth
          ? 'shown' : 'invisible';
      })()],
      // The list switch appears on both flows and has to read as one control. Computed styles are
      // measured rather than the boxes, because the hidden screen reports no box at all.
      ['segMatch', (() => {
        const style = (n) => {
          if (!n) return 'gone';
          const c = getComputedStyle(n);
          const held = getComputedStyle(n.parentElement);
          return [c.fontSize, c.lineHeight, c.padding, c.fontWeight,
                  `group:${held.padding} ${held.gap}`].join(' ');
        };
        const verify = style(document.getElementById('verify-list-online'));
        const update = style(document.querySelector('[data-mode=online]'));
        return verify === update ? 'identical' : `verify [${verify}] update [${update}]`;
      })()],
      // The three flow titles are the same role on three screens whose --scale differs (update 1.2,
      // verify 0.95, transfer 0.78), which is how they drifted apart once before.
      ['titleMatch', (() => {
        const size = (id) => {
          const node = document.getElementById(id);
          const head = node && node.tagName === 'H1' ? node : node && node.querySelector('h1');
          return head ? getComputedStyle(head).fontSize : 'gone';
        };
        const found = ['device-name', 'screen-verify', 'transfer-title'].map(size);
        return found[0] === found[1] && found[0] === found[2]
          ? 'identical'
          : found.join(' vs ');
      })()]])
      // A control's label is what the person reads; `hidden` counts as saying nothing.
      .concat(['btn-verify', 'btn-ready-verify', 'btn-camera', 'verify-list-file',
               'btn-retry', 'btn-failed-back', 'btn-verify-update',
               'btn-verdict', 'btn-verdict-guide'].map((id) => {
        const n = document.getElementById(id);
        return ['button:' + id, !n ? null : n.hidden ? '' : (n.textContent || '').trim()];
      }))),
    errors: window.__errors__ ?? null,
    screen: [...document.querySelector('.ksh').children]
      .filter((n) => n.id.startsWith('screen-') && shown(n)).map((n) => n.id).join('+'),
    contentBottom: Math.ceil(Math.max(...boxes.map(n => n.getBoundingClientRect().bottom))),
    contentRight: Math.ceil(Math.max(...boxes.map(n => n.getBoundingClientRect().right))),
    // What `.ksh` would need with nothing scrolled: a window shorter than this means a scrollbar.
    natural: Math.ceil(document.querySelector('.ksh').scrollHeight),
    overflowY: document.querySelector('.ksh').scrollHeight - document.querySelector('.ksh').clientHeight,
    overlaps,
    stackOverlaps,
    outsideWindow,
    stackRects: stack.map((n) => {
      const r = n.getBoundingClientRect();
      return `${n.id || n.className || n.tagName}: ${Math.round(r.top)}..${Math.round(r.bottom)}`;
    }),
    // The challenge and the preview are square by design: a stretched QR is a QR nobody reads.
    tileWidth: tile ? Math.round(tile.width) : null,
    tileHeight: tile ? Math.round(tile.height) : null,
    tileOutside: tile ? (tile.bottom > frame.clientHeight + 1 || tile.right > frame.clientWidth + 1) : false,
    checkboxCentre: centre(check),
    titleGlyphCentre: centre(title),
    iconCentre: centre(icon),
    headingCentre: centre(heading),
    debug: {
      titleHeight: title ? +title.getBoundingClientRect().height.toFixed(1) : null,
      titleLineHeight: title ? getComputedStyle(title).lineHeight : null,
      checkboxHeight: check ? +check.getBoundingClientRect().height.toFixed(1) : null,
      checkboxTop: check ? +check.getBoundingClientRect().top.toFixed(1) : null,
      titleTop: title ? +title.getBoundingClientRect().top.toFixed(1) : null,
      rowTopAlign: getComputedStyle(document.querySelector('.ksh__row-top')).alignItems,
    },
  };
})())"""

# name -> (bridge settings, what to click after load, ms to wait before the snapshot)
STATES = {
    # (what the bridge double answers, how to get there, which screen must be visible)
    # The markup leaves home-sub to the app, so this can only be satisfied by a rendered hint: the
    # sentence lives in strings.ts and nowhere else.
    'home': ({'noDevice': True}, [], 'screen-home',
             {'home-sub': 'unavailable until the Shell', 'homeTitle': 'Update Shell',
              'segMatch': 'identical', 'titleMatch': 'identical',
              'homeImg': 'panel_shell.png'}),
    'ready': ({}, ["document.getElementById('card-update').click()"], 'screen-ready',
               {'homeImg': 'panel_shell.png', 'onlineSeg': 'Online · enabled',
                'transfer-bytes': None, 'transfer-step': None, 'rowSha': 'sha256',
                'device-line': 'Installed on Shell · firmware', 'rowVersions': 'omni',
                'databaseChoice': '20261081'}),
    'ready-up-to-date': ({'config': 'current'},
                         ["document.getElementById('card-update').click()"], 'screen-ready',
                         {'device-line': 'Installed on Shell · firmware 1.5.3'}),
    # A Shell running ahead of every published build has nothing to take: the public release is not
    # offered as if it were newer, which is also the write the Shell would refuse.
    'ready-ahead': ({'config': 'ahead'},
                    ["document.getElementById('card-update').click()"], 'screen-ready',
                    {'device-line': 'Installed on Shell · firmware 1.6.0',
                     'transferButton': 'Transfer · disabled'}),
    # A beta Shell is offered the public release by the same rule as the web updater, so the write has
    # to reach the Shell instead of coming back as a refusal nobody can read.
    'beta-to-release': ({'config': 'beta', 'flash': 'ok'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-done'),
    'ready-list-error': ({'listError': True},
                         ["document.getElementById('card-update').click()"], 'screen-ready',
                         {'status-note': 'No connection - try again when you are online',
                          'noteShown': 'shown',
                          'codeBtns': '2',
                          'rowHint': ''}),
    # The connection going away is its own line in the strip, so the list sentence next to it has
    # nothing left to repeat, and the rows are left holding whatever was read while it worked.
    'home-offline': ({}, ["window.__net(false);"], 'screen-home',
                      {'status-net-word': 'Offline - no connection', 'netShown': 'shown',
                       'status-note': '', 'homeAirgap': 'shown'}),
    # Offline before the page asks, which is the other way in: `network_status` at load rather than
    # an event, and every read the app starts with failing as it would on a machine with no route.
    'home-starts-offline': ({'online': False}, [], 'screen-home',
                            {'status-net-word': 'Offline - no connection', 'netShown': 'shown',
                             'status-note': ''}),
    'offline-recovered': ({}, ["window.__net(false);", "window.__net(true);"], 'screen-home',
                          {'status-net-word': '', 'status-note': ''}),
    # Offline is not "the list on screen, read earlier": the online source is closed, its list and
    # its files gone with it, and what is left to offer is a file and a download code.
    'ready-offline': ({}, [
        "document.getElementById('card-update').click()",
        "window.__net(false);",
    ], 'screen-ready', {
        'status-net-word': 'Offline - no connection', 'netShown': 'shown', 'status-note': '',
        'onlineSeg': 'Online · disabled', 'codeBtns': '2',
        'rowStatus': 'No list', 'transferButton': 'Transfer · disabled',
    }),
    # A write to the Shell travels over USB, so the network leaving mid-transfer must not disturb it:
    # the queue, its labels and the cancel button are exactly as they were.
    'transfer-offline': ({'flash': 'pending'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
        "window.__net(false);",
    ], 'screen-transfer', {
        'cancelButton': 'Cancel · enabled',
        # The strip is hidden on this screen, which is also the answer for the offline line: nothing
        # about the network has a place to appear while a write is running.
        'netShown': 'invisible',
    }),
    # A units list already in hand is what makes the challenge scannable, and the check is local: the
    # connection going away stops a re-fetch, it does not end the verification under way.
    'verify-offline-midway': ({}, [
        "document.getElementById('card-verify').click()",
        "window.__net(false);",
    ], 'screen-verify', {
        'status-net-word': 'Offline - no connection', 'cameraButton': 'Enable camera',
        'verify-qr-note': '', 'verify-list-line': 'Units list fetched 2026-11-13',
    }),
    'verify-goes-offline': ({}, [
        "document.getElementById('card-verify').click()",
        "window.__net(false);",
    ], 'screen-verify', {'unitsSeg': 'Fetched list \u00b7 disabled'}),
    'verify-comes-back': ({}, [
        "document.getElementById('card-verify').click()",
        "window.__net(false);",
        "window.__net(true);",
    ], 'screen-verify', {'unitsSeg': 'Fetched list \u00b7 enabled'}),
    # The badge describes the pass that failed, not the rest of the session: once the network is back
    # the app lists again and the status bar has to go quiet.
    'list-error-recovered': ({'listError': True}, [
        "window.__backOnline = true;",
        "window.__net(false);",
        "window.__net(true);",
    ], 'screen-home', {'status-net-word': '', 'status-note': ''}),
    'list-newest-unhashed': ({'newestOffered': False},
                             ["document.getElementById('card-update').click()"], 'screen-ready',
                             {'status-note': 'publishes no hash', 'noteShown': 'shown'}),
    # The guide belongs to the card, not to the network: whatever the hint above it says, and
    # online or offline, the way to the air-gapped instructions stays on screen.
    'home-airgapped-link': ({}, [], 'screen-home',
                            {'ag-guide': 'Read the guide', 'homeAirgap': 'shown'}),
    'home-airgapped-offline': ({'online': False}, [], 'screen-home', {'homeAirgap': 'shown'}),
    'ready-airgapped-codes': ({'listError': True},
                              ["document.getElementById('card-update').click()"], 'screen-ready',
                              {'codeBtns': '2', 'status-note': 'No connection - try again when you are online'}),
    'ready-airgapped-code-open': ({'listError': True}, [
        "document.getElementById('card-update').click()",
        "document.querySelector('[data-kind=firmware] [data-slot=code]').click()",
    ], 'screen-ready', {'agModal': 'shown', 'ag-title': 'Download the latest firmware'}),
    'verify-airgapped-codes': ({'units': 'none'},
                               ["document.getElementById('card-verify').click()"], 'screen-verify',
                               {'unitsCode': 'shown', 'cameraButton': 'Enable camera · disabled',
                                'verify-qr-note': 'No units list available'}),
    'verify-airgapped-code-open': ({'units': 'none'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-units-code').click()",
    ], 'screen-verify', {'agModal': 'shown', 'ag-title': 'Download the units list'}),
    'transfer': ({'flash': 'pending'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-transfer', {'transfer-bytes': None, 'transfer-step': None}),
    'ready-checksum-mismatch': ({'preloadFail': True},
                                ["document.getElementById('card-update').click()"], 'screen-ready'),
    'preload-retried': ({'preloadFail': 'once'}, [
        "document.getElementById('card-update').click()",
        "window.__net(false);",
        "window.__net(true);",
    ], 'screen-ready', {'transferButton': 'Transfer to device · enabled'}),
    'verify': ({}, ["document.getElementById('card-verify').click()"], 'screen-verify',
               {'verify-qr-note': '',
                'verify-list-line': 'Units list fetched 2026-11-13',
                'challengeMints': '1', 'note': '',
                'button:btn-verify-update': 'Update Shell', 'cameraButton': 'Enable camera',
                'challengeText': 'UR:DEV-AUTH/oeadadamhdcx0'}),
    'verify-cached-list': ({'units': 'cached'},
                           ["document.getElementById('card-verify').click()"], 'screen-verify',
                           {'note': ''}),
    # A list loaded from disk: named on the line, and enough to unblur the challenge.
    'verify-own-list': ({'units': 'file'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('verify-list-file').click()",
    ], 'screen-verify', {
        'verify-qr-note': '',
        'verify-list-line': 'Units list from units-2026-11-12.bin',
        'challengeMints': '1', 'note': '',
        'challengeText': 'UR:DEV-AUTH/oeadadamhdcx0',
    }),
    'verify-no-list': ({'units': 'none'},
                       ["document.getElementById('card-verify').click()"], 'screen-verify',
                       {'verify-qr-note': 'No units list available',
                        'verify-list-line': 'Load own list to verify offline.',
                        # The camera button is the camera in every state: until a list is in it
                        # waits, it does not double as a fetch.
                        'cameraButton': 'Enable camera · disabled'}),
    # The network coming back is the retry: the stuck screen fetches again by itself.
    'verify-fetch-after-online': ({'units': 'none'}, [
        "document.getElementById('card-verify').click()",
        "window.__backOnline = true;",
        "window.__net(false);",
        "window.__net(true);",
    ], 'screen-verify', {
        'cameraButton': 'Enable camera', 'unitsCode': '',
        'verify-qr-note': '', 'verify-list-line': 'Units list fetched 2026-11-13',
    }),
    # The first paint of a slow fetch, which is what a white empty tile used to look like.
    # A failure message is however long the Rust is long, and this screen must not be sized for its
    # tidiest state: the one that started this was a two-line `state not managed` complaint.
    'verify-no-list-long': ({'units': 'none', 'longReason': True},
                            ["document.getElementById('card-verify').click()"], 'screen-verify'),
   'verify-refused-file-keeps-way-back': ({'fileFails': True}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('verify-list-file').click()",
    ], 'screen-verify', {
        'unitsSeg': 'Fetched list \u00b7 enabled',
        'verify-qr-note': 'No units list available',
        'note': 'That file is not something the app can use - choose another one',
    }),
    'verify-refused-file-back-to-fetched': ({'fileFails': True}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('verify-list-file').click()",
        "document.getElementById('verify-list-online').click()",
    ], 'screen-verify', {
        'verify-list-line': 'Units list fetched 2026-11-13',
        'verify-qr-note': '', 'cameraButton': 'Enable camera', 'note': '',
        'unitsSeg': 'Fetched list \u00b7 enabled',
    }),
    'verify-own-list-refused': ({'fileFails': True}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('verify-list-file').click()",
    ], 'screen-verify', {
        'verify-qr-note': 'No units list available',
        'button:verify-list-file': 'From file',
    }),
    'verify-fetching':  ({'units': 'none', 'fetchHangs': True},
                        ["document.getElementById('card-verify').click()"], 'screen-verify',
                        {'note': ''}),
    'verify-scan': ({'scan': 'ok'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
    ], 'screen-scan', {'cameraCount': '2', 'cameraChoice': 'External Camera', 'scan-problem': ''}),
    # The pick follows the answer rather than the click: choosing the integrated camera when the Rust
    # says the external one is running must leave the external one named. It once bounced to the first
    # entry on every switch, and the replaced loop kept pulling frames beside the one replacing it.
    'scan-switched': ({'scan': 'ok'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
        "(() => { const pick = document.getElementById('scan-camera'); pick.value = '0';"
        " pick.dispatchEvent(new Event('change')); })()",
    ], 'screen-scan', {'cameraCount': '2', 'cameraChoice': 'External Camera', 'scan-problem': ''}),
    # The camera is asked for in the Rust, so these are the answers the harness can give it: none, one
    # held by another app, one that opened and sent nothing. The first two are said on the challenge
    # screen, where the press happened; the last on the scanning screen, where the picture is missing.
    'scan-none': ({'scan': 'none'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
    ], 'screen-verify', {'note': 'No camera is connected'}),
    'scan-busy': ({'scan': 'busy'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
    ], 'screen-verify', {'note': 'Another app is using the camera'}),
    'scan-denied': ({'scan': 'denied'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
    ], 'screen-verify', {'note': 'Camera access is off for this app'}),
    'scan-silent': ({'scan': 'silent'}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-camera').click()",
    ], 'screen-scan', {'scan-problem': 'sent no picture'}),
    'verdict-authentic': ({'verdict': 'authentic'}, [
        "document.getElementById('card-verify').click()",
        "window.kshell.scanAnswer('UR:DEV-AUTH/oeadadamhdcx')",
    ], 'screen-verdict',
        {'button:btn-verdict': 'Done', 'button:btn-verdict-guide': ''}),
    # A refused answer ends the session, and trying again has to be a new code: the Shell would
    # otherwise be asked to sign against one it already answered.
    'verify-again': ({'verdict': 'refused'}, [
        "document.getElementById('card-verify').click()",
        "window.kshell.scanAnswer('UR:DEV-AUTH/not-bytewords')",
        "document.getElementById('btn-verdict').click()",
    ], 'screen-verify', {'challengeMints': '2',
                          'challengeText': 'UR:DEV-AUTH/oeadadamhdcx1'}),
    # Leaving and returning without finishing is the same session: the code on the screen must stay
    # the code the Shell was shown, so nothing may be minted here.
    'verify-reentered': ({}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-verify-update').click()",
        "document.getElementById('card-verify').click()",
    ], 'screen-verify', {'challengeMints': '1', 'note': '',
                          'challengeText': 'UR:DEV-AUTH/oeadadamhdcx0'}),
    # A Shell that is not in the list is a dead end inside the app, so the way out is the button and
    # the guide is the quiet link under it - the person has to be able to put the app down.
    'verdict-not-registered': ({'verdict': 'notRegistered'}, [
        "document.getElementById('card-verify').click()",
        "window.kshell.scanAnswer('UR:DEV-AUTH/oeadadamhdcx')",
    ], 'screen-verdict',
        {'button:btn-verdict': 'Back to Home', 'button:btn-verdict-guide': 'Learn more'}),
    'verdict-not-registered-home': ({'verdict': 'notRegistered'}, [
        "document.getElementById('card-verify').click()",
        "window.kshell.scanAnswer('UR:DEV-AUTH/oeadadamhdcx')",
        "document.getElementById('btn-verdict').click()",
    ], 'screen-home'),
    # With no Shell plugged in the verify screen has nothing to update, and saying otherwise would
    # send the person to a screen that cannot do the thing its button promises.
    'verify-no-device': ({'noDevice': True}, [
        "document.getElementById('card-verify').click()",
    ], 'screen-verify', {'button:btn-verify-update': 'Back to Home'}),
    'verify-no-device-home': ({'noDevice': True}, [
        "document.getElementById('card-verify').click()",
        "document.getElementById('btn-verify-update').click()",
    ], 'screen-home'),
    'verdict-refused': ({'verdict': 'refused'}, [
        "document.getElementById('card-verify').click()",
        "window.kshell.scanAnswer('UR:DEV-AUTH/not-bytewords')",
    ], 'screen-verdict',
        {'button:btn-verdict': 'Try again', 'button:btn-verdict-guide': ''},
        # The refusal behind this is a bytewords decode failure naming bytes; what is said is that the
        # code did not check out.
        {'verdict-sub': 'That answer does not match this code - scan it again'}),
    'complete': ({'flash': 'ok'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-done'),
    # A file from disk: the row names the file it would write, in the box that asked for it.
    'ready-own-file': ({'ownFile': True}, [
        "document.getElementById('card-update').click()",
        "document.querySelector('[data-kind=firmware] [data-mode=file]').click()",
        "document.querySelector('[data-kind=firmware] [data-slot=pick]').click()",
    ], 'screen-ready', {
        'pickLabel': 'shellos-20261112-1.5.3.bin',
        'rowSha': 'sha256',
    }),
    # An older .bin from disk is refused in words, on the row, before the Shell is asked.
    'ready-own-file-old': ({'ownFile': True, 'ownFileOld': True}, [
        "document.getElementById('card-update').click()",
        "document.querySelector('[data-kind=firmware] [data-mode=file]').click()",
        "document.querySelector('[data-kind=firmware] [data-slot=pick]').click()",
    ], 'screen-ready', {'rowHint': 'older than the firmware 1.5.2 on your Shell'}),
    'ready-own-file-variant': ({'ownFile': True, 'ownFileVariant': 1, 'flash': 'ok'}, [
        "document.getElementById('card-update').click()",
        "document.querySelector('[data-kind=firmware] [data-mode=file]').click()",
        "document.querySelector('[data-kind=firmware] [data-slot=pick]').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-done'),
    'failed': ({'flash': 'error'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-failed', {
        'failed-title': 'The Shell was unplugged mid-write.',
        'failed-detail': 'Nothing was written',
        'button:btn-retry': 'Try again',
        'button:btn-failed-back': 'Back to updates',
    }),
    # A cancelled write is not a broken Shell: the screen says so, and both ways out are there.
    'cancelled': ({'flash': 'cancelled'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-failed', {
        'failed-title': 'The transfer was cancelled.',
        'button:btn-failed-back': 'Back to updates',
    }),
    'cancel-again': ({'flash': 'cancelled', 'cancelAgain': True}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
        "document.getElementById('btn-cancel').click()",
        "document.getElementById('btn-failed-back').click()",
        "document.getElementById('btn-transfer').click()",
    ], 'screen-transfer', {'cancelButton': 'Cancel · enabled'}),
    'failed-back': ({'flash': 'error'}, [
        "document.getElementById('card-update').click()",
        "document.getElementById('btn-transfer').click()",
        "document.getElementById('btn-failed-back').click()",
    ], 'screen-ready', {
        'rowSha': 'sha256',
        'device-line': 'Installed on Shell · firmware',
    }),
}


def settle(options):
    return 500 if options.get('scan') or options.get('flash') else 300


def stub(name):
    options = STATES[name][0]
    config = None if options.get('noDevice') else CONFIGS.get(options.get('config'), BEHIND)
    bridge = {
        'preloadFail': options.get('preloadFail', False),
        'devices': [] if options.get('noDevice') else [DEVICE],
        'config': config,
        'fixtures': FIXTURES,
        'imageInfo': FIXTURES['imageInfo'],
        'flash': options.get('flash'),
        'configAfterFlash': CURRENT,
        # The longest thing the panel hint has to hold: what a proxy that is not there costs, with
        # every layer of the error chain, as the Rust now writes it.
        'listError': (
            'could not reach https://api.github.com/repos/keycard-tech/keycard-shell/releases'
            '?per_page=100: client error (Connect): tunnel error: failed to create underlying '
            'connection: tcp connect error: Connection refused (os error 111)'
            if options.get('listError')
            else None
        ),
        'units': options.get('units', 'online'),
        'fetchHangs': bool(options.get('fetchHangs')),
        'longReason': bool(options.get('longReason')),
        'verdict': options.get('verdict'),
        'scan': options.get('scan'),
        # Two pixels of picture: enough for the canvas to have been painted without the harness
        # carrying a webcam it cannot have.
        'jpeg': JPEG,
        'ownFile': bool(options.get('ownFile')),
        'fileFails': bool(options.get('fileFails')),
        'ownFileOld': bool(options.get('ownFileOld')),
        'ownFileVariant': options.get('ownFileVariant'),
        'newestOffered': options.get('newestOffered', True),
        'cancelAgain': bool(options.get('cancelAgain')),
        # What the app is told about the network when it asks at load. A state that starts with no
        # connection says so here, and the reads that need one answer as the Rust would answer.
        'online': options.get('online', True),
        'qr': QR_SVG,
    }
    template = """
      const b = __BRIDGE__;
      // A rejected promise nobody listens to is how this harness once reported 'ok' for a screen that
      // never opened; the messages are collected and printed with the measurements.
      window.__errors__ = [];
      addEventListener('error', (e) => window.__errors__.push(String(e.message)));
      addEventListener('unhandledrejection', (e) => window.__errors__.push(String(e.reason)));
      // The real bridge rejects with the bare string, so the view's String(error) reads the same.
      // The bare sentence a request that got no answer arrives as, so a screen with no connection is
      // driven with what the Rust would hand the page rather than with a tidy placeholder.
      const NO_CONNECTION = 'could not reach https://api.github.com/repos/keycard-tech/keycard-shell'
        + '/releases?per_page=100: client error (Connect): dns error: failed to lookup address '
        + 'information: Temporary failure in name resolution';
      const fail = (m) => Promise.reject(m);
      window.__events__ = {};
      // A state announces the network the only way the app can hear it: the event the Rust emits when
      // its answer about github.com changes.
      window.__net = (online) => window.__events__['net://reachable']({ payload: online });
      window.__TAURI__ = {
        core: { invoke: (command, args) => {
          switch (command) {
            case 'network_status': return Promise.resolve(b.online);
            case 'list_shells': return Promise.resolve(b.devices);
            // After a flash the Shell answers with what it now runs, and the view asks again to be sure.
            case 'shell_app_configuration': return Promise.resolve(b.flashedConfig ?? b.config);
            case 'preload_images':
              return Promise.resolve(args.files.map((file) => {
                const one = b.fixtures[file.kind].files.find((f) => f.url === file.url);
                if (!one) return null;
                // The longest message the update screen ever shows: a published hash the download
                // does not match. The window is sized for this, not for the tidy case.
                if (b.preloadFail === true || (b.preloadFail === 'once' && !window.__preloaded)) {
                  window.__preloaded = true;
                  if (file.kind === 'firmware') {
                    window.__events__['shell://preloaded']({
                      payload: { key: file.url, ok: false,
                        note: one.name + ' did not match its published checksum' } });
                    return null;
                  }
                }
                return {
                  info: { ...b.imageInfo, kind: file.kind, version: one.version ?? one.date,
                          fwVariant: one.variant, sha256: one.sha256,
                          bytes: file.kind === 'firmware' ? 622592 : 412672,
                          matchesRelease: one.name },
                  key: file.url,
                };
              }).filter(Boolean));
            case 'flash_image': {
              const total = args.request.kind === 'firmware' ? 622592 : 412672;
              const emit = window.__events__['shell://flash-progress'];
              emit({ payload: { kind: args.request.kind, done: Math.round(total * 0.42), total } });
              if (b.flash === 'pending') return new Promise(() => {});
              // A Rust error arrives as a sentence, first one the headline of the failure screen.
              if (b.flash === 'cancelled') {
                window.__flashTries = (window.__flashTries || 0) + 1;
                if (b.cancelAgain && window.__flashTries > 1) return new Promise(() => {});
                return new Promise((_, reject) => setTimeout(
                  () => reject('The transfer was cancelled. Nothing more was written.'), 120));
              }
              if (b.flash === 'error') {
                return new Promise((_, reject) => setTimeout(
                  () => reject('The Shell was unplugged mid-write. Nothing was written, so the '
                               + 'update can be started again.'), 120));
              }
              return new Promise((resolve) => setTimeout(() => {
                b.flashedConfig = { ...(b.flashedConfig ?? b.config), ...b.configAfterFlash };
                resolve(b.flashedConfig);
              }, 120));
            }
            case 'cancel_flash':
              return Promise.resolve(null);
            case 'plugin:dialog|open':
              return Promise.resolve('/home/tester/Downloads/shellos-20261112-1.5.3.bin');
            case 'pick_image_file':
              return Promise.resolve({
                info: {
                  ...b.imageInfo,
                  kind: args.file.kind,
                  version: b.ownFileOld
                    ? '1.4.0'
                    : (args.file.kind === 'firmware'
                      ? b.fixtures.firmware.files[0].version
                      : b.fixtures.database.files[0].date),
                  fwVariant: b.ownFileVariant ?? b.imageInfo.fwVariant,
                  matchesRelease: null,
                },
                key: 'file://' + args.file.path,
              });
            case 'load_update_record':
              return Promise.resolve(null);
            case 'save_update_record':
            case 'clear_update_record':
              return Promise.resolve(null);
            case 'list_releases':
              // `__backOnline` is set by a state that announces the network coming back: the list is
              // unreachable until then and reachable afterwards, so a screen that never came back
              // down fails here rather than in a browser.
              if ((b.listError || !b.online) && !window.__backOnline) {
                return fail(b.listError || NO_CONNECTION);
              }
              return Promise.resolve({
                ...b.fixtures[args.kind],
                newestOffered: args.kind === 'firmware' ? b.newestOffered : true,
              });
            // The verify screens: a list, a challenge, and a verdict.
            case 'units_status':
              // As the command answers: an empty status, not an error, when nothing is loaded.
              if (b.units === 'none') {
                return Promise.resolve({ build: null, origin: null, fetchedAt: null, file: null });
              }
              // `fetchedAt` is days since the epoch, as `units_status` sends it. A `const` inside one
              // `case` is uninitialised when a different `case` runs first, which is what threw.
              function day(yyyymmdd) { return Math.round((Date.parse(yyyymmdd) / 1000) / 86400); }
              return Promise.resolve(b.units === 'cached'
                ? { build: 20260812, origin: 'cached', fetchedAt: day('2026-08-12'), file: null }
                : b.units === 'file'
                  ? { build: 20261112, origin: 'file', fetchedAt: day('2026-11-12'), file: 'units-2026-11-12.bin' }
                  : { build: 20261112, origin: 'online', fetchedAt: day('2026-11-13'), file: null });
            case 'units_use_file_bad':
              return fail('that file was refused: no produced-units list in it');
            case 'units_use_file':
              if (b.fileFails) return fail('that file was refused: no produced-units list in it');
              return Promise.resolve({
                build: 20261112, origin: 'file', fetchedAt: day('2026-11-12'),
                file: 'units-2026-11-12.bin',
              });
            case 'units_fetch':
              if (b.fetchHangs) return new Promise(() => {});
              // Past the allowance until the network returns, like `list_releases`: the state that
              // announces the network coming back must be able to see the screen recover without
              // leaving it.
              if ((b.units === 'none' || b.units === 'cached')
                && (!b.online || !window.__backOnline)) {
                return fail(b.longReason
                  ? 'the downloaded units list was refused: the signature over 131200 bytes does not '
                    + 'match the pinned key 032ac68f2b2a500ebf1bc31f0c36e4645b24530ce6d1460e92c2d94e9a47ba550e, '
                    + 'so nothing was cached and nothing can be concluded'
                  : 'https://api.github.com/repos/keycard-tech/keycard-shell/units/latest is past '
                    + "GitHub's allowance for this network, which renews in 20 minutes");
              }
              return Promise.resolve({
                build: 20261112, origin: 'online', fetchedAt: day('2026-11-13'), file: null,
              });
            case 'units_challenge':
              // The command answers with the challenge the session holds, and makes a new one only
              // when asked to renew or when nothing is held yet.
              if (args.renew || !window.__challenge) {
                window.__challengeMints = (window.__challengeMints || 0) + 1;
                window.__challenge = 'UR:DEV-AUTH/oeadadamhdcx' + (window.__challengeMints - 1);
              }
              return Promise.resolve({ text: window.__challenge, svg: b.qr });
            case 'link_code':
              // The air-gapped codes: the same drawing the challenge uses, for any url.
              return Promise.resolve(b.qr);
            case 'units_check':
              return Promise.resolve(b.verdict === 'authentic' ? { outcome: 'authentic' }
                : b.verdict === 'notRegistered'
                  ? { outcome: 'notRegistered', uid: 'ff11aa22bb33cc44dd55ee6677889900' }
                  : { outcome: 'refused', reason: 'bytewords checksum does not match its content' });
            // The scanner: a camera list from the Rust, and frames pulled one at a time. The answer is
            // on a timer so the app's pull loop runs at a frame rate rather than as fast as the CPU.
            case 'scan_start':
              if (b.scan === 'none') return fail('No camera is connected. Plug one in and try again.');
              if (b.scan === 'busy') return fail('Another app is using the camera. Close it and try again.');
              if (b.scan === 'denied') {
                return fail('Camera access is off for this app. Turn it on in System Settings.');
              }
              // What the Rust answers: one entry per camera, and which one is running — the second, no
              // matter what was asked for, because the camera asked for may not be the one that opened.
              return Promise.resolve({
                cameras: [
                  { id: '0', label: 'Integrated Camera' },
                  { id: '4', label: 'External Camera' },
                ],
                device: '4',
              });
            case 'scan_frame':
              if (b.scan === 'silent') {
                return new Promise((_, reject) => setTimeout(
                  () => reject('The camera sent no picture. Try another one below.'), 120));
              }
              return new Promise((resolve) => setTimeout(
                () => resolve(Uint8Array.from(atob(b.jpeg), (c) => c.charCodeAt(0))), 60));
            case 'scan_stop':
              return Promise.resolve(null);
            default: return fail('unexpected command ' + command);
          }
        } },
        event: {
          listen: (name, callback) => {
            window.__events__[name] = callback;
            return Promise.resolve(() => {});
          },
        },
      };
    """
    return template.replace('__BRIDGE__', json.dumps(bridge))


def page(name):
    html = open(f'{FRONTEND}/index.html').read()
    entry = '<script type="module" src="dist/main.js"></script>'
    return html.replace(entry, f'<script>{stub(name)}</script>\n' + entry)


def render(name, report):
    options, actions = STATES[name][0], STATES[name][1]
    # A frame has to arrive and be painted before the scan screens are measured.
    settle_ms = settle(options)
    loop = GLib.MainLoop()
    win = Gtk.OffscreenWindow()
    view = WebKit2.WebView()
    view.set_size_request(WINDOW['width'], WINDOW['height'])
    win.add(view)
    win.show_all()

    def finish():
        win.destroy()
        loop.quit()

    def snapshot():
        view.get_snapshot(WebKit2.SnapshotRegion.FULL_DOCUMENT, WebKit2.SnapshotOptions.NONE,
                          None, on_snapshot)

    def on_snapshot(_view, result):
        try:
            # This binding hands back the cairo surface directly, not a WebKitSnapshot.
            view.get_snapshot_finish(result).write_to_png(f'{OUT}/{name}.png')
        except Exception as error:  # noqa: BLE001
            report.setdefault('snapshot', repr(error))
        finish()

    def measured(_view, result):
        try:
            text = view.run_javascript_finish(result).get_js_value().to_string()
            report.update(json.loads(text))
        except Exception as error:  # noqa: BLE001
            report['measure'] = repr(error)
        snapshot()

    def measure_now():
        view.run_javascript(MEASURE, None, measured)

    def on_load(_view, event):
        if event != WebKit2.LoadEvent.FINISHED:
            return True

        # One click per hop, with the app's own work in between: the clicks are a person, and a
        # person cannot press the camera button in the same instant they open the screen it is on.
        def drive(step=0):
            if step < len(actions):
                view.run_javascript(actions[step], None, None)
                GLib.timeout_add(250, drive, step + 1)
                return False
            GLib.timeout_add(settle_ms, measure_now)
            return False

        GLib.timeout_add(250, drive)
        return False

    view.connect('load-changed', on_load)
    view.load_html(page(name), base_uri())
    watchdog = GLib.timeout_add(20000, lambda: (report.setdefault('timeout', True), loop.quit())[1])
    loop.run()
    GLib.source_remove(watchdog)
    return report


def main():
    os.makedirs(OUT, exist_ok=True)
    bad = 0
    attempts = {}
    for name in (sys.argv[1:] or STATES):
        report = render(name, {})
        
        while (report.get('timeout') or not report.get('contentBottom')) and attempts.pop(name, 0) < 1:
            # A shared WebKit process under Xvfb occasionally needs a second pass to answer the
            # measurement at all; a real failure repeats itself, a hiccup does not.
            attempts[name] = attempts.get(name, 0) + 1
            report = render(name, {})
        
        overflow = report.get('overflowY', 0)
        fit = overflow <= 1
        overlap = report.get('overlaps', [])
        off = (report.get('checkboxCentre') or 0) - (report.get('titleGlyphCentre') or 0)
        off_icon = (report.get('iconCentre') or 0) - (report.get('headingCentre') or 0)
        stretched = report.get('tileWidth') and abs(report['tileWidth'] - report['tileHeight']) > 2
        expect = STATES[name][2]
        # Words a state must show, must show differently, or must not show at all (None).
        want = STATES[name][3] if len(STATES[name]) > 3 else {}
        words = report.get('words') or {}
        sayings = []
        for node, needle in want.items():
            got = words.get(node, 'gone')
            if needle is None:
                if got not in (None, 'gone', ''):
                    sayings.append(f'{node} says {got!r}')
            elif needle not in (got or ''):
                sayings.append(f'{node} says {got!r}, wants {needle!r}')
        shown = report.get('screen')
        stack = report.get('stackOverlaps', [])
        outside = report.get('outsideWindow', [])
        ok = (shown == expect and bool(report.get('contentBottom')) and fit and not sayings
              and not overlap
              and not stack and not outside
              and abs(off) <= 1.0 and abs(off_icon) <= 1.0
              and not stretched and not report.get('tileOutside'))
        bad += 0 if ok else 1
        print(f"{name:<10} {'ok  ' if ok else 'FAIL'} {shown or 'nothing shown'} "
              f"content {report.get('contentBottom')}px in {WINDOW['height']} · overlaps {overlap or 'none'} · "
              f"checkbox {off:+.1f}px, icon {off_icon:+.1f}px off their titles · "
              f"· scroll {report.get('overflowY')}px "
              f"{'· ' + '; '.join(sayings) if sayings else ''}· png wip/{name}.png")
        if not ok:
            print(f'             {report}')
    print(f'window {WINDOW["width"]}x{WINDOW["height"]} at --scale {SCALE}')
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
