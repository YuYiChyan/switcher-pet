"""Validate proxy metadata through installed Codex's real model/list parser. No inference/auth."""
import json, os, subprocess, sys, tempfile, threading
from pathlib import Path
from urllib.request import build_opener, ProxyHandler

port=int(sys.argv[1])
binaries=sorted((Path.home()/'AppData/Local/OpenAI/Codex/bin').glob('*/codex.exe'),key=lambda p:p.stat().st_mtime,reverse=True)
assert binaries,'Installed Codex executable not found'
binary=binaries[0]
with tempfile.TemporaryDirectory(prefix='switcher-catalog-') as root:
    version=subprocess.check_output([str(binary),'--version'],text=True).strip().split()[-1]
    catalog=json.load(build_opener(ProxyHandler({})).open(f'http://127.0.0.1:{port}/v1/models?client_version={version}',timeout=15))
    path=Path(root)/'models.json';path.write_text(json.dumps(catalog),encoding='utf8')
    env=dict(os.environ,CODEX_HOME=root)
    args=[str(binary),'-c','model_catalog_json='+json.dumps(str(path)),
          '-c','model_provider="synthetic"','-c','model_providers.synthetic={name="Synthetic",base_url="http://127.0.0.1:1/v1",wire_api="responses",requires_openai_auth=false}',
          'app-server']
    with open(Path(root)/'stderr.log','w+',encoding='utf8') as err:
        proc=subprocess.Popen(args,env=env,cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,text=True,encoding='utf8')
        timer=threading.Timer(25,proc.kill);timer.start()
        def send(value):proc.stdin.write(json.dumps(value)+'\n');proc.stdin.flush()
        def receive(wanted):
            for line in proc.stdout:
                value=json.loads(line)
                if value.get('id')==wanted:return value
            err.seek(0);raise RuntimeError(err.read()[-3000:])
        try:
            send({'id':1,'method':'initialize','params':{'clientInfo':{'name':'switcher_catalog_test','version':'1.0'},'capabilities':{'experimentalApi':True}}})
            init=receive(1);assert 'error' not in init,init
            send({'method':'initialized'})
            send({'id':2,'method':'model/list','params':{'limit':200,'includeHidden':False}})
            response=receive(2);assert 'error' not in response,response
            result=response['result']['data']
            visible=[row['model'] for row in result]
            expected=[row['slug'] for row in sorted(catalog['models'],key=lambda row:row.get('priority',50)) if row.get('visibility')=='list']
            missing=sorted(set(expected)-set(visible))
            print(json.dumps({'visible':visible,'missing':missing,'count':len(result),'real_requests':0}))
            assert not missing,missing
            assert [m for m in visible if m in expected]==expected,'Codex model/list order differs from catalog priority'
        finally:
            timer.cancel();proc.terminate();proc.wait(timeout=5)
