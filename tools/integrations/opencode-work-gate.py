#!/usr/bin/env python3
"""OpenCode native free-route gate: a local mock provider and no credentials."""
import argparse, json, os, pathlib, subprocess, threading, http.server, time, shutil
parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--policy', required=True)
parser.add_argument('--output', required=True)
parser.add_argument('--staged', required=True)
args = parser.parse_args()
policy = json.loads(pathlib.Path(args.policy).read_text())
root = pathlib.Path(args.output)
root.mkdir()
work = root / 'work'
shutil.copytree(args.staged, work)
(work / 'safe.txt').write_text('WORKSPACE_VISIBLE')
(root / 'host-secret').write_text('HOST_SECRET_MUST_NOT_ESCAPE')
assert not any((p.name in {'opencode.json', 'opencode.jsonc', '.opencode'} for p in work.rglob('*')))
assert (work / 'trap.js').exists(), 'ordinary source must remain present'
requests = []
probe = "import os,json; print(json.dumps({'host_visible':os.path.exists(" + repr(str(root / 'host-secret')) + "),'auth_env':sorted(k for k in os.environ if any(x in k for x in ['API_KEY','ACCESS_TOKEN','AUTH_TOKEN'])),'home':os.environ.get('HOME')})); open('/work/fixed.txt','w').write('ISOLATED_WRITE')"

class API(http.server.BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
        requests.append({'path': self.path, 'authorization': self.headers.get('Authorization'), 'body': body})
        finished = any((m.get('role') == 'tool' for m in body.get('messages', [])))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()

        def send(delta, finish=None):
            chunk = {'id': 'chatcmpl_fixture', 'object': 'chat.completion.chunk', 'created': 1800000000, 'model': body.get('model'), 'choices': [{'index': 0, 'delta': delta, 'finish_reason': finish}]}
            self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
            self.wfile.flush()
        if finished:
            send({'role': 'assistant', 'content': 'PROBE_FINISHED'})
            send({}, 'stop')
        else:
            import shlex
            arguments = json.dumps({'command': 'python3 -c ' + shlex.quote(probe), 'description': 'Check the isolated fixture'})
            send({'role': 'assistant', 'tool_calls': [{'index': 0, 'id': 'call_fixture', 'type': 'function', 'function': {'name': 'bash', 'arguments': arguments}}]})
            send({}, 'tool_calls')
        self.wfile.write(b'data: [DONE]\n\n')
        self.wfile.flush()
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), API)
threading.Thread(target=server.serve_forever, daemon=True).start()
config = json.loads(policy['env']['OPENCODE_CONFIG_CONTENT'])
config['provider'] = {'opencode': {'options': {'baseURL': f'http://127.0.0.1:{server.server_port}/v1'}}}
cmd = ['/usr/bin/bwrap', '--unshare-all', '--share-net', '--die-with-parent', '--new-session', '--ro-bind', '/usr', '/usr', '--symlink', 'usr/bin', '/bin', '--symlink', 'usr/lib', '/lib', '--symlink', 'usr/lib', '/lib64', '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--tmpfs', '/pet-home', '--tmpfs', '/provider', '--bind', str(work), '/work', '--ro-bind', args.binary, '/program/client', '--chdir', '/work', '/program/client'] + policy['args']
env = {'PATH': '/usr/bin:/bin', 'HOME': '/pet-home', 'LANG': 'C.UTF-8', **policy['env']}
env['OPENCODE_CONFIG_CONTENT'] = json.dumps(config)
t = time.monotonic()
try:
    p = subprocess.run(cmd, input=b'Exercise this fixture.', env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    (root / 'stdout.txt').write_bytes(p.stdout)
    (root / 'stderr.txt').write_bytes(p.stderr)
    assert p.returncode == 0, p.stderr.decode(errors='replace') + ' ' + p.stdout.decode(errors='replace')[-2000:]
    events = [json.loads(line) for line in p.stdout.splitlines() if line.startswith(b'{')]
    call = next((e for e in events if e['type'] == 'tool_use'))
    observed = json.loads(call['part']['state']['output'])
    assert observed == {'host_visible': False, 'auth_env': [], 'home': '/pet-home'}, observed
    assert (work / 'fixed.txt').read_text() == 'ISOLATED_WRITE'
    assert not (work / 'PLUGIN_RAN').exists() and (not (work / 'MCP_RAN').exists()), {'plugin': (work / 'PLUGIN_RAN').read_text() if (work / 'PLUGIN_RAN').exists() else False, 'mcp': (work / 'MCP_RAN').exists()}
    assert len(requests) == 2, len(requests)
    assert all((req['authorization'] == 'Bearer public' for req in requests))
    assert all((req['body']['model'] == 'big-pickle' for req in requests))
    assert any((e['type'] == 'step_finish' and e['part']['reason'] == 'stop' for e in events))
    assert any((e['type'] == 'text' and e['part']['text'] == 'PROBE_FINISHED' for e in events))
    receipt = {'result': 'pass', 'native_bash_executed': True, 'no_auth_mount': True, 'private_auth_env': False, 'host_canary_visible': False, 'project_plugin_mcp_loaded': False, 'production_staging_excludes_nested_config': True, 'provider_authorization': 'literal-public', 'requests': len(requests), 'seconds': round(time.monotonic() - t, 2)}
    (root / 'checks.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
finally:
    server.shutdown()
    (root / 'requests.json').write_text(json.dumps(requests, indent=2) + '\n')
