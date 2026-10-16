#!/usr/bin/env python3
"""Nested Omarchy QA. Requires a dedicated, already-running nested compositor.

Example (run from the OUTER session, never export the nested target first):
  scripts/rig-qa.sh --home "$RUN/qa/home" --output "$RUN/qa" \
    --display wayland-2 --instance NESTED_SIGNATURE --binary /path/to/codepet \
    --pointer /path/to/vpointer

Output and HOME must be new/empty; nothing is deleted. The compositor is never
started, stopped, resized or sent workspace commands. Only this HOME's exact qs
PIDs on the supplied display are terminated. Synthetic export screenshots are
not hook-ingestion, live-client, visual-acceptance or click-through qualification.
The staged plugin alone gains a manifested QA observer/page selector.
The operator must establish that the supplied compositor uses a nested backend
on the outer display. The guard verifies a distinct Hyprland/socket binding,
but does not verify its backend or parent display.
"""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import signal
import stat
import subprocess
import time


REPO = Path(__file__).resolve().parents[1]
SHELL = Path('/usr/share/omarchy/shell')
# Installed Style.qml uses round(base * 1.333) for headings. PopupTheme.qml
# uses Style.effectiveSpacingScale * .75, i.e. base / 16 at default spacing.
FONT_EXPECTATIONS = {
    12: {'headingSize': 16, 'scale': .75},
    16: {'headingSize': 21, 'scale': 1.0},
    20: {'headingSize': 27, 'scale': 1.25},
}
CONNECT_CLIENTS = ('claude-code', 'opencode', 'hermes', 'grok-cli')
HELPER_CLIENTS = ('claude-code', 'codex', 'opencode', 'hermes', 'grok-cli')
OBSERVE = r'''
        // QA ONLY: installed staging copy, never a product API.
        function qaObserve(): string {
            var card = popupLoader.item, app = card ? card.app : null, nodes = []
            function walk(item, depth) {
                if (!item || depth > 24) return
                if ((item.objectName || item.text !== undefined) && item.visible && item.width > 0 && item.height > 0) {
                    var p = item.mapToItem(null, 0, 0)
                    nodes.push({name: item.objectName, x: p.x, y: p.y,
                        width: item.width, height: item.height,
                        text: item.text === undefined ? null : String(item.text)})
                }
                var kids = item.children || []
                for (var i = 0; i < kids.length; i++) walk(kids[i], depth + 1)
            }
            if (app) walk(app, 0)
            return JSON.stringify({state: root.barState, face: root.face,
                faceWakeRunning: faceWake.running, popup: !!app,
                screen: app ? app.screen : null, animating: app ? app.animating : false,
                headingSize: app ? app.t.fontHeading : null, scale: app ? app.t.scale : null,
                card: card ? {x: card.cardOrigin.x, y: card.cardOrigin.y,
                    width: card.contentWidth, height: card.contentHeight} : null,
                nodes: nodes})
        }
        function qaPage(page: string): void {
            if (!popupLoader.item) return
            var app = popupLoader.item.app
            if (page === "menu") app.mode = "menu"
            else { app.mode = "popup"; app.home(); if (page) app.go(page,
                page === "connect" ? {offered: ["claude-code", "opencode", "hermes", "grok-cli"]} : {}) }
        }
        function qaSynthetic(on: bool): void { root.qaSyntheticMode = on }
        function qaConnect(count: int): void {
            if (!popupLoader.item || count < 1 || count > 4) return
            var app = popupLoader.item.app
            app.mode = "popup"; app.home()
            app.go("connect", {offered: ["claude-code", "opencode", "hermes", "grok-cli"].slice(0, count)})
        }
        function qaGuide(kind: string, page: int): void {
            if (!popupLoader.item) return
            function find(item) {
                if (item.objectName === "cp.body") return item
                var kids = item.children || []
                for (var i = 0; i < kids.length; i++) { var got = find(kids[i]); if (got) return got }
                return null
            }
            var body = find(popupLoader.item.app)
            if (body && body.item && body.item.view !== undefined) {
                body.item.current = "codex"; body.item.guide = kind
                body.item.guidePage = page; body.item.view = "guide"
            }
        }
'''


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def safe_path(value):
    path = Path(value).absolute()
    require(path == path.resolve(), f'Symlinks or noncanonical path refused: {path}')
    return path


