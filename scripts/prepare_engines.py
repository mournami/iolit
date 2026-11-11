#!/usr/bin/env python3
"""Build the frozen macOS engine. Windows bundles are prepared by `cargo run -p zbuild -- prepare-windows`."""
import argparse, os, pathlib, shutil, subprocess

ROOT=pathlib.Path(__file__).resolve().parents[1]
WORK=ROOT/'work'; RUNTIME=ROOT/'resources/runtime'

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
    parser=argparse.ArgumentParser();parser.add_argument('--platform',choices=['macos'],default='macos');parser.parse_args()
    RUNTIME.mkdir(parents=True,exist_ok=True)
    prepare_mac()
    shutil.copytree(ROOT/'licenses',RUNTIME/'licenses',dirs_exist_ok=True)

if __name__=='__main__':main()
