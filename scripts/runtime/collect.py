"""Assemble a relocatable musl runtime and matching redistribution sources."""
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import urllib.request

OUT = Path('/out')
ROOT = Path('/build/runtime')
SOURCES = Path('/build/corresponding-sources')
ROOT.mkdir()
SOURCES.mkdir()
arch = os.uname().machine
loader = Path('/lib') / ('ld-musl-' + arch + '.so.1')
binary = Path('/build/stage/usr/bin/openconnect')
library_path = '/build/stage/usr/lib:/usr/lib:/lib'

def run(*args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs)

def copy(source, destination):
    destination = ROOT / destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if destination.name.startswith('ld-musl-') or destination.name == 'openconnect' else 0o644)

copy(binary, 'bin/openconnect')
listing = run(str(loader), '--library-path', library_path, '--list', str(binary))
paths = {Path(path) for path in re.findall(r'(/[^\s()]+)', listing)}
paths.add(loader)
for path in sorted(paths):
    copy(path, 'lib/' + path.name)
copy(loader, 'lib/libc.musl-' + arch + '.so.1')
copy('/etc/ssl/certs/ca-certificates.crt', 'ca-certificates.crt')
with tarfile.open('/build/vpnc-scripts.tar.gz') as archive:
    (ROOT / 'vpnc-script').write_bytes(archive.extractfile('vpnc-script').read())
    (ROOT / 'licenses/vpnc-scripts').mkdir(parents=True)
    (ROOT / 'licenses/vpnc-scripts/COPYING').write_bytes(archive.extractfile('COPYING').read())
(ROOT / 'vpnc-script').chmod(0o755)
shutil.copyfile('/build/openconnect.tar.gz', SOURCES / 'openconnect-9.12.tar.gz')
shutil.copyfile('/build/vpnc-scripts.tar.gz', SOURCES / 'vpnc-scripts-ce9e961.tar.gz')
shutil.copyfile('/build/build-recipe.Dockerfile', SOURCES / 'Dockerfile')
shutil.copyfile(__file__, SOURCES / 'collect.py')

# APK's signed package metadata records each exact upstream recipe commit.
packages = {}
for block in Path('/lib/apk/db/installed').read_text().split('\n\n'):
    fields = dict(line.split(':', 1) for line in block.splitlines() if ':' in line and len(line.split(':', 1)[0]) == 1)
    if 'P' in fields and 'V' in fields:
        packages[fields['P'] + '-' + fields['V']] = fields
owners = {}
for path in sorted(paths | {Path('/etc/ssl/certs/ca-certificates.crt')}):
    if str(path).startswith('/build/stage/'):
        continue
    owner = run('apk', 'info', '--who-owns', str(path)).strip().split(' is owned by ')[-1]
    package = packages[owner]
    owners[package.get('o', package['P'])] = package

def recipe(package):
    origin = package.get('o', package['P'])
    commit = package['c']
    for repository in ['main', 'community']:
        url = ('https://gitlab.alpinelinux.org/api/v4/projects/1/repository/archive.tar.gz'
               '?sha=%s&path=%s/%s' % (commit, repository, origin))
        try:
            with urllib.request.urlopen(url, timeout=90) as response:
                data = response.read()
        except urllib.error.HTTPError as error:
            if error.code == 404:
                continue
            raise
        directory = SOURCES / 'recipes' / origin
        directory.mkdir(parents=True)
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            prefix = None
            for member in archive.getmembers():
                if member.name.endswith('/' + repository + '/' + origin + '/APKBUILD'):
                    prefix = member.name[:-len('APKBUILD')]
                    break
            if prefix is None:
                continue
            for member in archive.getmembers():
                if member.name.startswith(prefix) and member.isfile():
                    relative = Path(member.name[len(prefix):])
                    if relative.is_absolute() or '..' in relative.parts:
                        raise RuntimeError('Unsafe upstream recipe path')
                    target = directory / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(archive.extractfile(member).read())
        return directory
    raise RuntimeError('Cannot locate recipe for ' + origin)