def validate_target(display, instance, original):
    require(bool(original.get('WAYLAND_DISPLAY')), 'Run from the original outer Wayland session')
    require(bool(original.get('HYPRLAND_INSTANCE_SIGNATURE')), 'Outer Hyprland signature missing')
    require(display != original['WAYLAND_DISPLAY'], 'Refusing original display')
    require(instance != original['HYPRLAND_INSTANCE_SIGNATURE'], 'Refusing original compositor')
    require('/' not in display and display.startswith('wayland-'), 'Expected a Wayland socket name')


def proc_info(pid):
    try:
        proc = Path('/proc') / str(pid)
        env = dict(x.split(b'=', 1) for x in (proc / 'environ').read_bytes().split(b'\0') if b'=' in x)
        argv = [os.fsdecode(x) for x in (proc / 'cmdline').read_bytes().split(b'\0') if x]
        return env, argv
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return {}, []


def matching_qs(pid, home, display):
    env, argv = proc_info(pid)
    return bool(argv and Path(argv[0]).name in ('qs', 'quickshell')
                and env.get(b'HOME') == os.fsencode(home)
                and env.get(b'WAYLAND_DISPLAY') == display.encode())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rename_converged(observed, expected):
    return (observed.get('state_version') == expected['state_version']
            and (observed.get('life') or {}).get('name') == expected['life']['name'])


def font_settings(size):
    require(size in FONT_EXPECTATIONS, f'Unsupported QA font size: {size}')
    return f'[font]\nbase-size = {size}\n'


def font_converged(observed, size):
    return all(observed.get(key) == value for key, value in FONT_EXPECTATIONS[size].items())


def page_converged(observed, expected):
    nodes = observed.get('nodes', [])
    rows = sorted(n['name'] for n in nodes if n.get('name', '').startswith(expected['row_prefix']))
    texts = {n.get('text') for n in nodes}
    return (observed.get('screen') == expected['screen']
            and rows == sorted(expected['rows'])
            and all(text in texts for text in expected.get('texts', [])))


