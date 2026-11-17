#!/usr/bin/env python3
"""Reviewable module ZIP assembler; NO installer, Git mutation, or release qualification.

Consumes exact upstream module scripts plus already cross-compiled executables, verified
ELF architectures and bundled WebUI assets. No opaque KernelSU profile helper.
"""
import argparse
import hashlib
import json
import os
import re
import stat
import struct
import sys
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED

ABI = {
    'arm64-v8a': ('aarch64-linux-android', 2, 183),
    'armeabi-v7a': ('armv7-linux-androideabi', 1, 40),
}
MAX_FILE = 12 * 1024 * 1024
TIMESTAMP = (2020, 1, 1, 0, 0, 0)

class Refusal(ValueError):
    pass

def regular(path: Path, *, max_size=MAX_FILE) -> bytes:
    p = Path(path)
    if p.is_symlink() or not p.is_file():
        raise Refusal(f'missing, unsafe or symlinked file: {p}')
    if p.stat().st_size <= 0 or p.stat().st_size > max_size:
        raise Refusal(f'empty or oversized file: {p}')
    return p.read_bytes()

def elf(data: bytes, elf_class: int, machine: int, name: str) -> None:
    if len(data) < 20 or data[:4] != b'\x7fELF' or data[4] != elf_class or data[5] != 1:
        raise Refusal(f'ELF class/endianness invalid: {name}')
    if struct.unpack_from('<H', data, 18)[0] != machine:
        raise Refusal(f'ELF architecture mismatched: {name}')

def web_files(webroot: Path):
    if webroot.is_symlink() or not webroot.is_dir():
        raise Refusal('missing safe webroot')
    root = webroot.resolve()
    files = {}
    for p in sorted(root.rglob('*')):
        if p.is_symlink(): raise Refusal('WebUI symlink forbidden')
        if p.is_dir(): continue
        if not p.is_file() or not p.resolve().is_relative_to(root):
            raise Refusal('WebUI path escape')
        relative = p.relative_to(root).as_posix()
        if not re.fullmatch(r'[\w./-]{1,160}', relative, re.ASCII) or '..' in Path(relative).parts:
            raise Refusal(f'invalid WebUI filename {relative}')
        if relative.endswith('.map') or relative.startswith('.') or '/.' in relative:
            raise Refusal(f'unexpected debug/hidden WebUI asset {relative}')
        files['webroot/' + relative] = (regular(p, max_size=2*1024*1024), False)
    if 'webroot/index.html' not in files:
        raise Refusal('built WebUI index.html missing')
    html = files['webroot/index.html'][0]
    if b'<html' not in html.lower(): raise Refusal('WebUI index content invalid')
    if b'node_modules' in html: raise Refusal('WebUI has unbundled dependency reference')
    return files

def collect(repo: Path, webroot: Path, built: Path) -> dict:
    repo = repo.resolve(strict=True)
    sources = {
        'LICENSE': repo / 'LICENSE',
        'module.prop': repo / 'module/module.prop',
        'customize.sh': repo / 'module/customize.sh',
        'post-fs-data.sh': repo / 'module/post-fs-data.sh',
        'META-INF/com/google/android/update-binary': repo / 'module/META-INF/com/google/android/update-binary',
        'META-INF/com/google/android/updater-script': repo / 'module/META-INF/com/google/android/updater-script',
    }
    out = {name: (regular(src), name.endswith('.sh') or name.endswith('update-binary'))
           for name, src in sources.items()}
    properties = out['module.prop'][0].decode('utf-8')
    if not re.search(r'^id=zygisk-detach\s*$', properties, flags=re.M):
        raise Refusal('unexpected module ID')
    # An unqualified experimental fork MUST NOT advertise upstream updateJson:
    # KernelSU/Magisk could otherwise offer an incompatible official module.
    metadata_lines=[]
    seen_version=False
    for line in properties.splitlines():
        if line.startswith('updateJson='): continue
        if line.startswith('version='):
            line = line + '-unqualified'
            seen_version=True
        metadata_lines.append(line)
    if not seen_version: raise Refusal('missing module version')
    out['module.prop'] = (('\n'.join(metadata_lines)+'\n').encode('utf-8'), False)
    if b'ksu_profile' in out['customize.sh'][0]:
        raise Refusal('undocumented KernelSU helper is still required')
    out.update(web_files(webroot))
    for abi,(triple,klass,machine) in ABI.items():
        so = regular(built / f'zygisk/libs/{abi}/libzygisk_detach.so')
        binary = regular(built / f'cli/target/{triple}/release/cli')
        elf(so,klass,machine,f'zygisk/{abi}.so')
        elf(binary,klass,machine,f'bin/{abi}/detach')
        out[f'zygisk/{abi}.so'] = (so, False)
        out[f'bin/{abi}/detach'] = (binary, True)
    return out

def package(repo: Path, webroot: Path, built: Path, output: Path) -> dict:
    content = collect(repo, webroot, built)
    output = output.absolute()
    if output.exists() or output.is_symlink() or output.suffix != '.zip':
        raise Refusal('output must be a new ZIP file; never overwrite')
    if output.resolve().is_relative_to(repo.resolve()):
        raise Refusal('output must be outside source repository')
    if not output.parent.is_dir() or output.parent.is_symlink():
        raise Refusal('unsafe output parent')
    fd = os.open(output, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        with os.fdopen(fd, 'wb') as f, ZipFile(f, 'w', compression=ZIP_DEFLATED, compresslevel=9) as z:
            for name, (raw, executable) in sorted(content.items()):
                info = ZipInfo(name, TIMESTAMP)
                info.compress_type = ZIP_DEFLATED
                info.create_system = 3
                info.external_attr = ((stat.S_IFREG | (0o755 if executable else 0o644)) << 16)
                z.writestr(info, raw)
            f.flush()
        with ZipFile(output) as z:
            if z.testzip() or set(z.namelist()) != set(content):
                raise Refusal('ZIP internal verification failed')
            for name, (raw, _) in content.items():
                if hashlib.sha256(z.read(name)).digest() != hashlib.sha256(raw).digest():
                    raise Refusal('ZIP payload hash mismatch')
        return {'sha256':hashlib.sha256(output.read_bytes()).hexdigest(),
                'members':{name:hashlib.sha256(raw).hexdigest()
                           for name,(raw,_) in sorted(content.items())},
                'installable_structure_validated':True,
                'runtime_or_device_verified':False}
    except BaseException:
        output.unlink(missing_ok=True)
        raise

def main() -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', type=Path, default=Path.cwd())
    p.add_argument('--webroot', type=Path, required=True)
    p.add_argument('--build-root', type=Path, default=Path.cwd())
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    try:
        receipt = package(args.repo,args.webroot,args.build_root,args.output)
        print(json.dumps(receipt,sort_keys=True,indent=2))
        return 0
    except (OSError,Refusal,UnicodeError) as exc:
        print('MODULE_PACKAGE_REFUSED: '+str(exc), file=sys.stderr)
        return 2
if __name__=='__main__': raise SystemExit(main())
