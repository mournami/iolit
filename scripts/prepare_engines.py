#!/usr/bin/env python3
"""Fetch pinned upstream data. Never execute batch files or upstream installers."""
import argparse, hashlib, json, os, pathlib, platform, re, shutil, subprocess, tarfile, urllib.request

ROOT=pathlib.Path(__file__).resolve().parents[1]
WORK=ROOT/'work'; RUNTIME=ROOT/'resources/runtime'

def download(url,path,digest=None):
    path.parent.mkdir(parents=True,exist_ok=True)
    if not path.exists():
        with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'ZapretDesktop-build'})) as r:
            path.write_bytes(r.read())
    actual=hashlib.sha256(path.read_bytes()).hexdigest()
    if digest and actual!=digest: raise RuntimeError(f'SHA256 mismatch: {path}')
    return actual

def import_strategy(path):
    text=path.read_text(encoding='utf-8-sig').replace('^\r\n',' ').replace('^\n',' ')
    match=re.search(r'(?im)^\s*start\s+[^\n]*?winws\.exe"?\s+([^\n]+)',text)
    if not match: raise RuntimeError(f'No winws invocation: {path.name}')
    args=re.findall(r'(?:[^\s"]|"[^"]*")+',match[1])
    args=[a.replace('"','') for a in args]
    groups=[[]]
    for arg in args:
        if arg=='--new': groups.append([])
        else: groups[-1].append(arg)
    out=[]
    for group in groups:
        if any(a.startswith(('--filter-tcp=%GameFilter','--filter-udp=%GameFilter')) for a in group): continue
        converted=[]
        for a in group:
            a=re.sub(r',?%GameFilter(?:TCP|UDP)%','',a)
            a=a.replace('%BIN%','@bin/').replace('%LISTS%','@lists/').replace('\\','/')
            if '%' in a or not a.startswith('--'): raise RuntimeError(f'Unknown syntax in {path.name}: {a}')
            if not re.fullmatch(r'--[a-z0-9-]+(?:=.*)?',a): raise RuntimeError(f'Invalid argument: {a}')
            converted.append(a)
        if out: out.append('--new')
        out.extend(converted)
    ident=re.sub(r'[^a-z0-9]+','-',path.stem.lower()).strip('-')
    return {'id':ident,'name':path.stem,'description':'Импортировано из flowseal; профили игры отключены по умолчанию.','supported':True,'arguments':out}

def prepare_windows(lock):
    archive=WORK/'flowseal.tar.gz';download(lock['url'],archive,lock['sha256'])
    extracted=WORK/'flowseal';extracted.mkdir(parents=True,exist_ok=True)
    with tarfile.open(archive) as t:t.extractall(extracted,filter='data')
    roots=list(extracted.rglob('general.bat'))
    if len(roots)!=1: raise RuntimeError('Unexpected flowseal archive structure')
    source=roots[0].parent;target=RUNTIME/'windows';target.mkdir(parents=True,exist_ok=True)
    for name in ['bin','lists']: shutil.copytree(source/name,target/name,dirs_exist_ok=True)
    # service.bat creates these at first launch. We never run batch files.
    for name in ['list-general-user.txt','list-exclude-user.txt','ipset-exclude-user.txt']:
        (target/'lists'/name).touch(exist_ok=True)
    catalog=[import_strategy(p) for p in sorted(source.glob('general*.bat'))]
    catalog.sort(key=lambda x:(x['id']!='general',x['id']))
    for s in catalog:
        for arg in s['arguments']:
            if '=@' in arg and not (target/arg.split('=@',1)[1]).is_file(): raise RuntimeError(f'Missing payload: {arg}')
    (target/'strategies.json').write_text(json.dumps(catalog,ensure_ascii=False,indent=2)+'\n')
    shutil.copy2(ROOT/'licenses/Flowseal-MIT.txt',target/'LICENSE-flowseal.txt')
    print(f'Imported {len(catalog)} pinned Windows strategies')

def go_binary():
    configured=os.environ.get('ZAPRET_GO')
    if configured:return configured
    local=WORK/'toolchain/go/bin/go'
    if local.is_file():return str(local)
    found=shutil.which('go')
    if not found:raise RuntimeError('Install Go 1.26+ or set ZAPRET_GO')
    return found

def prepare_mac():
    source=ROOT/'engines/macos';target=RUNTIME/'macos';target.mkdir(parents=True,exist_ok=True)
    go=go_binary()
    for command in ['zapretd','zaprctl']:
        subprocess.run([go,'build','-trimpath','-ldflags=-s -w -X main.version=desktop-0.1.0','-o',str(target/command),'./cmd/'+command],cwd=source,check=True)
    for folder in ['strategies','lists','fakes']:shutil.copytree(source/folder,target/'data'/folder,dirs_exist_ok=True)
    shutil.copy2(source/'LICENSE',target/'LICENSE-macos-engine.txt')
    print('Built native macOS packet engine and control tool')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--platform',choices=['macos','windows','all'],default='macos' if platform.system()=='Darwin' else 'windows');args=parser.parse_args()
    lock=json.loads((ROOT/'resources/upstream.lock.json').read_text());RUNTIME.mkdir(parents=True,exist_ok=True)
    if args.platform in ('windows','all'):prepare_windows(lock['flowseal'])
    if args.platform in ('macos','all'):prepare_mac()
    shutil.copytree(ROOT/'licenses',RUNTIME/'licenses',dirs_exist_ok=True)

if __name__=='__main__':main()