class Rig:
    def __init__(self, args):
        self.args = args
        self.original = dict(os.environ)
        validate_target(args.display, args.instance, self.original)
        self.output, self.home = safe_path(args.output), safe_path(args.home)
        require(self.home.is_relative_to(self.output) and self.home != self.output,
                'HOME must be a child of output')
        require(not self.home.exists() or (self.home.is_dir() and not any(self.home.iterdir())),
                'HOME must be new/empty')
        if self.output.exists():
            require(self.output.is_dir(), 'Output must be a directory')
            require(all(p == self.home for p in self.output.iterdir()),
                    'Output must be new/empty (an empty HOME is allowed)')
        self.binary, self.pointer = safe_path(args.binary), safe_path(args.pointer)
        for p in (self.binary, self.pointer):
            require(p.is_file() and os.access(p, os.X_OK), f'Not executable: {p}')
        for tool in ('hyprctl', 'qs', 'grim'):
            require(shutil.which(tool), f'Missing command: {tool}')
        self.env = {'HOME': str(self.home), 'PATH': f'{self.home}/.local/bin:/usr/bin:/bin',
                    'XDG_RUNTIME_DIR': self.original.get('XDG_RUNTIME_DIR', ''),
                    'WAYLAND_DISPLAY': args.display, 'HYPRLAND_INSTANCE_SIGNATURE': args.instance,
                    'XDG_CONFIG_HOME': str(self.home / '.config'),
                    'XDG_STATE_HOME': str(self.home / '.local/state'),
                    'XDG_DATA_HOME': str(self.home / '.local/share'),
                    'OMARCHY_PATH': '/usr/share/omarchy', 'QT_QPA_PLATFORM': 'wayland',
                    'LANG': 'C.UTF-8', 'TMPDIR': str(self.output / 'scratch'),
                    'CODEPET_TEST_STATE': '1'}
        self.compositor_pid = None
        self.shell_pid = None
        self.manifest = {'owner': str(REPO), 'purpose': 'SPEC-v0.3 9.4 nested QA',
                         'display': args.display, 'instance': args.instance, 'home': str(self.home),
                         'binary': str(self.binary), 'binary_sha256': digest(self.binary),
                         'retention': 'Preserve evidence; HOME is disposable only after review.',
                         'review': 'pending human/agent screenshot review', 'checks': {}, 'captures': [],
                         'limitations': ['Synthetic exports do not test hook ingestion.',
                                         'Nested backend and parent display are operator-verified prerequisites; the guard does not verify them.',
                                         'Overlay click-through/click-to-Home and upgrade flow need separate QA.',
                                         'No live-desktop, live-client or K12 read-aloud acceptance.']}

    def guard(self):
        validate_target(self.args.display, self.args.instance, self.original)
        found = json.loads(subprocess.check_output(['hyprctl', 'instances', '-j'],
                                                   env=self.original, text=True, timeout=5))
        matches = [x for x in found if x['instance'] == self.args.instance
                   and x['wl_socket'] == self.args.display]
        require(len(matches) == 1, 'Nested compositor/socket binding not unique')
        pid = int(matches[0]['pid'])
        require(Path(f'/proc/{pid}').stat().st_uid == os.getuid(), 'Compositor owner mismatch')
        _, argv = proc_info(pid)
        require(argv and Path(argv[0]).name.lower() == 'hyprland', 'Target is not Hyprland')
        require(self.compositor_pid in (None, pid), 'Compositor changed during run')
        self.compositor_pid = pid
        sock = Path(self.env['XDG_RUNTIME_DIR']) / self.args.display
        require(sock.exists() and stat.S_ISSOCK(sock.stat().st_mode), 'Display socket unavailable')

    def run(self, argv, **kwargs):
        self.guard()
        return subprocess.run([str(x) for x in argv], env=self.env, text=True,
                              check=True, timeout=30, **kwargs)

    def save(self):
        (self.output / 'manifest.json').write_text(json.dumps(self.manifest, indent=2) + '\n')

    def pids(self):
        return [int(p.name) for p in Path('/proc').glob('[0-9]*')
                if matching_qs(int(p.name), self.home, self.args.display)]

    def ipc(self, target, method, *args):
        require(self.shell_pid and matching_qs(self.shell_pid, self.home, self.args.display),
                'Exact staged shell PID is missing')
        return self.run(['qs', 'ipc', '--pid', self.shell_pid, 'call', target, method, *args],
                        capture_output=True).stdout.strip()

    def observe(self):
        return json.loads(self.ipc('greyforge.codepet', 'qaObserve'))

    def wait(self, predicate, seconds=8):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(.1)
        raise RuntimeError('Timed out waiting for nested QA condition')

    def rpc(self, method, params=None):
        request = {'v': 1, 'id': f'rig-{time.time_ns()}', 'method': method, 'params': params or {}}
        reply = json.loads(self.run([self.home / '.local/bin/codepet', 'rpc'],
                                   input=json.dumps(request) + '\n', capture_output=True).stdout)
        with (self.output / 'rpc.jsonl').open('a') as log:
            log.write(json.dumps({'request': request, 'response': reply}) + '\n')
        require(reply.get('ok'), f'RPC failed: {reply}')
        return reply['result']

    def stage(self):
        self.guard()
        # Never replace another shell on the nested target. It must be dedicated.
        for proc in Path('/proc').glob('[0-9]*'):
            env, argv = proc_info(proc.name)
            require(not (argv and Path(argv[0]).name in ('qs', 'quickshell')
                         and env.get(b'WAYLAND_DISPLAY') == self.args.display.encode()),
                    'Target already has qs processes; use a dedicated empty nested compositor')
        self.output.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.home.mkdir(parents=True, exist_ok=True, mode=0o700)
        (self.output / 'scratch').mkdir()
        self.manifest['source'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip()
        self.manifest['dirty'] = subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True)
        self.save()
        config = self.home / '.config/omarchy'
        config.mkdir(parents=True)
        (config / 'shell.json').write_text(json.dumps({'version': 1, 'plugins': [],
            'disabledPlugins': ['omarchy.background', 'omarchy.idle', 'omarchy.screensaver'],
            'idle': {'lock': 0, 'screensaver': 0},
            'bar': {'position': 'top', 'layout': {'left': [], 'center': [], 'right': [
                {'id': 'greyforge.codepet', 'overlayOnLoad': True,
                 'stateHome': str(self.home / '.local/state')}]}}}))
        (config / 'shell.toml').write_text(font_settings(12))
        stubs = self.output / 'installer-stubs'
        stubs.mkdir()
        (stubs / 'qs').write_text('#!/bin/sh\n[ "$1" = list ] && { echo "No running instances."; exit 0; }; exit 1\n')
        (stubs / 'omarchy').write_text('#!/bin/sh\n[ "$1 $2" = "plugin validate" ] || exit 1\n[ "$3" = --help ] && exit 0\nexec /usr/share/omarchy/bin/omarchy-plugin-validate "$3"\n')
        for path in stubs.iterdir():
            path.chmod(0o700)
        env = dict(self.env, CODEPET_BIN_SOURCE=str(self.binary), CODEPET_QS=str(stubs / 'qs'),
                   CODEPET_OMARCHY=str(stubs / 'omarchy'))
        self.guard()
        with (self.output / 'install.log').open('w') as log:
            subprocess.run([REPO / 'packaging/omarchy/install.sh'], env=env, check=True,
                           stdout=log, stderr=log, timeout=90)
        plugin = config / 'plugins/greyforge.codepet/BarWidget.qml'
        original = plugin.read_text()
        anchor = '        function openHome(): void { root.open() }'
        require(original.count(anchor) == 1, 'Staging observer anchor changed')
        amended = original.replace(anchor, OBSERVE + anchor)
        property_anchor = '    property var barState: null'
        request_anchor = '    function request(method, params, callback) {'
        require(amended.count(property_anchor) == 1 and amended.count(request_anchor) == 1,
                'Synthetic fixture isolation anchors changed')
        amended = amended.replace(property_anchor, '    property bool qaSyntheticMode: false\n' + property_anchor)
        amended = amended.replace(request_anchor, request_anchor + '''
        if (root.qaSyntheticMode) {
            // Fixture-only pages cannot mutate or refresh the real database.
            if (callback) callback(method === "pet.memories"
                ? {ok: true, result: {entries: [], next_before_seq: null}}
                : {ok: false, error: {code: "QA_SYNTHETIC"}})
            return false
        }
''')
        plugin.write_text(amended)
        (self.output / 'qa-observer.qml.txt').write_text(OBSERVE)
        (self.output / 'qa-staged-BarWidget.qml').write_text(amended)
        self.manifest['staged_amendment'] = {'path': str(plugin),
            'purpose': 'geometry/state observer, page selector, synthetic-mode RPC suppression; empty synthetic Memories response',
            'before_sha256': hashlib.sha256(original.encode()).hexdigest(), 'after_sha256': digest(plugin)}
        self.save()
        self.guard()
        with (self.output / 'shell.log').open('w') as log:
            child = subprocess.Popen(['qs', '-p', str(SHELL)], env=self.env, stdin=subprocess.DEVNULL,
                                     stdout=log, stderr=log, start_new_session=True)
        self.shell_pid = child.pid
        time.sleep(3)
        require(self.ipc('shell', 'ping'), 'Nested shell did not answer')

    def monitor(self):
        monitors = json.loads(self.run(['hyprctl', '-j', 'monitors'], capture_output=True).stdout)
        require(len(monitors) == 1 and monitors[0]['scale'] == 1
                and monitors[0]['x'] == 0 and monitors[0]['y'] == 0, 'Require one scale-1 output at 0,0')
        return monitors[0]

    def click(self, x, y):
        mon = self.monitor()
        require(0 <= x < mon['width'] and 0 <= y < mon['height'], 'Click outside nested output')
        self.run([self.pointer, mon['width'], mon['height'], 'click', round(x), round(y), 'sleep', 150])

    def capture(self, name, kind, **details):
        observation = self.observe()
        if 'font_size' in details:
            size = details['font_size']
            require(font_converged(observation, size),
                    f'Capture {name}: expected font {size} {FONT_EXPECTATIONS[size]}, '
                    f'observed headingSize={observation.get("headingSize")} scale={observation.get("scale")}')
        if 'page_coverage' in details:
            require(page_converged(observation, details['page_coverage']),
                    f'Capture {name}: required page rows or hint text missing')
        path = self.output / f'{name}.png'
        self.run(['grim', path])
        crop = None
        if observation['card']:
            c, m = observation['card'], self.monitor()
            x, y = max(0, math.floor(c['x'])), max(0, math.floor(c['y']))
            w, h = min(math.ceil(c['width']), m['width'] - x), min(math.ceil(c['height']), m['height'] - y)
            require(w > 0 and h > 0, 'Popup crop outside output')
            crop = f'{name}-popup.png'
            self.run(['grim', '-g', f'{x},{y} {w}x{h}', self.output / crop])
        (self.output / f'{name}.json').write_text(json.dumps(observation, indent=2) + '\n')
        self.manifest['captures'].append({'image': path.name, 'popup_crop': crop, 'kind': kind,
                                          'observation': f'{name}.json', **details})
        self.save()

    def checks(self):
        self.monitor()
        initial = self.rpc('state.get')
        require(initial['life'] is None, 'K7 requires genuinely fresh state')
        geometry = json.loads(self.ipc('shell', 'debugBarGeometry'))
        cells = [x for x in geometry if x['id'] == 'greyforge.codepet' and x['visible']]
        require(len(cells) == 1, 'Expected one visible CodePet cell')
        cell = cells[0]
        began = time.monotonic()
        self.click(cell['x'] + cell['width'] / 2, cell['y'] + cell['height'] / 2)
        self.wait(lambda: self.observe()['screen'] == 'first_run')
        before = self.observe()
        go = [x for x in before['nodes'] if x['name'] == 'cp.go']
        require(len(go) == 1, 'Start button geometry unavailable')
        self.capture('k7-first-run', 'real-rpc')
        self.click(go[0]['x'] + go[0]['width'] / 2, go[0]['y'] + go[0]['height'] / 2)
        self.wait(lambda: bool(self.observe()['state'].get('life')) and len(self.pids()) >= 2)
        state = self.rpc('state.get')
        elapsed = time.monotonic() - began
        require(elapsed < 60 and state['life']['armed'] and state['life']['appearance'] == 'cat'
                and all(x == 10000 for x in state['meters'].values()), 'K7 fast start did not pass')
        self.manifest['checks']['K7'] = {'status': 'pass', 'clicks': 1, 'taps': 1, 'seconds': elapsed,
                                        'overlay_pids': [p for p in self.pids() if p != self.shell_pid]}
        self.capture('k7-home', 'real-rpc')
        names = ['Birch', 'Cedar', 'Maple', 'Willow',
                 'Juniper' if state['life']['name'] != 'Juniper' else 'Aspen']
        for name in names:
            renamed = self.rpc('pet.rename', {'life_id': state['life']['life_id'], 'name': name})
        require(renamed['state_version'] > state['state_version']
                and renamed['life']['name'] == names[-1], 'Final rename mutation missing')
        exported = json.loads((self.home / '.local/state/codepet/bar-state.json').read_text())
        require(rename_converged(exported, renamed), 'Final rename export does not match mutation')
        self.wait(lambda: rename_converged(self.observe()['state'] or {}, renamed))
        self.manifest['checks']['rename_exports'] = {'status': 'pass', 'names': names,
            'initial_state_version': state['state_version'], 'final_state_version': renamed['state_version'],
            'method': 'five consecutive real RPC mutations without UI waits; final FileView convergence'}
        self.rpc('app.set_enabled', {'enabled': False})
        self.wait(lambda: self.pids() == [self.shell_pid] and not self.observe()['state']['enabled'])
        off = self.observe()
        require(not off['faceWakeRunning'] and not off['animating'], 'Off left a UI scheduler running')
        self.manifest['checks']['Off'] = {'status': 'pass', 'overlay_pids': [],
                                         'faceWakeRunning': False, 'popupAnimating': False}
        self.capture('real-off', 'real-rpc')
        self.save()

    def set_font_size(self, size):
        self.guard()
        # Color.mergeShell layers the user file over applyTheme's theme payload.
        # Change the watched user override itself; theme IPC cannot override it.
        path = self.home / '.config/omarchy/shell.toml'
        temp = path.with_name('qa-shell.tmp')
        temp.write_text(font_settings(size))
        temp.chmod(0o600)
        temp.replace(path)
        try:
            self.wait(lambda: font_converged(self.observe(), size))
        except RuntimeError as exc:
            raise RuntimeError(f'Font {size} did not reach {FONT_EXPECTATIONS[size]}') from exc
        self.manifest['checks'][f'font_size_{size}'] = {
            'status': 'pass', 'expected': FONT_EXPECTATIONS[size],
            'method': 'watched user shell.toml override; observed heading and scale convergence',
        }
        self.save()

    def fixtures(self):
        path = self.home / '.local/state/codepet/bar-state.json'
        fixtures = REPO / 'docs/v0.3/contracts/fixtures/bar-v2'
        def install(doc):
            temp = path.with_name('qa-export.tmp')
            temp.write_text(json.dumps(doc))
            temp.chmod(0o600)
            temp.replace(path)
        self.ipc('greyforge.codepet', 'qaSynthetic', 'true')
        burst = json.loads((fixtures / 'idle-happy.json').read_text())
        for seq in range(5):
            burst['state_version'] = 900000 + seq
            install(burst)
        self.wait(lambda: self.observe()['state']['state_version'] == 900004)
        burst['state_version'] = 900005
        install(burst)
        self.wait(lambda: self.observe()['state']['state_version'] == 900005)
        self.manifest['checks']['synthetic_rename_burst'] = {'status': 'pass',
            'method': 'five immediate atomic export replacements; final convergence then a sixth watched replacement'}
        # Keep the real DB Off throughout. Fixture exports are explicitly synthetic.
        for size in (12, 16, 20):
            self.set_font_size(size)
            for source in sorted(fixtures.glob('*.json')):
                doc = json.loads(source.read_text())
                # Freeze the intended priority by moving fixture timestamps together.
                delta = int(time.time() * 1000) - 1792123900000
                def shift(value):
                    if isinstance(value, dict):
                        return {k: shift(v) for k, v in value.items()}
                    if isinstance(value, list):
                        return [shift(v) for v in value]
                    return value + delta if isinstance(value, int) and 1700000000000 < value < 1900000000000 else value
                doc = shift(doc)
                self.ipc('greyforge.codepet', 'qaPage', '')
                install(doc)
                self.wait(lambda: self.observe()['state'] == doc)
                time.sleep(.2)
                self.capture(f'fixture-{size}-{source.stem}', 'synthetic-export', font_size=size,
                             fixture=str(source.relative_to(REPO)), timestamp_shift_ms=delta)
            doc = json.loads((fixtures / 'idle-happy.json').read_text())
            install(doc)
            self.wait(lambda: self.observe()['state'] == doc)
            for page in ('change', 'more', 'help', 'memories', 'goodbye', 'menu'):
                self.ipc('greyforge.codepet', 'qaPage', page)
                time.sleep(.3)
                self.capture(f'page-{size}-{page}', 'synthetic-page', font_size=size,
                             note='Page selector only; no navigation/Connect actions. RPC suppressed; Memories is synthetic empty.')
            # The mixed frozen export has five apps, a waiting hint and an error
            # hint. Capture Helpers itself; loading this fixture on Home is insufficient.
            doc = json.loads((fixtures / 'clients-mixed.json').read_text())
            install(doc)
            self.wait(lambda: self.observe()['state'] == doc)
            self.ipc('greyforge.codepet', 'qaPage', 'helpers')
            strings = json.loads((REPO / 'omarchy-plugin/strings.json').read_text())
            helpers = {'screen': 'helpers', 'row_prefix': 'cp.helperRow.',
                       'rows': ['cp.helperRow.' + cid for cid in HELPER_CLIENTS],
                       'texts': [strings['hint.open'].replace('{app}', 'Grok'),
                                 strings['reason.too_old'].replace('{app}', 'Hermes'),
                                 strings['privacy.1'].replace('{name}', doc['life']['name']),
                                 strings['privacy.2']]}
            self.wait(lambda: page_converged(self.observe(), helpers))
            self.capture(f'page-{size}-helpers', 'synthetic-page', font_size=size,
                         fixture='docs/v0.3/contracts/fixtures/bar-v2/clients-mixed.json',
                         page_coverage=helpers, note='Five app rows, two row hints and both privacy lines; RPC suppressed.')
            time.sleep(.2)
            for kind, pages in (('codex.trust', range(5)), ('codex.off', range(1))):
                for page in pages:
                    self.ipc('greyforge.codepet', 'qaGuide', kind, str(page))
                    time.sleep(.2)
                    self.capture(f'guide-{size}-{kind}-{page}', 'synthetic-page', font_size=size)
            # Derive bar records from the frozen first-run status fixture. No
            # Connect action is sent; each subset tests only its offered layout.
            status_path = REPO / 'docs/v0.3/contracts/fixtures/clients-status/owner-first-run.json'
            records = {rec['id']: {k: v for k, v in rec.items() if k not in ('id', 'name', 'events_7d')}
                       for rec in json.loads(status_path.read_text())['clients']}
            for count in range(1, 5):
                offered = CONNECT_CLIENTS[:count]
                doc = json.loads((fixtures / 'idle-happy.json').read_text())
                doc['clients'] = {cid: records[cid] for cid in offered}
                install(doc)
                self.wait(lambda: self.observe()['state'] == doc)
                self.ipc('greyforge.codepet', 'qaConnect', str(count))
                connect = {'screen': 'connect', 'row_prefix': 'cp.connectRow.',
                           'rows': ['cp.connectRow.' + cid for cid in offered]}
                self.wait(lambda: page_converged(self.observe(), connect))
                self.capture(f'page-{size}-connect-{count}', 'synthetic-page', font_size=size,
                             fixture=str(status_path.relative_to(REPO)), page_coverage=connect,
                             note='First-run status subset on idle-happy export; offered layout only, no Connect action; RPC suppressed.')
        self.ipc('greyforge.codepet', 'qaSynthetic', 'false')
        self.rpc('state.get')  # restore export from the real Off database
        self.save()

    def cleanup(self):
        # Revalidate immediately before every exact-PID signal. No broad kill or shell IPC.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            self.guard()
            for pid in self.pids():
                self.guard()
                if matching_qs(pid, self.home, self.args.display):
                    try:
                        os.kill(pid, sig)
                    except ProcessLookupError:
                        pass  # It exited after the identity check; continue cleanup.
            deadline = time.monotonic() + 3
            while self.pids() and time.monotonic() < deadline:
                time.sleep(.1)
            if not self.pids():
                self.manifest['cleanup'] = 'verified no staged qs processes remain'
                return
        require(False, f'Staged qs processes remain after cleanup: {self.pids()}')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    for arg in ('home', 'display', 'instance', 'binary', 'output', 'pointer'):
        parser.add_argument('--' + arg, required=True)
    args = parser.parse_args()
    rig = Rig(args)
    try:
        rig.stage()
        rig.checks()
        rig.fixtures()
        rig.manifest['status'] = 'automated checks complete; visual review pending'
    except Exception as exc:
        rig.manifest['status'] = 'failed'
        rig.manifest['error'] = str(exc)
        raise
    finally:
        cleanup_error = None
        if rig.shell_pid:
            try:
                rig.cleanup()
            except Exception as exc:
                cleanup_error = exc
                rig.manifest['status'] = 'failed'
                rig.manifest['cleanup_error'] = str(exc)
        if (rig.output / 'manifest.json').exists():
            rig.save()
        if cleanup_error is not None:
            raise RuntimeError(f'Nested QA cleanup failed: {cleanup_error}') from cleanup_error


if __name__ == '__main__':
    main()
