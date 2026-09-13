#!/usr/bin/env python3
"""Create deterministic release archives from two native Linux builds (allowlist only)."""
import argparse, gzip, hashlib, json, os, pathlib, re, shutil, struct, tarfile, tempfile
BINS=('sn-api','sn-sub','sn-bot','sn-worker','sn-admin','sn-node','sn-cabinet')
EDGE=('sn-node','sn-sub','sn-cabinet')
ROOT=pathlib.Path(__file__).resolve().parent.parent

def digest(path):
    h=hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''): h.update(block)
    return h.hexdigest()

def validate_binary(path,arch):
    with path.open('rb') as stream: head=stream.read(20)
    machine=62 if arch=='amd64' else 183
    if len(head)<20 or head[:6]!=b'\x7fELF\x02\x01' or struct.unpack('<H',head[18:20])[0]!=machine:
        raise ValueError(f'{path}: expected Linux ELF {arch}')

def build(args):
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-[a-zA-Z0-9][a-zA-Z0-9.-]*)?',args.version): raise ValueError('Invalid release tag')
    out=pathlib.Path(args.output).resolve(); out.mkdir(parents=True,exist_ok=True)
    builds=pathlib.Path(args.builds).resolve()
    for arch in ('amd64','arm64'):
        for name in BINS: validate_binary(builds/arch/name,arch)
    for arch in ('amd64','arm64'):
        with tempfile.TemporaryDirectory() as tmp:
            stage=pathlib.Path(tmp)
            for folder in ('bin','web','deploy','docs','db/migrations'): (stage/folder).mkdir(parents=True,exist_ok=True)
            for name in BINS: shutil.copy2(builds/arch/name,stage/'bin'/name); (stage/'bin'/name).chmod(0o755)
            shutil.copytree(ROOT/'web',stage/'web',dirs_exist_ok=True,ignore=shutil.ignore_patterns('*.map','.DS_Store','.impeccable','DESIGN.md','__pycache__'))
            shutil.copy2(ROOT/'web/miniapp-unavailable.html',stage/'web/app/index.html')
            for other in ('amd64','arm64'):
                for name in EDGE:
                    dest=stage/'web'/f'{name}-linux-{other}'
                    shutil.copy2(builds/other/name,dest); dest.chmod(0o755)
                    (dest.with_name(dest.name+'.sha256')).write_text(digest(dest)+'\n')
                    asset=out/dest.name
                    shutil.copy2(dest,asset); asset.with_name(asset.name+'.sha256').write_text(digest(asset)+'\n')
            for name in ('install.sh','update.sh','Makefile','LICENSE','LICENSE.md','THIRD_PARTY_NOTICES.md'): shutil.copy2(ROOT/name,stage/name)
            for name in ('installer.py','migrate.sh','pg-env.py'): shutil.copy2(ROOT/'deploy'/name,stage/'deploy'/name)
            migrations=sorted((ROOT/'db/migrations').glob('[0-9][0-9][0-9]_*.sql'))
            if not migrations or migrations[0].name!='001_init.sql': raise ValueError('Migration sources are missing')
            for path in migrations: shutil.copy2(path,stage/'db/migrations'/path.name)
            for name in ('installation.md','installation-releases.md'): shutil.copy2(ROOT/'docs'/name,stage/'docs'/name)
            manifest={'version':args.version,'arch':arch,'layout':1,'glibc_min':'2.35',
                      'files':{str(p.relative_to(stage)):digest(p) for p in sorted(stage.rglob('*')) if p.is_file()}}
            (stage/'RELEASE.json').write_text(json.dumps(manifest,indent=2)+'\n')
            archive=out/f'stealthnet-{args.version}-linux-{arch}.tar.gz'
            with archive.open('wb') as raw, gzip.GzipFile(fileobj=raw,mode='wb',filename='',mtime=0) as gz, tarfile.open(fileobj=gz,mode='w',dereference=True) as tar:
                for path in sorted(stage.rglob('*')):
                    if not path.is_file(): continue
                    info=tar.gettarinfo(str(path),str(path.relative_to(stage)))
                    info.uid=info.gid=0; info.uname=info.gname='root'; info.mtime=0
                    info.mode=0o755 if path.parent.name=='bin' or path.suffix=='.sh' else 0o644
                    with path.open('rb') as file: tar.addfile(info,file)
            archive.with_name(archive.name+'.sha256').write_text(digest(archive)+'\n')
            print(f'{archive.name}: {archive.stat().st_size//1024//1024} MiB')
    (out/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in sorted(out.iterdir()) if p.is_file() and not p.name.endswith('.sha256') and p.name!='SHA256SUMS'))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('--version',required=True); p.add_argument('--builds',required=True); p.add_argument('--output',required=True)
    build(p.parse_args())
