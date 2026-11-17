#!/usr/bin/env python3
import importlib.util
import json
import os
import sys

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('WebKit2', '4.1')
from gi.repository import GLib, Gtk, WebKit2  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location('harness', f'{HERE}/webkit-check.py')
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)

state = sys.argv[1] if len(sys.argv) > 1 else 'home'
options, actions = harness.STATES[state][0], harness.STATES[state][1]
# the same settling time the check harness allows this state
settle = harness.settle(options)

WATCH = """
<script>
  window.__log = [];
  for (const level of ['log', 'info', 'warn', 'error']) {
    const was = console[level];
    console[level] = (...args) => {
      window.__log.push(level + ': ' + args.map((a) => String(a)).join(' '));
      was.apply(console, args);
    };
  }
  window.addEventListener('error', (e) =>
    window.__log.push('uncaught: ' + e.message + ' @ ' + e.filename + ':' + e.lineno));
  window.addEventListener('unhandledrejection', (e) =>
    window.__log.push('rejected: ' + e.reason));
</script>
"""

win = Gtk.OffscreenWindow()
view = WebKit2.WebView()
view.set_size_request(harness.WINDOW['width'], harness.WINDOW['height'])
win.add(view)
win.show_all()


def on_load(_view, event):
    if event != WebKit2.LoadEvent.FINISHED:
        return True

    def drive(step=0):
        if step < len(actions):
            view.run_javascript(actions[step], None, None)
            GLib.timeout_add(250, drive, step + 1)
            return False
        GLib.timeout_add(settle, report)
        return False

    def report():
        script = """JSON.stringify({
          active: document.querySelector('.ksh.is-active')?.id ?? null,
          visible: [...document.querySelectorAll('.ksh > *')].filter((s) => !s.hidden).map((s) => s.id),
          problem: (() => { const n = document.getElementById('scan-problem'); return n ? { cls: n.className, color: getComputedStyle(n).color, hidden: n.hidden } : null; })(),
          log: window.__log,
        })"""

        def show(_view, result):
            try:
                data = json.loads(view.run_javascript_finish(result).get_js_value().to_string())
            except Exception as error:  # noqa: BLE001
                print('read failed:', error)
                Gtk.main_quit()
                return
            print('active:', data['active'], '· visible:', ', '.join(data['visible']) or 'none')
            print('scan-problem:', data.get('problem'))
            for line in data['log']:
                print(' ', line)
            Gtk.main_quit()

        view.run_javascript(script, None, show)

    GLib.timeout_add(250, drive)
    return False


html = harness.page(state).replace('<head>', '<head>' + WATCH, 1)
view.connect('load-changed', on_load)
view.load_html(html, harness.base_uri())
GLib.timeout_add(20000, lambda: (print('timeout'), Gtk.main_quit())[1])
Gtk.main()