for origin, package in sorted(owners.items()):
    print('Collecting corresponding sources:', origin, package['V'], flush=True)
    directory = recipe(package)
    destination = SOURCES / 'upstream' / origin
    destination.mkdir(parents=True)
    # Prefer Alpine's archived inputs over occasionally unavailable upstreams.
    # abuild still verifies them against the exact recipe checksums below.
    checksums = re.search(r'(?:sha512sums|sha256sums|b2sums)=([\"\'])(.*?)\1',
                          (directory / 'APKBUILD').read_text(), re.S)
    if checksums:
        for line in checksums.group(2).strip().splitlines():
            fields = line.split()
            if len(fields) != 2:
                continue
            name = fields[1]
            if (directory / name).exists() or Path(name).name != name:
                continue
            try:
                url = 'https://distfiles.alpinelinux.org/distfiles/v3.23/' + name
                with urllib.request.urlopen(url, timeout=45) as response:
                    (destination / name).write_bytes(response.read())
            except (urllib.error.HTTPError, TimeoutError):
                pass
    environment = dict(os.environ, SRCDEST=str(destination))
    subprocess.run(['abuild', '-F', '-d', 'fetch', 'verify'], cwd=directory, env=environment, check=True)
    notices = ROOT / 'licenses' / origin
    notices.mkdir(parents=True)
    shutil.copyfile(directory / 'APKBUILD', notices / 'APKBUILD')

# Preserve full sources separately and include their actual license texts here.
for archive_path in sorted(SOURCES.rglob('*')):
    if not archive_path.is_file():
        continue
    try:
        archive = tarfile.open(archive_path)
    except (tarfile.TarError, OSError):
        continue
    origin = archive_path.parent.name if archive_path.parent != SOURCES else archive_path.name.split('-')[0]
    with archive:
        for member in archive:
            name = Path(member.name).name.lower()
            if not member.isfile() or member.size > 2000000 or member.name.count('/') > 3:
                continue
            if not (name.startswith('copying') or name in ['license', 'license.txt', 'licence', 'copyright', 'notice']):
                continue
            destination = ROOT / 'licenses' / origin / (hashlib.sha256(member.name.encode()).hexdigest()[:12] + '-' + Path(member.name).name[:80])
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(archive.extractfile(member).read())

version = run(str(loader), '--library-path', library_path, str(binary), '--version').splitlines()[0]
metadata = {
    'schema': 1, 'arch': arch, 'openconnect_version': version,
    'loader': 'lib/' + loader.name,
    'vpnc_commit': 'ce9e961bd0f6b867e1c7c35f78f6fb973f6ff101',
    'packages': [{'name': p['P'], 'version': p['V'], 'origin': o, 'license': p['L'], 'commit': p['c']} for o, p in sorted(owners.items())],
}
notice = ('OpenConnect 9.12 is built from the unmodified upstream source with the flags in the paired sources archive.\n'
          'vpnc-script is unmodified upstream commit ' + metadata['vpnc_commit'] + '.\n'
          'Dynamic libraries and the musl loader are from Alpine Linux 3.23 signed packages.\n'
          'License texts and exact package recipes are in licenses/. Corresponding sources,\n'
          'upstream archives and build recipes are distributed alongside this CLI as\n'
          'hku-VERSION-runtime-sources-TARGET.tar.gz (or runtime-sources.tar.gz in build output).\n'
          'These components keep their upstream licenses and are not part of the Rust CLI license.\n')
(ROOT / 'THIRD-PARTY.txt').write_text(notice)
(SOURCES / 'README.txt').write_text(notice + '\nRebuild the runtime with scripts/runtime/Dockerfile and collect.py from the project.\n'
    'recipes/ contains the exact Alpine APKBUILDs and patches. upstream/ contains\n'
    'all fetched and checksum-verified source inputs for those recipes. Use Alpine abuild\n'
    'on the recorded architecture to rebuild the corresponding library packages.\n')
metadata['files'] = {}
for path in sorted(ROOT.rglob('*')):
    if path.is_file():
        data = path.read_bytes()
        relative = path.relative_to(ROOT).as_posix()
        metadata['files'][relative] = {'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data), 'mode': path.stat().st_mode & 0o777}
(ROOT / 'manifest.json').write_text(json.dumps(metadata, indent=2) + '\n')
(SOURCES / 'runtime-manifest.json').write_text(json.dumps(metadata, indent=2) + '\n')

def pack(source, destination, manifest_first=False):
    with open(destination, 'wb') as output, gzip.GzipFile(fileobj=output, mode='wb', filename='', mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w') as archive:
            files = sorted(path for path in source.rglob('*') if path.is_file())
            if manifest_first:
                files.remove(source / 'manifest.json')
                files.insert(0, source / 'manifest.json')
            for path in files:
                data = path.read_bytes()
                info = tarfile.TarInfo(path.relative_to(source).as_posix())
                info.size = len(data)
                info.mode = path.stat().st_mode & 0o777
                archive.addfile(info, io.BytesIO(data))

pack(ROOT, OUT / 'runtime.tar.gz', True)
pack(SOURCES, OUT / 'runtime-sources.tar.gz')
shutil.copyfile(ROOT / 'manifest.json', OUT / 'runtime-manifest.json')
print(version, 'runtime bytes:', (OUT / 'runtime.tar.gz').stat().st_size, flush=True)
