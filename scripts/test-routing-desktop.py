"""Actual EXE → actual proxy HTTP/WS → loopback fake providers; no real accounts."""
import gzip
import base64
import datetime
import wave
import json
import socket
import sys
import threading
import subprocess
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.request import Request, build_opener, ProxyHandler
from urllib.error import HTTPError
from playwright.sync_api import sync_playwright, expect

endpoint, output = sys.argv[1:3]
out = Path(output)
out.mkdir(parents=True, exist_ok=True)
models = ['test-compact', 'test-off', 'test-on', 'test-pin', 'test-worker', 'test-history', 'test-all', 'test-bad', 'test-excluded', 'test-stream','gemini-synthetic','claude-synthetic']
calls = []
reads = []
fail_budget_usage = False
usage_remaining = 12.5
lock = threading.Lock()
rate_limited = {'test-off', 'test-on', 'test-history', 'test-all', 'test-excluded'}

class Provider(BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        source=self.path.split('/')[1]
        with lock: reads.append({'source':source,'path':self.path,'authorization':self.headers.get('Authorization')})
        if source=='e': self.reply(429,{'error':'synthetic probe limit'})
        elif source=='f':
            self.send_response(302); self.send_header('Location',f'http://127.0.0.1:{self.server.server_port}/leak/v1/usage');self.send_header('Content-Length','0');self.end_headers()
        elif self.path.endswith('/billing/subscription'): self.reply(200,{'soft_limit_usd':50})
        elif self.path.endswith('/billing/usage'):
            self.reply(503 if fail_budget_usage else 200, {'error':'synthetic outage'} if fail_budget_usage else {'total_usage':200})
        elif self.path.endswith('/usage'): self.reply(200,{'remaining':usage_remaining,'unit':'USD','is_active':True})
        else: self.reply(200, {'data': [{'id': m} for m in models]})
    def reply(self, status, data, content_type='application/json'):
        raw = data.encode() if isinstance(data, str) else json.dumps(data).encode()
        self.send_response(status)
        self.send_header('Content-Type', content_type)
        self.send_header('Content-Length', str(len(raw)))
        if status == 429: self.send_header('Retry-After', '60')
        self.end_headers()
        self.wfile.write(raw)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        source = self.path.split('/')[1]
        with lock: calls.append({'source': source, 'path': self.path, 'body': body, 'headers': {k.lower(): v for k, v in self.headers.items()}})
        model = body['model']
        if source in ['ga','gb']:
            if source=='ga' and model in ['gemini-google-off','gemini-google-on']:
                self.reply(429,{'error':{'message':'synthetic Google quota'}});return
            if model=='claude-google-error':
                self.reply(400,{'error':{'message':'synthetic Google tool schema rejected'}});return
            parts=[{'text':'Google '+source}]
            if body['request'].get('tools'):
                declaration=body['request']['tools'][0]['functionDeclarations'][0]
                parts=[{'functionCall':{'id':'synthetic-call','name':declaration['name'],'args':{'text':'OK'}},'thoughtSignature':'synthetic-signature'}]
            data={'response':{'candidates':[{'index':0,'content':{'role':'model','parts':parts},'finishReason':'STOP'}],
                'usageMetadata':{'promptTokenCount':4,'candidatesTokenCount':2}}}
            if self.path.endswith('/streamGenerateContent'):self.reply(200,'data: '+json.dumps(data)+'\n\n','text/event-stream')
            else:self.reply(200,data)
            return
        if source == 'd':
            self.reply(200, {'id':'chat-synthetic','object':'chat.completion','model':model,'choices':[{'index':0,'message':{'role':'assistant','content':'image received'},'finish_reason':'stop'}]})
        elif (source=='oa' and model in ['gpt-native-on','gpt-native-off']) or (source == 'a' and model in rate_limited) or model == 'test-all':
            self.reply(429, {'error': {'code': 'rate_limit_exceeded', 'message': 'synthetic limit'}})
        elif model == 'test-bad':
            self.reply(400, {'error': {'code': 'invalid_request', 'message': 'synthetic validation'}})
        elif self.path.endswith('/compact'):
            self.reply(200, {'object': 'response.compaction', 'output': [{'type': 'compaction', 'encrypted_content': 'opaque-synthetic-history'}]})
        elif body.get('stream'):
            if model == 'test-stream':
                event = {'type': 'response.failed', 'response': {'status': 'failed', 'error': {'code': 'rate_limit_exceeded'}}}
            else:
                event = {'type': 'response.completed', 'response': {'id': 'resp-test', 'status': 'completed', 'output': [], 'source': source}}
            self.reply(200, 'data: '+json.dumps(event)+'\n\ndata: [DONE]\n\n', 'text/event-stream')
        else:
            self.reply(200, {'id': 'resp-test', 'source': source, 'model': model})

server = ThreadingHTTPServer(('127.0.0.1', 0), Provider)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
http = build_opener(ProxyHandler({}))
checks = []
with sync_playwright() as p:
    browser = p.chromium.connect_over_cdp(endpoint)
    page = browser.contexts[0].pages[0]
    page.wait_for_url('**tauri.localhost/**', timeout=30000)
    page.wait_for_load_state('networkidle')
    expect(page.locator('.isolated-preview-notice')).to_be_visible(timeout=30000)
    def invoke(cmd, args=None):
        return page.evaluate('([cmd,args]) => window.__TAURI_INTERNALS__.invoke(cmd,args)', [cmd, args or {}])
    assert invoke('get_isolated_preview_mode') is True
    assert invoke('get_accounts') == []
    before = invoke('get_settings')
    source_ids = []
    try:
        for source in ['a', 'b', 'c']:
            account = invoke('add_relay_account', {'name': 'Synthetic '+source.upper(), 'baseUrl': f'http://127.0.0.1:{server.server_port}/{source}/v1',
                'apiKey': 'synthetic-key-'+source, 'modelMap': {m: m for m in models}, 'relayProtocol': 'responses'})
            source_ids.append(account['id'])
        a, b, c = source_ids
        def disable_probe(account):
            row=next(r for r in invoke('get_pool_dashboard')['accounts'] if r['id']==account)
            assert row['policy']['probe_models'] and row['policy']['balance_adapter']=='auto'
            invoke('set_source_policy',{'id':account,'policy':dict(row['policy'],probe_models=False,balance_adapter='off')})
        for account in source_ids: disable_probe(account)

        with socket.socket() as s:
            s.bind(('127.0.0.1', 0)); proxy_port = s.getsockname()[1]
        settings = dict(before, proxy_enabled=True, proxy_port=proxy_port, proxy_allow_lan=False)
        invoke('update_settings', {'settings': settings})
        for _ in range(60):
            try:
                http.open(f'http://127.0.0.1:{proxy_port}/health', timeout=1).close(); break
            except OSError: time.sleep(.1)
        else: raise AssertionError('Proxy did not start')
        def request(model, path='/v1/responses', extra=None, headers=None, method='POST', compressed=False):
            payload = dict(model=model, input=[{'role':'user','content':'synthetic input'}], reasoning={'effort':'high'}, stream=False)
            payload.update(extra or {})
            raw = json.dumps(payload).encode()
            h = {'Content-Type':'application/json', 'Authorization':'Bearer should-not-leak', 'Cookie':'private=should-not-leak', 'chatgpt-account-id':'private-account'}
            h.update(headers or {})
            if compressed: raw=gzip.compress(raw); h['Content-Encoding']='gzip'
            req = Request(f'http://127.0.0.1:{proxy_port}'+path, data=raw, headers=h, method=method)
            try: response = http.open(req, timeout=10)
            except HTTPError as error: response = error
            with response:
                return response.status, response.read()
        def seen(model):
            with lock: return [r for r in calls if r['body']['model']==model]
        def select(model, account): invoke('switch_relay_model_account', {'id':account, 'model':model})
        def auto(model, enabled=True, allowed=None):
            invoke('set_model_route_policy', {'model':model, 'policy':{'auto_switch':enabled, 'allowed_sources':allowed if allowed is not None else [a,b]}})
        # First assertion reproduces the original compact bug even with the old EXE.
        select('test-compact', a)
        status, raw = request('relay-current:test-compact', '/v1/responses/compact?test=1',
            {'input':[{'type':'compaction','encrypted_content':'do-not-drop'},{'type':'function_call_output','call_id':'call-test','output':'tool-result'}]}, compressed=True)
        assert status == 200, f'native compact rejected: {status} {raw!r}'
        assert json.loads(raw)['output'][0]['encrypted_content']=='opaque-synthetic-history'
        record = seen('test-compact')[-1]
        assert record['path']=='/a/v1/responses/compact'
        assert record['body']['input'][0]['encrypted_content']=='do-not-drop'
        assert record['body']['input'][1]['output']=='tool-result'
        assert record['headers']['authorization']=='Bearer synthetic-key-a'
        assert 'cookie' not in record['headers'] and 'chatgpt-account-id' not in record['headers']
        checks.append('native compact + query + gzip + opaque history + credential isolation')
        for method,path in [('GET','/v1/responses'),('POST','/v1/embeddings')]:
            assert request('relay-current:test-compact',path,method=method)[0]==400
        checks.append('invalid endpoint/method cannot reach provider')
        select('test-pin', a)
        assert json.loads(request(f'relay-model:{b}:test-pin')[1])['source']=='b'
        assert invoke('get_settings')['current_relay_accounts']['test-pin']==a
        checks.append('explicit account slug is pinned, not redirected to current')
        select('test-off', a)
        assert request('relay-current:test-off')[0]==429
        assert [r['source'] for r in seen('test-off')]==['a']
        checks.append('automatic switch is off by default')
        for model in ['test-on','test-history','test-all','test-bad','test-excluded','test-stream']:
            select(model, a); auto(model)
        # Test the actual checkbox, not only its backend command.
        page.get_by_role('button',name='模型与来源',exact=True).click()
        page.locator('.model-routes-list button').filter(has_text='test-on').click()
        toggle=page.get_by_label('允许同模型自动切换',exact=True)
        expect(toggle).to_be_checked()
        toggle.click(); expect(toggle).not_to_be_checked()
        assert invoke('get_settings')['model_route_policies']['test-on']['auto_switch'] is False
        toggle.click(); expect(toggle).to_be_checked()
        status,raw=request('relay-current:test-on')
        assert status==200 and json.loads(raw)['source']=='b'
        assert [r['source'] for r in seen('test-on')]==['a','b']
        assert all(r['body']['reasoning']['effort']=='high' for r in seen('test-on'))
        assert invoke('get_settings')['current_relay_accounts']['test-on']==b
        assert invoke('get_current_account_id') is None
        checks.append('UI checkbox persists; 429 switches exact model A→B; reasoning and global current unchanged')
        select('test-on', a)
        assert request('relay-current:test-on')[0]==200
        assert [r['source'] for r in seen('test-on')]==['a','b','b']
        checks.append('cooldown prevents repeated hits to rate-limited source')
        assert request('relay-current:test-history',extra={'previous_response_id':'source-specific-id'})[0]==429
        assert [r['source'] for r in seen('test-history')]==['a']
        assert request('relay-current:test-bad')[0]==400
        assert [r['source'] for r in seen('test-bad')]==['a']
        assert request('relay-current:test-stream',extra={'stream':True})[0]==200
        assert [r['source'] for r in seen('test-stream')]==['a']
        checks.append('no replay of source-bound history, validation errors or started streams')
        auto('test-excluded',allowed=[a])
        assert request('relay-current:test-excluded')[0]==429
        assert [r['source'] for r in seen('test-excluded')]==['a']
        assert request('relay-current:test-all')[0]==429
        assert [r['source'] for r in seen('test-all')]==['a','b']
        assert request('relay-current:test-all')[0]==429
        assert [r['source'] for r in seen('test-all')]==['a','b']
        checks.append('unchecked sources never used; all exhausted stops without downgrade')
        invoke('add_session_route',{'sessionId':'synthetic-worker','accountId':b,'label':'synthetic only'})
        select('test-worker',a)
        assert json.loads(request('relay-current:test-worker',headers={'x-pod-worker-route':'synthetic-worker'})[1])['source']=='b'
        assert json.loads(request('test-worker',headers={'session_id':'synthetic-worker'})[1])['source']=='b'
        assert 'x-pod-worker-route' not in seen('test-worker')[-1]['headers']
        # A real WS handshake, including the session identity, goes through the local HTTP bridge.
        from websocket import create_connection
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10,
            header=['x-pod-worker-route: synthetic-worker','x-codex-routing-hint: model=relay-current:test-worker'])
        ws.send(json.dumps({'type':'response.create','model':'relay-current:test-worker','input':[{'role':'user','content':'test'}]}))
        event=json.loads(ws.recv()); ws.close()
        assert event['type']=='response.completed' and event['response']['source']=='b',event
        assert invoke('get_settings')['current_relay_accounts']['test-worker']==a
        checks.append('worker hard binding beats model current over HTTP and WS; bridge preserves identity')
        # Exercise the production chat translation through the real proxy too.
        d = invoke('add_relay_account', {'name':'Synthetic Chat','baseUrl':f'http://127.0.0.1:{server.server_port}/d/v1',
            'apiKey':'synthetic-key-d','modelMap':{'test-tools':'test-tools'},'relayProtocol':'chat_completions'})['id']
        disable_probe(d)
        invoke('refresh_relay_models', {'id':d})
        status,raw = request(f'relay-model:{d}:test-tools',extra={'input':[
            {'type':'function_call','call_id':'screen-one','name':'screen','arguments':'{}'},
            {'type':'function_call_output','call_id':'screen-one','output':[
                {'type':'text','text':'screenshot'}, {'type':'input_image','image_url':'data:image/png;base64,YQ=='}]}
        ]})
        assert status==200,(status,raw)
        translated=seen('test-tools')[-1]['body']['messages']
        assert translated[1]['role']=='tool'
        assert translated[2]['content'][1]['image_url']['url']=='data:image/png;base64,YQ=='
        assert seen('test-tools')[-1]['path']=='/d/v1/chat/completions'
        assert seen('test-tools')[-1]['headers']['authorization']=='Bearer synthetic-key-d'
        count=len(seen('test-tools'))
        status,raw=request(f'relay-model:{d}:test-tools','/v1/responses/compact')
        assert status==501,(status,raw)
        assert len(seen('test-tools'))==count
        checks.append('chat bridge preserves tool images; unsupported compact is 501, no fake rate limit or upstream call')
        page.screenshot(path=str(out/'model-routes.png'),full_page=True)
        # Hiding and disabling are different operations; discovery must preserve both.
        def model_policy(model, enabled=True, visible=True):
            invoke('set_model_route_policy',{'model':model,'policy':{'enabled':enabled,'visible':visible,'auto_switch':False,'allowed_sources':[]}})
        def catalog_ids():
            response=http.open(f'http://127.0.0.1:{proxy_port}/v1/models',timeout=10)
            data=json.load(response)
            return [m.get('slug',m.get('id')) for m in data.get('models',data.get('data',[]))]
        model_policy('test-pin',visible=False)
        assert 'relay-current:test-pin' not in catalog_ids()
        assert request('relay-current:test-pin')[0]==200
        model_policy('test-pin',enabled=False)
        old=len(seen('test-pin'))
        for model in ['relay-current:test-pin',f'relay-model:{a}:test-pin','test-pin']:
            assert request(model)[0]==403
        invoke('refresh_relay_models',{'id':a})
        assert invoke('get_settings')['model_route_policies']['test-pin']['enabled'] is False
        assert len(seen('test-pin'))==old
        from websocket import WebSocketBadStatusException
        try:
            ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10,header=['x-codex-routing-hint: model=relay-current:test-pin'])
            ws.close();raise AssertionError('Disabled model accepted WS upgrade')
        except WebSocketBadStatusException as error: assert error.status_code==403
        checks.append('hidden model remains callable; disabled model rejects plain, pinned, current and WS requests; discovery preserves policy')

        def dashboard(): return invoke('get_pool_dashboard')
        def policy_for(account): return next(row['policy'] for row in dashboard()['accounts'] if row['id']==account)
        def save_source(account,**patch):
            policy=dict(policy_for(account),**patch)
            invoke('set_source_policy',{'id':account,'policy':policy})
        assert all(not row['policy']['probe_models'] and row['policy']['balance_adapter']=='off' for row in dashboard()['accounts'])
        # UI configuration persists, and the manual probe makes only GETs.
        page.get_by_role('button',name='额度与探针',exact=True).click()
        page.locator('.pool-workbench aside').get_by_role('button',name='Synthetic A').click()
        page.get_by_label('自动探测模型',exact=True).check()
        page.get_by_label('余额接口',exact=True).select_option('openai_usage')
        page.get_by_label('共享计费组',exact=True).fill('synthetic-shared')
        page.get_by_role('button',name='保存配置',exact=True).click()
        expect(page.locator('.pool-settings').get_by_role('status')).to_have_text('已保存')
        count=len(calls)
        page.get_by_role('button',name='立即探测',exact=True).click()
        expect(page.locator('.probe-status')).to_contain_text('ok')
        assert len(calls)==count
        assert dashboard()['summary']['balances']['balance / USD']==12.5
        save_source(b,balance_adapter='openai_usage',billing_group='synthetic-shared')
        invoke('probe_source_now',{'id':b})
        assert dashboard()['summary']['balances']['balance / USD']==12.5
        assert not dashboard()['summary']['show_short']
        assert all(r['authorization']=='Bearer synthetic-key-'+r['source'] for r in reads if r['source'] in ['a','b'])
        checks.append('probe settings save from UI; GET-only reads; shared API balance deduplicated; API-only pool has no short window')

        save_source(d,balance_adapter='new_api_budget')
        invoke('probe_source_now',{'id':d})
        assert dashboard()['summary']['balances']['budget / USD']==48
        fail_budget_usage=True
        try: invoke('probe_source_now',{'id':d}); raise AssertionError('Missing usage treated as zero')
        except Exception as error: assert 'http_503' in str(error)
        assert dashboard()['summary']['balances']['budget / USD']==48  # last valid observation, not 50
        assert dashboard()['probes'][d+':balance']['status']=='http_503'
        fail_budget_usage=False
        checks.append('budget separate from cash; failed usage endpoint never becomes zero usage or inflated budget')

        for source in ['e','f']:
            acc=invoke('add_relay_account',{'name':'Synthetic '+source.upper(),'baseUrl':f'http://127.0.0.1:{server.server_port}/{source}/v1',
                'apiKey':'synthetic-key-'+source,'modelMap':{},'relayProtocol':'responses'})['id']
            save_source(acc,balance_adapter='openai_usage')
            try: invoke('probe_source_now',{'id':acc});raise AssertionError('Expected probe failure')
            except Exception as error: assert ('http_429' if source=='e' else 'http_302') in str(error)
            if source=='e':
                count=len([r for r in reads if r['source']=='e'])
                try: invoke('probe_source_now',{'id':acc});raise AssertionError('Cooldown was reported as success')
                except Exception as error: assert 'rate_limited' in str(error)
                assert len([r for r in reads if r['source']=='e'])==count
            save_source(acc,probe_models=False,balance_adapter='off')
        assert not any(r['source']=='leak' for r in reads)
        checks.append('429 blocks manual retry during cooldown; redirects never receive credentials')

        save_source(c,probe_models=True)
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            if dashboard()['probes'].get(c+':models',{}).get('status')=='ok':break
            time.sleep(.25)
        else:raise AssertionError('Background model probe did not run')
        save_source(c,probe_models=False)
        checks.append('background scheduler discovers models without manual action; disabled sources are excluded')
        page.screenshot(path=str(out/'pool-settings.png'),full_page=True)
        # Stale whole-settings writes cannot clobber per-source probe configuration.
        invoke('update_settings',{'settings':settings})
        assert policy_for(a)['balance_adapter']=='openai_usage'
        try:save_source(a,weekly_weight=-1);raise AssertionError('Invalid weight accepted')
        except Exception as error:assert 'Weight' in str(error)
        checks.append('source settings survive stale settings page saves; invalid custom weights rejected')
        # Real second native window, not a mocked browser pet.
        invoke('open_pet')
        pet=None
        for _ in range(100):
            for context in browser.contexts:
                for candidate in context.pages:
                    if candidate.locator('.desktop-pet').count(): pet=candidate;break
                if pet:break
            if pet:break
            page.wait_for_timeout(100)
        assert pet is not None,'Pet WebView did not appear'
        try:
            pet.evaluate("() => window.__TAURI_INTERNALS__.invoke('get_accounts')")
            raise AssertionError('Pet could access account secrets')
        except Exception as error: assert 'cannot access account secrets' in str(error)
        assert request('relay-current:gemini-synthetic')[0]==200
        expect(pet.locator('.pet-character img')).to_have_attribute('src','/pets/gemini.webp')
        expect(pet.locator('.pet-character img')).to_have_css('transform','none')
        pet.wait_for_function("() => { const img=document.querySelector('.pet-character img'); return img && img.complete && img.naturalWidth>0; }")
        invoke('probe_source_now',{'id':a})
        assert invoke('get_pet_activity')['model']=='gemini-synthetic'
        usage_remaining=10
        invoke('probe_source_now',{'id':a})
        api_cache=next(r for r in invoke('get_accounts') if r['id']==a)['relay_usage_cache']
        assert api_cache['remaining']==10 and api_cache['balance_capacity']==12.5 and api_cache['balance_capacity_estimated']
        usage_remaining=12.5
        invoke('probe_source_now',{'id':a})
        assert request('relay-current:claude-synthetic',headers={'x-pod-worker-route':'synthetic-worker'})[0]==200
        assert invoke('get_pet_activity')['model']=='gemini-synthetic'
        pet.get_by_role('button',name='查看用量',exact=True).click()
        assert not invoke('get_pet_preferences')['detailed']
        expect(pet.locator('.pet-mini')).to_be_visible()
        expect(pet.locator('.pet-mini')).not_to_contain_text('—')
        expect(pet.locator('.pet-mini')).to_contain_text('API$60.50')
        model_button=pet.get_by_label('切换详细模式',exact=True)
        box=model_button.bounding_box();pet.mouse.move(box['x']+12,box['y']+12);pet.mouse.down();pet.wait_for_timeout(750);pet.mouse.up()
        expect(pet.locator('.pet-widget')).to_have_class('pet-widget is-expanded')
        assert invoke('get_pet_preferences')['detailed']
        assert pet.locator('.pet-widget-reveal').evaluate('(el)=>getComputedStyle(el).transitionProperty').find('grid-template-rows')>=0
        expect(pet.locator('.pet-galaxy-fill').last).to_be_visible()
        expect(pet.get_by_label('桌宠用量',exact=True)).to_be_visible()
        expect(pet.locator('.pet-balance')).to_have_count(1)
        expect(pet.locator('.pet-balance')).to_contain_text('60.50')
        expect(pet.locator('.pet-balance strong')).to_have_text('$60.50')
        expect(pet.locator('.pet-api-balance .pet-meter')).to_contain_text('API$60.50')
        expect(pet.locator('.pet-api-balance [role="progressbar"]')).to_have_count(1)
        expect(pet.locator('.pet-api-balance [role="progressbar"]')).to_be_visible()
        first_phrase=pet.get_by_label('捏一捏对话',exact=True).inner_text()
        pet.get_by_label('捏一捏对话',exact=True).click()
        assert pet.get_by_label('捏一捏对话',exact=True).inner_text()!=first_phrase
        assert not any(text in pet.locator('.desktop-pet').inner_text() for text in ['Synthetic','只读观察','当前来源','非窗口焦点'])
        invoke('set_pet_preferences',{'preferences':dict(invoke('get_pet_preferences'),follow_activity=False,fixed_family='deepseek')})
        expect(pet.locator('.pet-character img')).to_have_attribute('src','/pets/deepseek.webp')
        pet.screenshot(path=str(out/'desktop-pet.png'),omit_background=True)
        invoke('hide_pet')
        assert request('relay-current:gemini-synthetic')[0]==200
        checks.append('real transparent pet renders licensed skins and usage; worker/probe cannot steal appearance; credential IPC denied; hiding leaves proxy running')
        # Preserve native routing on bulk import, then recover an imported invalid API key.
        native=next(r for r in invoke('get_accounts') if r['id']==a)
        native.update(name='Synthetic native import',is_token_invalid=True,relay_base_url=f'http://127.0.0.1:{server.server_port}')
        native['cached_quota']={'five_hour_left':0,'five_hour_reset':'','weekly_left':0,'weekly_reset':'','plan_type':'','is_valid_for_cli':False,'updated_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
        content=base64.b64encode(json.dumps({'accounts':{'old-id':native}}).encode()).decode()
        result=invoke('bulk_import_accounts',{'files':[{'filename':'accounts.json','content_b64':content}]})
        assert not result['fatal'],result
        imported=next(r for r in invoke('get_accounts') if r['name']=='Synthetic native import')
        assert imported['kind']=='relay' and imported['relay_base_url']==native['relay_base_url']
        assert imported['is_token_invalid']
        save_source(imported['id'],probe_models=False,balance_adapter='off')
        invoke('probe_source_now',{'id':imported['id']})  # manual works while scheduler off
        restored=next(r for r in invoke('get_accounts') if r['id']==imported['id'])
        assert not restored['is_token_invalid'] and len(restored['relay_model_catalog'])>0
        assert any(r['path']=='/v1/models' for r in reads)
        page.get_by_role('button',name='账号管理',exact=True).click()
        expect(page.locator('.account-row').filter(has_text='Synthetic native import')).not_to_have_class(__import__('re').compile(r'\bexpired\b'))
        checks.append('native import preserves type, provider URL and routing; manual GET probe recovers invalid API without OAuth; bare host gains /v1/models')
        # Auto mode is genuinely scheduled without a separate enable step.
        auto=invoke('add_relay_account',{'name':'Synthetic Auto','baseUrl':f'http://127.0.0.1:{server.server_port}/auto/v1',
            'apiKey':'synthetic-auto','modelMap':{},'relayProtocol':'responses'})['id']
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            if dashboard()['probes'].get(auto+':models',{}).get('status')=='ok':break
            time.sleep(.2)
        else: raise AssertionError('Default automatic probe never ran')
        checks.append('new API sources schedule model and auto balance probes by default')
        # Native Google fixtures use only validation-guarded loopback endpoints.
        google_models=['gemini-google-off','gemini-google-on','claude-google-test','claude-google-error','gemini-3.8-flash-low','gemini-3.8-flash-medium','gemini-3.8-flash-high']
        google_ids=[]
        template=next(r for r in invoke('get_accounts') if r['id']==a)
        for source in ['ga','gb']:
            account=dict(template,id='synthetic-'+source,name='Synthetic Google '+source,kind='antigravity_oauth',
                auth_json={'provider':'antigravity','email':source+'@synthetic.invalid',
                    'project_id':f'validation:http://127.0.0.1:{server.server_port}/{source}',
                    'tokens':{'access_token':'synthetic-google-'+source,'refresh_token':'synthetic-refresh-'+source,'expires_at':'2099-01-01T00:00:00Z'},
                    'model_quotas':{m:{'remaining_fraction':1,'updated_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'reset_time':None} for m in google_models}},
                relay_base_url=None,relay_model_map={},relay_model_catalog=[],is_token_invalid=False,cached_quota=None)
            content=base64.b64encode(json.dumps({'accounts':{account['id']:account}}).encode()).decode()
            result=invoke('bulk_import_accounts',{'files':[{'filename':'accounts.json','content_b64':content}]})
            assert not result['fatal'],result
            google_ids.append(next(r['id'] for r in invoke('get_accounts') if r['name']==account['name']))
        ga,gb=google_ids
        grouped=[g for g in invoke('get_model_routes') if g['model'].startswith('gemini-3.8-flash')]
        assert len(grouped)==1 and grouped[0]['model']=='gemini-3.8-flash', 'Gemini effort presets must share one logical model'
        assert len(grouped[0]['sources'])==2
        entries=json.load(http.open(f'http://127.0.0.1:{proxy_port}/v1/models'))['models']
        visible=[e for e in entries if e.get('visibility')!='hide' and e['slug'].startswith('gemini-3.8-flash')]
        assert len(visible)==1 and visible[0]['slug']=='gemini-3.8-flash'
        assert [e['effort'] for e in visible[0]['supported_reasoning_levels']]==['low','medium','high']
        client_check=subprocess.run([sys.executable,str(Path(__file__).with_name('test-catalog-client.py')),str(proxy_port)],capture_output=True,text=True,encoding='utf8',timeout=40)
        assert client_check.returncode==0,(client_check.stdout,client_check.stderr)
        (out/'codex-model-list.json').write_text(client_check.stdout,encoding='utf8')
        checks.append('installed Codex parses the entire proxy catalog and lists every enabled third-party model with no real inference')
        invoke('set_model_source',{'id':gb,'model':'gemini-3.8-flash'})
        for effort in ['low','medium','high']:
            status,raw=request('gemini-3.8-flash',extra={'reasoning':{'effort':effort}})
            assert status==200,(status,raw)
            wire=seen('gemini-3.8-flash-'+effort)[-1]
            assert wire['source']=='gb' and wire['body']['request']['generationConfig']['thinkingConfig']['thinkingLevel']==effort.upper()
        status,raw=request('gemini-3.8-flash-low',extra={'reasoning':None})
        assert status==200,(status,raw)
        assert seen('gemini-3.8-flash-low')[-1]['body']['request']['generationConfig']['thinkingConfig']['thinkingLevel']=='LOW'
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10)
        ws.send(json.dumps({'type':'response.create','model':'gemini-3.8-flash','input':'test','reasoning':{'effort':'medium'}}))
        events=[]
        for _ in range(20):
            event=json.loads(ws.recv());events.append(event)
            if event['type'] in ['response.completed','response.failed','error']:break
        ws.close();assert events[-1]['type']=='response.completed',events
        assert seen('gemini-3.8-flash-medium')[-1]['source']=='gb'
        gemini_relay=invoke('add_relay_account',{'name':'Synthetic Gemini presets','baseUrl':f'http://127.0.0.1:{server.server_port}/gr/v1',
            'apiKey':'synthetic-gr','modelMap':{m:m for m in google_models if m.startswith('gemini-3.8-flash')},'relayProtocol':'responses'})['id']
        disable_probe(gemini_relay)
        grouped=[g for g in invoke('get_model_routes') if g['model'].startswith('gemini-3.8-flash')]
        assert len(grouped)==1 and len(grouped[0]['sources'])==3
        invoke('set_model_source',{'id':gemini_relay,'model':'gemini-3.8-flash'})
        status,raw=request('gemini-3.8-flash',extra={'reasoning':{'effort':'low'}})
        assert status==200,(status,raw)
        assert seen('gemini-3.8-flash-low')[-1]['source']=='gr'
        assert seen('gemini-3.8-flash-low')[-1]['body']['reasoning']['effort']=='low'
        checks.append('Gemini presets share one selector and source; independent effort reaches its bound Google variant; legacy low preset retains its default')
        for m in google_models:invoke('set_model_source',{'id':ga,'model':m})
        assert any(r['model']=='claude-google-test' and r['sources'][0]['protocol']=='google' for r in invoke('get_model_routes'))
        assert request('gemini-google-off')[0]==429
        assert [r['source'] for r in seen('gemini-google-off')]==['ga']
        limited=next(a for a in invoke('get_accounts') if a['id']==ga)['auth_json']['model_quotas']['gemini-google-off']
        reset=datetime.datetime.fromisoformat(limited['reset_time'])
        assert 0<(reset-datetime.datetime.now(datetime.timezone.utc)).total_seconds()<=65
        invoke('set_model_route_policy',{'model':'gemini-google-on','policy':{'enabled':True,'visible':True,'auto_switch':True,'allowed_sources':[gb]}})
        assert request('gemini-google-on')[0]==200
        assert [r['source'] for r in seen('gemini-google-on')]==['ga','gb']
        assert invoke('get_settings')['current_google_accounts']['gemini-google-on']==gb
        checks.append('Google model selector and explicit same-model 429 failover work; unchecked auto mode does not switch')
        invoke('add_session_route',{'sessionId':'synthetic-google-worker','accountId':gb,'label':'synthetic only'})
        status,raw=request('claude-google-test',headers={'x-pod-worker-route':'synthetic-google-worker'},extra={'tools':[{'type':'namespace','name':'functions','tools':[{'type':'function','name':'echo','parameters':{'type':'object','properties':{'text':{'type':'string'}}}}]}]})
        assert status==200,raw
        assert any(o.get('type')=='function_call' for o in json.loads(raw)['output']),raw
        assert seen('claude-google-test')[-1]['source']=='gb'
        assert 'x-pod-worker-route' not in seen('claude-google-test')[-1]['headers']
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10,header=['x-pod-worker-route: synthetic-google-worker','x-codex-routing-hint: model=claude-google-test'])
        ws.send(json.dumps({'type':'response.create','model':'claude-google-test','input':'test'}))
        events=[]
        for _ in range(20):
            event=json.loads(ws.recv());events.append(event)
            if event['type'] in ['response.completed','response.failed','error']:break
        ws.close();assert events[-1]['type']=='response.completed',events
        assert seen('claude-google-test')[-1]['source']=='gb'
        assert invoke('get_settings')['current_google_accounts']['claude-google-test']==ga
        assert request('claude-google-test',path='/v1/responses/compact')[0]==501
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10,header=['x-pod-worker-route: synthetic-google-worker','x-codex-routing-hint: model=claude-google-error'])
        ws.send(json.dumps({'type':'response.create','model':'claude-google-error','input':'test'}))
        error=json.loads(ws.recv());ws.close()
        assert 'synthetic Google tool schema rejected' in json.dumps(error),error
        assert [r['source'] for r in seen('claude-google-error')]==['gb']
        checks.append('Google tools translate end to end; HTTP and WS worker binding wins without changing model current; compact is explicit 501')
        # Native subscription/API accounts participate in the same model selector.
        native_models=['gpt-native-test','gpt-native-on','gpt-native-off']
        catalog=Path.home()/'.switcher-pet/validation/profile/.codex/models_cache.json'
        reasoning_models=['gpt-6-astra','gpt-6-sol','gpt-6.1-sol']
        catalog.parent.mkdir(parents=True,exist_ok=True);catalog.write_text(json.dumps({'models':
            [{'slug':m,'priority':native_models.index(m)+1} for m in reversed(native_models)] +
            [{'slug':m,'priority':20+i,'supported_reasoning_levels':[]} for i,m in enumerate(reasoning_models)] +
            [{'slug':'gpt-6-luna','priority':24,'default_reasoning_level':'medium','supported_reasoning_levels':[{'effort':e,'description':e} for e in ['low','medium','high','xhigh','max']]}]}),encoding='utf-8')
        # A partial desktop cache must not hide GPTs found only in the CLI cache.
        catalog.with_name('models_cache.codex_work_desktop.json').write_text(json.dumps({'models':[{'slug':native_models[1],'priority':2}]}),encoding='utf-8')
        native_ids=[]
        for source,kind in [('oa','chatgpt_oauth'),('ob','openai_key')]:
            account=dict(template,id='synthetic-'+source,name='Synthetic OpenAI '+source,kind=kind,
                auth_json={'tokens':{'access_token':'synthetic-'+source,'refresh_token':'synthetic-refresh-'+source,'account_id':'synthetic-identity-'+source},
                    'validation_endpoint':f'http://127.0.0.1:{server.server_port}/{source}'},
                relay_base_url=None,relay_model_map={},relay_model_catalog=[],is_token_invalid=False,cached_quota=None)
            if kind=='chatgpt_oauth':
                account['cached_quota']={'five_hour_left':25,'five_hour_reset':'','weekly_left':75,'weekly_reset':'','plan_type':'plus','is_valid_for_cli':True,
                    'primary_window_seconds':18000,'secondary_window_seconds':604800,'updated_at':(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(hours=2)).isoformat()}
                claims={'email':'synthetic@example.invalid','https://api.openai.com/auth':{'chatgpt_account_id':'synthetic-identity-'+source,
                    'chatgpt_user_id':'synthetic-user','chatgpt_plan_type':'plus','chatgpt_subscription_active_until':'2099-01-01T00:00:00Z'}}
                account['auth_json']['tokens']['id_token']='test.'+base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip('=')+'.test'
            content=base64.b64encode(json.dumps({'accounts':{account['id']:account}}).encode()).decode()
            invoke('bulk_import_accounts',{'files':[{'filename':'accounts.json','content_b64':content}]})
            native_ids.append(next(r['id'] for r in invoke('get_accounts') if r['name']==account['name']))
        oa,ob=native_ids
        groups=invoke('get_model_routes')
        assert [g['model'] for g in groups[:3]]==native_models
        assert {s['protocol'] for s in groups[0]['sources']}=={'openai_subscription','openai_key'}
        subscription=next(s for s in groups[0]['sources'] if s['id']==oa)
        assert subscription['identity']['plan_label']=='Plus'
        assert subscription['identity']['scope']=='个人空间'
        assert subscription['identity']['status']=='订阅期内'
        assert subscription['identity']['email']=='synthetic@example.invalid'
        assert 'Plus' in subscription['menu_label']
        assert all(secret not in json.dumps(groups) for secret in ['synthetic-refresh-oa','test.eyJ','access_token'])
        global_current=invoke('get_current_account_id')
        for m in native_models:invoke('set_model_source',{'id':oa,'model':m})
        assert request('gpt-native-off')[0]==429
        assert [r['source'] for r in seen('gpt-native-off')]==['oa']
        invoke('set_model_route_policy',{'model':'gpt-native-on','policy':{'enabled':True,'visible':True,'auto_switch':True,'allowed_sources':[ob]}})
        assert json.loads(request('gpt-native-on')[1])['source']=='ob'
        assert [r['source'] for r in seen('gpt-native-on')]==['oa','ob']
        assert invoke('get_settings')['current_model_sources']['gpt-native-on']==ob
        assert json.loads(request('gpt-native-test')[1])['source']=='oa'
        assert seen('gpt-native-test')[-1]['headers']['authorization']=='Bearer synthetic-oa'
        assert 'cookie' not in seen('gpt-native-test')[-1]['headers']
        assert request('gpt-native-test',path='/v1/responses/compact')[0]==200
        invoke('add_session_route',{'sessionId':'synthetic-openai-worker','accountId':ob,'label':'synthetic only'})
        assert json.loads(request('gpt-native-test',headers={'x-pod-worker-route':'synthetic-openai-worker'})[1])['source']=='ob'
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10,header=['x-pod-worker-route: synthetic-openai-worker','x-codex-routing-hint: model=gpt-native-test'])
        ws.send(json.dumps({'type':'response.create','model':'gpt-native-test','input':'test'}))
        event=json.loads(ws.recv());ws.close()
        assert event['type']=='response.completed' and event['response']['source']=='ob',event
        assert invoke('get_current_account_id')==global_current
        assert invoke('get_settings')['current_model_sources']['gpt-native-test']==oa
        # A provider's explicit alias must join the native selector, not become
        # a disconnected API-only entry. Exercise the installed EXE, not a mock resolver.
        alias_source=invoke('add_relay_account',{'name':'Synthetic mapped native','baseUrl':f'http://127.0.0.1:{server.server_port}/mapped/v1',
            'apiKey':'synthetic-mapped','modelMap':{'gpt-native-test':'provider/gpt-native-test','gpt-native-on':'provider/gpt-native-on',
                **{m:'provider/'+m for m in reasoning_models}},'relayProtocol':'responses'})['id']
        disable_probe(alias_source)
        group=next(g for g in invoke('get_model_routes') if g['model']=='gpt-native-test')
        assert alias_source in [s['id'] for s in group['sources']], 'native selector lost the API model-map key'
        page.get_by_role('button',name='模型与来源',exact=True).click()
        page.locator('.model-routes-list button').filter(has_text='gpt-native-test').click()
        page.get_by_label('Synthetic mapped native',exact=True).click()
        expect(page.get_by_label('Synthetic mapped native',exact=True)).to_be_checked()
        expect(page.locator('.model-routes article')).to_contain_text('→ provider/gpt-native-test')
        expect(page.locator('.model-routes article')).to_contain_text('订阅期内')
        expect(page.locator('.model-routes article')).to_contain_text('登录账号：synthetic@example.invalid')
        expect(page.locator('.model-routes article')).to_contain_text('按量计费')
        for m in reasoning_models:
            invoke('set_model_source',{'id':alias_source,'model':m})
        restored=json.load(http.open(f'http://127.0.0.1:{proxy_port}/v1/models?client_version=0.150.0'))['models']
        for m in reasoning_models:
            row=next(r for r in restored if r['slug']==m)
            assert [e['effort'] for e in row['supported_reasoning_levels']]==['low','medium','high','xhigh','max','ultra'],row
            assert request(m,extra={'reasoning':{'effort':'ultra'}})[0]==200
            assert seen('provider/'+m)[-1]['body']['reasoning']['effort']=='ultra'
        assert len(next(r for r in restored if r['slug']=='gpt-6-luna')['supported_reasoning_levels'])==5
        checks.append('OpenAI subscription/identity labels exclude credentials; Astra/Sol native efforts survive API routing and forward ultra unchanged; Luna unchanged')
        page.screenshot(path=str(out/'unified-model-alias.png'),full_page=True)
        assert json.loads(request('gpt-native-test')[1])['source']=='mapped'
        assert seen('provider/gpt-native-test')[-1]['headers']['authorization']=='Bearer synthetic-mapped'
        assert invoke('get_pet_activity')['model']=='gpt-native-test'
        save_source(alias_source,balance_adapter='openai_usage')
        invoke('probe_source_now',{'id':alias_source})
        assert dashboard()['model']=='gpt-native-test'
        assert dashboard()['model_summary']['balances']['balance / USD']==12.5
        assert request('gpt-native-test',path='/v1/responses/compact')[0]==200
        assert seen('provider/gpt-native-test')[-1]['path'].endswith('/responses/compact')
        assert json.loads(request('relay-current:gpt-native-test')[1])['source']=='mapped'
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10)
        ws.send(json.dumps({'type':'response.create','model':'gpt-native-test','input':'test'}))
        event=json.loads(ws.recv());ws.close()
        assert event['type']=='response.completed' and event['response']['source']=='mapped',event
        model_policy('test-pin')
        entries=json.load(build_opener(ProxyHandler({})).open(f'http://127.0.0.1:{proxy_port}/v1/models'))['models']
        visible=[e.get('slug',e.get('id')) for e in entries if e.get('visibility')!='hide']
        assert visible.count('gpt-native-test')==1 and 'relay-current:gpt-native-test' not in visible
        assert 'provider/gpt-native-test' not in visible
        assert not any(g['model']=='provider/gpt-native-test' for g in invoke('get_model_routes'))
        assert 'test-pin' in visible and 'relay-current:test-pin' not in visible
        ordered=sorted([e for e in entries if e.get('visibility')!='hide'],key=lambda e:e['priority'])
        assert [e['slug'] for e in ordered[:3]]==native_models,'Selecting an API source must preserve native GPT rank before third-party models'
        client_check=subprocess.run([sys.executable,str(Path(__file__).with_name('test-catalog-client.py')),str(proxy_port)],capture_output=True,text=True,encoding='utf8',timeout=40)
        assert client_check.returncode==0,(client_check.stdout,client_check.stderr)
        (out/'codex-model-list-native-priority.json').write_text(client_check.stdout,encoding='utf8')
        checks.append('native GPT priority survives partial desktop cache and mapped API source in installed Codex model/list')
        assert json.loads(request('test-pin')[1])['source'] in ['a','b','c']
        ws=create_connection(f'ws://127.0.0.1:{proxy_port}/v1/responses',timeout=10)
        ws.send(json.dumps({'type':'response.create','model':'test-pin','input':'test'}))
        event=json.loads(ws.recv());ws.close()
        assert event['type']=='response.completed' and event['response']['source'] in ['a','b','c'],event
        invoke('set_model_route_policy',{'model':'gpt-native-on','policy':{'enabled':True,'visible':True,'auto_switch':True,'allowed_sources':[alias_source]}})
        invoke('set_model_source',{'id':oa,'model':'gpt-native-on'})
        assert json.loads(request('gpt-native-on')[1])['source']=='mapped'
        assert seen('provider/gpt-native-on')[-1]['source']=='mapped'
        assert invoke('get_settings')['current_model_sources']['gpt-native-on']==alias_source
        invoke('set_model_source',{'id':oa,'model':'gpt-native-test'})
        checks.append('mapped API names join native selector; one visible logical catalog; HTTP/WS/compact rewrite only at source; alias-aware same-model failover')
        # Explicit Chat Completions maps must work without a /models probe.
        mapped_chat=invoke('add_relay_account',{'name':'Synthetic mapped chat','baseUrl':f'http://127.0.0.1:{server.server_port}/d/v1',
            'apiKey':'synthetic-mapped-chat','modelMap':{'gpt-native-test':'provider/gpt-native-test'},'relayProtocol':'chat_completions'})['id']
        disable_probe(mapped_chat)
        group=next(g for g in invoke('get_model_routes') if g['model']=='gpt-native-test')
        assert any(s['id']==mapped_chat and not s['automatic_supported'] for s in group['sources'])
        invoke('set_model_source',{'id':mapped_chat,'model':'gpt-native-test'})
        status,raw=request('gpt-native-test')
        assert status==200,(status,raw)
        assert seen('provider/gpt-native-test')[-1]['path']=='/d/v1/chat/completions'
        assert seen('provider/gpt-native-test')[-1]['headers']['authorization']=='Bearer synthetic-mapped-chat'
        assert invoke('get_pet_activity')['model']=='gpt-native-test'
        count=len(calls)
        assert request('gpt-native-test',path='/v1/responses/compact')[0]==501
        assert len(calls)==count
        invoke('set_model_source',{'id':oa,'model':'gpt-native-test'})
        checks.append('map-only Chat Completions source joins native selector without discovery; correct translation and credential; unsupported compact rejects without calling upstream')
        # Same upstream ID merges native subscription and third-party sources.
        relay_native=invoke('add_relay_account',{'name':'Synthetic native relay','baseUrl':f'http://127.0.0.1:{server.server_port}/nr/v1',
            'apiKey':'synthetic-nr','modelMap':{'gpt-native-test':'gpt-native-test'},'relayProtocol':'responses'})['id']
        disable_probe(relay_native)
        group=next(g for g in invoke('get_model_routes') if g['model']=='gpt-native-test')
        assert len(group['sources'])==5 and len([g for g in invoke('get_model_routes') if g['model']=='gpt-native-test'])==1
        invoke('set_model_source',{'id':relay_native,'model':'gpt-native-test'})
        assert json.loads(request('gpt-native-test')[1])['source']=='nr'
        invoke('set_model_source',{'id':oa,'model':'gpt-native-test'})
        assert json.loads(request('relay-current:gpt-native-test')[1])['source']=='oa'
        invoke('set_model_route_policy',{'model':'gpt-native-test','policy':{'enabled':False,'visible':True,'auto_switch':False,'allowed_sources':[]}})
        assert request('relay-current:gpt-native-test')[0]==403
        invoke('set_model_route_policy',{'model':'gpt-native-test','policy':{'enabled':True,'visible':True,'auto_switch':False,'allowed_sources':[]}})
        assert request('gpt-native-off')[0]==429
        assert [r['source'] for r in seen('gpt-native-off')]==['oa']
        checks.append('OpenAI subscription and API sources appear in native model order; actual HTTP/WS and compact bind correctly; opt-in same-model failover leaves global identity unchanged')
        checks.append('squeeze keeps compact mode; long press toggles persisted details; animated galaxy bars show API percentages and a smooth reveal')
        invoke('open_pet')
        model_button=pet.get_by_label('切换详细模式',exact=True)
        box=model_button.bounding_box();pet.mouse.move(box['x']+12,box['y']+12);pet.mouse.down();pet.wait_for_timeout(750);pet.mouse.up()
        expect(pet.locator('.pet-widget')).to_have_class('pet-widget ')
        assert not invoke('get_pet_preferences')['detailed']
        pet.wait_for_timeout(450)
        expect(pet.locator('.pet-mini')).to_be_visible()
        expect(pet.locator('.pet-mini')).to_contain_text('五小时≈25%')
        expect(pet.locator('.pet-mini')).to_contain_text('周额度≈75%')
        save_source(oa,short_weight=20,weekly_weight=20)
        expect(pet.locator('.pet-mini')).to_contain_text('五小时≈500%')
        expect(pet.locator('.pet-mini')).to_contain_text('周额度≈1500%')
        expect(pet.locator('.pet-mini').get_by_role('progressbar',name='五小时')).to_have_attribute('aria-valuenow','25')
        expect(pet.locator('.pet-mini').get_by_role('progressbar',name='周额度')).to_have_attribute('aria-valuenow','75')
        assert '含上次记录' in pet.locator('.pet-mini').get_by_role('progressbar',name='五小时').get_attribute('aria-valuetext')
        checks.append('quota labels stack to 500/1500 percent with editable 20x weights; bars remain 25/75 percent and cached readings stay visible; API detail has its own bar')
        # Two Business seats can share a workspace but own separate quotas.
        save_source(oa,included=False)
        for user in ['seat-one','seat-two']:
            claims=base64.urlsafe_b64encode(json.dumps({'sub':user}).encode()).decode().rstrip('=')
            account=dict(template,id='synthetic-'+user,name='Synthetic '+user,kind='chatgpt_oauth',
                auth_json={'tokens':{'account_id':'same-business-workspace','access_token':'synthetic.'+claims+'.synthetic'}},
                relay_base_url=None,relay_model_map={},relay_model_catalog=[],is_token_invalid=False,
                cached_quota={'five_hour_left':100,'five_hour_reset':'','weekly_left':100,'weekly_reset':'','plan_type':'team','is_valid_for_cli':True,
                    'primary_window_seconds':18000,'secondary_window_seconds':604800,'updated_at':(datetime.datetime.now(datetime.timezone.utc)-datetime.timedelta(hours=2)).isoformat()})
            content=base64.b64encode(json.dumps({'accounts':{account['id']:account}}).encode()).decode()
            result=invoke('bulk_import_accounts',{'files':[{'filename':'accounts.json','content_b64':content}]})
            assert not result['fatal'],result
        assert dashboard()['summary']['subscription_members']==2
        expect(pet.locator('.pet-mini')).to_contain_text('五小时≈200%')
        expect(pet.locator('.pet-mini')).to_contain_text('周额度≈200%')
        expect(pet.locator('.pet-mini').get_by_role('progressbar',name='五小时')).to_have_attribute('aria-valuenow','100')
        model_button.click()
        expect(pet.locator('.pet-pool')).to_contain_text('五小时≈200% / ≈200%')
        expect(pet.locator('.pet-pool').get_by_role('progressbar',name='五小时')).to_be_visible()
        pet.screenshot(path=str(out/'two-business-seats-200.png'),omit_background=True)
        model_button.click()
        checks.append('two distinct Business seats sharing a workspace display 200 percent in both modes, with a full visible bar; stale cached quotas remain labelled')
        expect(pet.locator('.pet-widget-reveal')).to_have_css('grid-template-rows','0px',timeout=2500)
        assert pet.locator('.pet-widget-reveal').evaluate('(el)=>el.getBoundingClientRect().height')<1
        pet.screenshot(path=str(out/'desktop-pet-compact.png'),omit_background=True)
        # Persisted, visible pet settings and synchronized independent windows.
        invoke('open_pet')
        page.get_by_role('button',name='桌宠与声音',exact=True).click()
        expect(page.get_by_label('捏一捏模式',exact=True)).to_be_checked()
        page.get_by_label('按模型切换形象',exact=True).check()
        page.get_by_label('固定形象',exact=True).select_option('claude')
        page.get_by_label('按模型切换形象',exact=True).uncheck()
        expect(pet.locator('.pet-character img')).to_have_attribute('src','/pets/claude.webp')
        art=pet.locator('.pet-character img')
        expect(art).to_have_css('transform','matrix(1, 0, 0, 1.75, 0, 0)')
        expect(page.locator('.pet-settings-preview img')).to_have_css('transform','matrix(1, 0, 0, 1.75, 0, 0)')
        checks.append('Claude-only presentation correction is shared by desktop and settings without modifying licensed artwork')
        box=pet.get_by_role('button',name='查看用量',exact=True).bounding_box()
        pet.mouse.move(box['x']+box['width']/2,box['y']+box['height']/2);pet.mouse.down()
        expect(pet.locator('.pet-character')).to_have_class('pet-character pet-held')
        pet.mouse.move(box['x']+box['width']/2+12,box['y']+box['height']/2+20)
        assert pet.locator('.pet-character').evaluate('(el)=>getComputedStyle(el).transform')!='none'
        pet.mouse.up();expect(pet.locator('.pet-character')).not_to_have_class('pet-character pet-held')
        expect(pet.locator('.pet-character')).to_have_css('transform','matrix(1, 0, 0, 1, 0, 0)')
        expect(art).to_have_css('transform','matrix(1, 0, 0, 1.75, 0, 0)')
        prefs=invoke('get_pet_preferences');assert prefs['silent_start'] and prefs['pinch']
        invoke('update_settings',{'settings':settings})
        assert invoke('get_pet_preferences')==prefs
        checks.append('pet settings visible, saved and synchronized; squeeze press/move/release deforms and rebounds; stale settings cannot overwrite preferences')
        # Real media decoding/playback, not a mock oscillator.
        sound=out/'synthetic.wav'
        with wave.open(str(sound),'wb') as w:w.setnchannels(1);w.setsampwidth(2);w.setframerate(8000);w.writeframes(b'\0\0'*1600)
        for which in ['press','release']:invoke('import_pet_sound',{'which':which,'path':str(sound)})
        expect(page.get_by_role('button',name='试听小黄鸭',exact=True)).to_be_enabled()
        data=invoke('get_pet_sound',{'which':'press'})
        assert data.startswith('data:audio/wav;base64,')
        decoded=pet.evaluate("async src=>{const a=new Audio(src);await new Promise((resolve,reject)=>{a.onloadedmetadata=resolve;a.onerror=reject});await a.play();return {duration:a.duration,paused:a.paused};}",data)
        assert decoded['duration']>0 and decoded['paused'] is False
        checks.append('local sound import, event reload and actual WebView audio decoding/playback pass; no synthesized duck substitute')
        # Read-only Codex observer: synthetic main metadata, then newer child metadata.
        root=Path.home()/'.switcher-pet/validation/profile/.codex/sessions/2026/11/06'
        root.mkdir(parents=True,exist_ok=True)
        main_path=root/'rollout-synthetic-main.jsonl';child_path=root/'rollout-synthetic-child.jsonl'
        assert not main_path.exists() and not child_path.exists()
        now=datetime.datetime.now(datetime.timezone.utc)+datetime.timedelta(seconds=2)
        def write_chat(path,source,model,at):
            path.write_text(json.dumps({'type':'session_meta','payload':{'id':'synthetic-main','source':source}})+'\n'+json.dumps({'type':'turn_context','timestamp':at.isoformat(),'payload':{'model':model}})+'\n',encoding='utf-8')
        try:
            page.get_by_label('按模型切换形象',exact=True).check()
            write_chat(main_path,'vscode','gemini-local-main',now)
            write_chat(child_path,{'subagent':{'parent_thread_id':'synthetic-main'}},'claude-child',now+datetime.timedelta(seconds=2))
            deadline=time.monotonic()+15
            observed=[]
            while time.monotonic()<deadline:
                current=invoke('get_pet_activity');observed.append(current)
                if current['model']=='gemini-local-main':break
                time.sleep(.25)
            else:raise AssertionError({'observer_did_not_update':observed[-3:],'preferences':invoke('get_pet_preferences'),'fixture_exists':main_path.exists()})
            expect(pet.locator('.pet-character img')).to_have_attribute('src','/pets/gemini.webp',timeout=15000)
            observed=invoke('get_pet_activity');assert observed['model']=='gemini-local-main',observed
            assert observed['origin']=='codex',observed
            time.sleep(3.5);assert invoke('get_pet_activity')['model']=='gemini-local-main'
            page.screenshot(path=str(out/'pet-settings.png'),full_page=True)
        finally:
            main_path.unlink(missing_ok=True);child_path.unlink(missing_ok=True)
        checks.append('Codex metadata observer follows real main turn without proxy traffic; newer subagent cannot steal skin; files remain unchanged')
        assert all(r['source']!='c' for r in calls)
        report={'checks':checks,'passed':len(checks),'fake_provider_requests':len(calls),'read_only_requests':len(reads),'real_accounts_used':0}
        (out/'routing-report.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
        print(json.dumps(report))
    finally:
        try: invoke('update_settings',{'settings':before})
        finally:
            browser.close()
            server.shutdown()
