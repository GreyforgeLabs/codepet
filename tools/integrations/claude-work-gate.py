#!/usr/bin/env python3
"""Pinned native Claude file-tool gate. Local dummy OAuth + mock SSE, no inference.

The Rust test supplies the actual adapter argv/environment. No user credential or
configuration is read; all filesystem writes stay in the caller's fixture root.
"""
import argparse, json, os, pathlib, subprocess, threading, http.server, time
parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--policy', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
policy = json.loads(pathlib.Path(args.policy).read_text())
root = pathlib.Path(args.output)
root.mkdir(exist_ok=True)
work = root / 'work'
work.mkdir(exist_ok=True)
(work / 'safe.txt').write_text('WORKSPACE_VISIBLE')
auth = root / 'auth'
auth.mkdir(exist_ok=True)
(auth / 'secret').write_text('DUMMY_CREDENTIAL_MUST_NOT_ESCAPE')
(auth / '.credentials.json').write_text(json.dumps({'claudeAiOauth': {'accessToken': 'sk-ant-oat01-DUMMY_CREDENTIAL_MUST_NOT_ESCAPE', 'expiresAt': 9999999999999, 'scopes': ['user:inference'], 'subscriptionType': 'max'}}))
link = work / 'credential-link'
if not link.exists():
    link.symlink_to('/auth/secret')
requests = []
paths = ['/work/safe.txt', '/auth/secret', '/provider/secret', '/proc/self/root/auth/secret', '/proc/1/root/auth/secret', '/work/credential-link', '/work/../auth/secret', '/proc/self/environ']

class API(http.server.BaseHTTPRequestHandler):

    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
        requests.append({'path': self.path, 'body': body, 'dummy_oauth_header': self.headers.get('Authorization') == 'Bearer sk-ant-oat01-DUMMY_CREDENTIAL_MUST_NOT_ESCAPE'})
        if 'count_tokens' in self.path:
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"input_tokens":20}')
            return
        calls = [m for m in body.get('messages', []) if any((isinstance(c, dict) and c.get('type') == 'tool_result' for c in m.get('content', []) if isinstance(m.get('content'), list)))]
        tools = [('Read', {'file_path': p}) for p in paths] + [('Grep', {'pattern': 'DUMMY', 'path': '/auth'}), ('Glob', {'pattern': '**/*', 'path': '/auth'}), ('Grep', {'pattern': 'DUMMY', 'path': '/work/credential-link'}), ('Write', {'file_path': '/provider/stolen', 'content': 'bad'}), ('Edit', {'file_path': '/auth/secret', 'old_string': 'DUMMY', 'new_string': 'bad'}), ('Bash', {'command': 'cat /auth/secret'}), ('Agent', {'prompt': 'read /auth/secret', 'subagent_type': 'general-purpose'}), ('Write', {'file_path': '/work/fixed.txt', 'content': 'PERMITTED_WRITE'}), ('Edit', {'file_path': '/work/safe.txt', 'old_string': 'WORKSPACE_VISIBLE', 'new_string': 'PERMITTED_EDIT'}), ('Write', {'file_path': '/work/.claude/settings.json', 'content': '{"hooks":{}}'})]
        content = [{'type': 'text', 'text': 'PROBE_FINISHED'}] if calls else [{'type': 'tool_use', 'id': 'probe' + str(i), 'name': name, 'input': args} for i, (name, args) in enumerate(tools)]
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()

        def send(kind, data):
            self.wfile.write(('event: ' + kind + '\ndata: ' + json.dumps(data) + '\n\n').encode())
            self.wfile.flush()
        send('message_start', {'type': 'message_start', 'message': {'id': 'msg_probe', 'type': 'message', 'role': 'assistant', 'model': 'claude-sonnet-4-6', 'content': [], 'stop_reason': None, 'stop_sequence': None, 'usage': {'input_tokens': 20, 'output_tokens': 1}}})
        for i, c in enumerate(content):
            base = dict(c)
            base['text'] = '' if c['type'] == 'text' else base.get('text', '')
            if c['type'] == 'tool_use':
                base = {'type': 'tool_use', 'id': c['id'], 'name': c['name'], 'input': {}}
            send('content_block_start', {'type': 'content_block_start', 'index': i, 'content_block': base})
            delta = {'type': 'text_delta', 'text': c['text']} if c['type'] == 'text' else {'type': 'input_json_delta', 'partial_json': json.dumps(c['input'])}
            send('content_block_delta', {'type': 'content_block_delta', 'index': i, 'delta': delta})
            send('content_block_stop', {'type': 'content_block_stop', 'index': i})
        send('message_delta', {'type': 'message_delta', 'delta': {'stop_reason': 'end_turn' if calls else 'tool_use', 'stop_sequence': None}, 'usage': {'output_tokens': 30}})
        send('message_stop', {'type': 'message_stop'})
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), API)
threading.Thread(target=server.serve_forever, daemon=True).start()
cmd = ['/usr/bin/bwrap', '--unshare-all', '--share-net', '--die-with-parent', '--new-session', '--ro-bind', '/usr', '/usr', '--symlink', 'usr/bin', '/bin', '--symlink', 'usr/lib', '/lib', '--symlink', 'usr/lib', '/lib64', '--proc', '/proc', '--dev', '/dev', '--tmpfs', '/tmp', '--tmpfs', '/pet-home', '--ro-bind', str(auth), '/auth', '--ro-bind', str(auth), '/provider', '--bind', str(work), '/work', '--ro-bind', args.binary, '/program/client', '--chdir', '/work', '/program/client'] + policy['args']
env = {'PATH': '/usr/bin:/bin', 'HOME': '/pet-home', 'ANTHROPIC_BASE_URL': f'http://127.0.0.1:{server.server_port}', 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC': '1', 'LANG': 'C.UTF-8'}
env.update(policy['env'])
(work / '.claude').mkdir(exist_ok=True)
canary = {'hooks': {'SessionStart': [{'hooks': [{'type': 'command', 'command': 'touch /work/HOOK_RAN'}]}]}}
(work / '.claude/settings.json').write_text(json.dumps(canary))
(auth / 'settings.json').write_text(json.dumps(canary))
start = time.monotonic()
try:
    p = subprocess.run(cmd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=35, input=b'Exercise the deterministic native tool fixture.')
    (root / 'stdout.txt').write_bytes(p.stdout)
    (root / 'stderr.txt').write_bytes(p.stderr)
    assert p.returncode == 0, p.stderr.decode(errors='replace')[:1000]
    results = {}
    for req in requests:
        for message in req['body'].get('messages', []):
            if isinstance(message.get('content'), list):
                results.update({part['tool_use_id']: part for part in message['content'] if part.get('type') == 'tool_result'})
    assert len(results) == 18, results
    assert all((req['dummy_oauth_header'] for req in requests)), 'Native client did not use the isolated subscription access token'
    for index in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 17]:
        assert results['probe' + str(index)].get('is_error'), results['probe' + str(index)]
    assert 'DUMMY_CREDENTIAL_MUST_NOT_ESCAPE' not in json.dumps(results)
    assert 'WORKSPACE_VISIBLE' in str(results['probe0']['content'])
    assert (work / 'fixed.txt').read_text() == 'PERMITTED_WRITE'
    assert (work / 'safe.txt').read_text() == 'PERMITTED_EDIT'
    assert not (work / 'HOOK_RAN').exists()
    exposed = {tool['name'] for tool in requests[0]['body']['tools']}
    assert exposed == {'Read', 'Glob', 'Grep', 'Edit', 'Write', 'WebSearch'}, exposed
    receipt = {'result': 'pass', 'native_tool_calls': len(results), 'denials': 15, 'allowed_read_write_edit': True, 'hooks_ran': False, 'credential_leaked': False, 'refresh_token_present': False, 'subscription_oauth_header': True, 'tools': sorted(exposed), 'seconds': round(time.monotonic() - start, 2)}
    (root / 'checks.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
finally:
    server.shutdown()
    (root / 'requests.json').write_text(json.dumps(requests, indent=2) + '\n')
