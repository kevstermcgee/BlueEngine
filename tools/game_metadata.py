#!/usr/bin/env python3
"""Portable, bounded game metadata. Inspection never runs the submitted game.

This file is intentionally stdlib-only and can be vendored with a publisher.
Metadata describes a file; it does not authenticate its author or source claims.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import struct
import subprocess
import tempfile
import zipfile

FORMAT = 'blueengine.game'
MANIFEST = 'blueengine.game.json'
MAX_PAYLOAD = 60 * 1024
MAGIC = b'BLUEENGINE_GAME_METADATA_V1\n'
FOOTER_SIZE = 32 + 8 + len(MAGIC)
BAT_BEGIN = b'@rem BLUEENGINE_GAME_METADATA_V1_BEGIN'
BAT_END = b'@rem BLUEENGINE_GAME_METADATA_V1_END'
MAX_DIRECTORY = 16 * 1024 * 1024
MAX_ENTRIES = 20000
_AUTO = object()


class MetadataError(ValueError):
    pass


def _text(value, limit, name, *, required=False):
    if not isinstance(value, str) or len(value) > limit or (required and not value.strip()):
        raise MetadataError(f'{name} must be {"nonempty " if required else ""}text of at most {limit} characters')
    if '\0' in value or any(0xD800 <= ord(c) <= 0xDFFF for c in value):
        raise MetadataError(f'{name} contains invalid text')


def _generic(value, depth=0):
    if depth > 24:
        raise MetadataError('metadata nesting exceeds 24 levels')
    if value is None or isinstance(value, bool):
        return
    if isinstance(value, int) and abs(value) <= 9007199254740991:
        return
    if isinstance(value, str):
        _text(value, 16000, 'metadata string')
        return
    if isinstance(value, list) and len(value) <= 256:
        for item in value:
            _generic(item, depth + 1)
        return
    if isinstance(value, dict) and len(value) <= 256:
        for key, item in value.items():
            if not isinstance(key, str) or not re.fullmatch(r'[A-Za-z0-9_.-]{1,128}', key):
                raise MetadataError('metadata object keys must be short ASCII identifiers')
            _generic(item, depth + 1)
        return
    raise MetadataError('metadata supports bounded objects, arrays, text and safe integers')


def _relative(value):
    _text(value, 512, 'source.game_path', required=True)
    if value == '.':
        return
    for part in value.split('/'):
        if (not part or part in ('.', '..') or part.rstrip(' .') != part or
                re.search(r'[\\:*?"<>|\x00-\x1f\x7f]', part) or
                re.fullmatch(r'(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?', part, re.I)):
            raise MetadataError('source.game_path must be a portable relative path')


def validate(document):
    _generic(document)
    if not isinstance(document, dict) or document.get('format') != FORMAT or type(document.get('schema_version')) is not int or document['schema_version'] != 1:
        raise MetadataError('expected blueengine.game schema_version 1')
    game = document.get('game')
    if not isinstance(game, dict):
        raise MetadataError('game must be an object')
    for name, limit in (('slug', 100), ('title', 160), ('version', 128), ('description', 16000), ('controls', 8000)):
        _text(game.get(name), limit, f'game.{name}', required=name in ('slug', 'title'))
    if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*', game['slug']):
        raise MetadataError('game.slug must be a lowercase portable slug')
    for name in ('authors', 'tags'):
        values = game.get(name)
        if not isinstance(values, list) or len(values) > 32:
            raise MetadataError(f'game.{name} must be an array of at most 32 items')
        for value in values:
            _text(value, 160, f'game.{name} item', required=True)
    for name in ('source', 'engine'):
        if name not in document:
            raise MetadataError(f'{name} must be present (null when unknown)')
        binding = document[name]
        if binding is None:
            continue
        if not isinstance(binding, dict):
            raise MetadataError(f'{name} must be null or an object')
        repository, commit = binding.get('repository'), binding.get('commit')
        if not isinstance(repository, str) or len(repository) > 200 or not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository):
            raise MetadataError(f'{name}.repository must be owner/repository')
        if not isinstance(commit, str) or not re.fullmatch(r'(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})', commit):
            raise MetadataError(f'{name}.commit must be a complete Git object id')
        if name == 'source':
            _relative(binding.get('game_path'))
            if 'tree' in binding and (not isinstance(binding['tree'], str) or not re.fullmatch(r'(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})', binding['tree'])):
                raise MetadataError('source.tree must be a complete Git tree object id')
            if 'dirty' in binding and not isinstance(binding['dirty'], bool):
                raise MetadataError('source.dirty must be boolean')
    if 'wiki' in document and not isinstance(document['wiki'], dict):
        raise MetadataError('wiki must be an object')
    if 'lineage' in document:
        if not isinstance(document['lineage'], dict):
            raise MetadataError('lineage must be an object')
        for name in ('root_game', 'parent_game', 'parent_revision', 'revision_id'):
            if document['lineage'].get(name) is not None:
                _text(document['lineage'][name], 160, 'lineage.' + name)
    return document


def canonical_payload(document):
    validate(document)
    payload = json.dumps(document, sort_keys=True, ensure_ascii=False, separators=(',', ':'), allow_nan=False).encode('utf-8')
    if len(payload) > MAX_PAYLOAD:
        raise MetadataError(f'game metadata exceeds {MAX_PAYLOAD} bytes')
    return payload


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise MetadataError('duplicate JSON metadata key')
        result[key] = value
    return result


def decode_payload(payload):
    if not 0 < len(payload) <= MAX_PAYLOAD:
        raise MetadataError('invalid metadata payload size')
    try:
        document = json.loads(payload.decode('utf-8'), object_pairs_hook=_pairs)
        if canonical_payload(document) != payload:
            raise MetadataError('metadata JSON is not canonical')
        return document
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise MetadataError('invalid metadata JSON') from error


def make_envelope(document):
    payload = canonical_payload(document)
    return payload + hashlib.sha256(payload).digest() + struct.pack('<Q', len(payload)) + MAGIC


def _envelope_at(stream, end):
    if end < FOOTER_SIZE:
        return None
    stream.seek(end - FOOTER_SIZE)
    footer = stream.read(FOOTER_SIZE)
    if footer[-len(MAGIC):] != MAGIC:
        return None
    length = struct.unpack('<Q', footer[32:40])[0]
    if not 0 < length <= MAX_PAYLOAD or length + FOOTER_SIZE > end:
        raise MetadataError('invalid embedded metadata length')
    start = end - FOOTER_SIZE - length
    stream.seek(start)
    payload = stream.read(length)
    if hashlib.sha256(payload).digest() != footer[:32]:
        raise MetadataError('embedded metadata checksum failed')
    return decode_payload(payload), start


def _pe(stream, size):
    stream.seek(0)
    dos = stream.read(64)
    if len(dos) != 64 or dos[:2] != b'MZ':
        raise MetadataError('expected a Windows PE executable')
    offset = struct.unpack_from('<I', dos, 60)[0]
    if not 64 <= offset <= 1024 * 1024 or offset + 24 > size:
        raise MetadataError('invalid PE header offset')
    stream.seek(offset)
    header = stream.read(24)
    if header[:4] != b'PE\0\0':
        raise MetadataError('invalid PE signature')
    count, optional_size = struct.unpack_from('<H', header, 6)[0], struct.unpack_from('<H', header, 20)[0]
    if not 1 <= count <= 96 or not 136 <= optional_size <= 4096:
        raise MetadataError('invalid PE section table')
    optional = stream.read(optional_size)
    if len(optional) != optional_size:
        raise MetadataError('truncated PE optional header')
    kind = struct.unpack_from('<H', optional)[0]
    directory, number = (96, 92) if kind == 0x10b else (112, 108) if kind == 0x20b else (None, None)
    if directory is None or optional_size < directory + 40 or struct.unpack_from('<I', optional, number)[0] < 5:
        raise MetadataError('unsupported PE optional header')
    certificate, certificate_size = struct.unpack_from('<II', optional, directory + 32)
    section_table_end = offset + 24 + optional_size + 40 * count
    end, ranges = section_table_end, []
    for _ in range(count):
        section = stream.read(40)
        if len(section) != 40:
            raise MetadataError('truncated PE section table')
        length, start = struct.unpack_from('<II', section, 16)
        if length:
            if start < section_table_end or start + length > size:
                raise MetadataError('invalid PE section extent')
            ranges.append((start, start + length))
            end = max(end, start + length)
    ranges.sort()
    if any(left[1] > right[0] for left, right in zip(ranges, ranges[1:])):
        raise MetadataError('overlapping PE sections')
    if bool(certificate) != bool(certificate_size):
        raise MetadataError('incomplete PE certificate table')
    if certificate:
        if certificate % 8 or certificate < end or certificate + certificate_size != size or certificate_size > MAX_DIRECTORY:
            raise MetadataError('invalid PE certificate extent')
        current = certificate
        while current < size:
            stream.seek(current)
            entry = stream.read(8)
            if len(entry) != 8:
                raise MetadataError('truncated PE certificate')
            length, revision, _kind = struct.unpack('<IHH', entry)
            if length < 8 or revision not in (0x100, 0x200):
                raise MetadataError('invalid PE certificate')
            current += (length + 7) & ~7
            if current > size:
                raise MetadataError('invalid PE certificate size')
    return end, certificate


def _open(path):
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0)
    fd = os.open(path, flags)
    if not stat.S_ISREG(os.fstat(fd).st_mode):
        os.close(fd)
        raise MetadataError('metadata input must be a regular file')
    return os.fdopen(fd, 'rb')


def _bat(stream):
    stream.seek(0)
    data = stream.read(MAX_PAYLOAD * 2)
    lines = data.splitlines(keepends=True)
    if not lines or lines[0].rstrip(b'\r\n') != BAT_BEGIN:
        return None
    encoded, consumed = [], len(lines[0])
    for line in lines[1:]:
        consumed += len(line)
        text = line.rstrip(b'\r\n')
        if text == BAT_END:
            try:
                payload = base64.b64decode(b''.join(encoded), validate=True)
            except ValueError as error:
                raise MetadataError('invalid batch metadata base64') from error
            if BAT_BEGIN in data[consumed:] or BAT_END in data[consumed:]:
                raise MetadataError('multiple batch metadata blocks')
            return decode_payload(payload), consumed
        if not text.startswith(b'@rem ') or not re.fullmatch(rb'[A-Za-z0-9+/=]{1,76}', text[5:]):
            raise MetadataError('invalid inert batch metadata line')
        encoded.append(text[5:])
    raise MetadataError('unterminated batch metadata block')


def _zip_bounds(stream, size):
    stream.seek(max(0, size - 65557))
    tail = stream.read(65557)
    found = None
    for index in range(len(tail) - 22, -1, -1):
        if tail[index:index + 4] == b'PK\x05\x06' and index + 22 + struct.unpack_from('<H', tail, index + 20)[0] == len(tail):
            found = index
            break
    if found is None:
        raise MetadataError('invalid ZIP end record')
    # zipfile chooses the last signature without validating its comment length.
    # A later fake record in the comment must not override the directory whose
    # allocation limits we just validated (including on sparse multi-GB inputs).
    if tail.rfind(b'PK\x05\x06') != found:
        raise MetadataError('ambiguous ZIP end record inside archive comment')
    disk, start_disk, disk_count, count, directory_size, directory_offset = struct.unpack_from('<HHHHII', tail, found + 4)
    if disk or start_disk or disk_count != count:
        raise MetadataError('multi-disk ZIP is unsupported')
    if count == 65535 or directory_size == 0xffffffff or directory_offset == 0xffffffff:
        end_offset = size - len(tail) + found
        stream.seek(end_offset - 20)
        locator = stream.read(20)
        if len(locator) != 20 or locator[:4] != b'PK\x06\x07':
            raise MetadataError('invalid ZIP64 locator')
        _signature, disk, record_offset, disks = struct.unpack('<4sIQI', locator)
        if disk or disks != 1 or record_offset + 56 > end_offset - 20:
            raise MetadataError('invalid ZIP64 extent')
        stream.seek(record_offset)
        record = stream.read(56)
        if record[:4] != b'PK\x06\x06':
            raise MetadataError('invalid ZIP64 record')
        record_size = struct.unpack_from('<Q', record, 4)[0]
        disk, start_disk, disk_count, count, directory_size, directory_offset = struct.unpack_from('<IIQQQQ', record, 16)
        if record_size < 44 or record_size > MAX_DIRECTORY or disk or start_disk or disk_count != count:
            raise MetadataError('invalid ZIP64 directory')
    if count > MAX_ENTRIES or directory_size > MAX_DIRECTORY or directory_offset + directory_size > size:
        raise MetadataError('ZIP directory exceeds bounded inspection limits')


def _zip_extract(stream, size, *, allow_manifest_only=False):
    _zip_bounds(stream, size)
    try:
        with zipfile.ZipFile(stream) as archive:
            seen, entry = set(), None
            if len(archive.infolist()) > MAX_ENTRIES:
                raise MetadataError('too many ZIP entries')
            for item in archive.infolist():
                member_name = item.filename.rstrip('/')
                if member_name == '.':
                    raise MetadataError('ZIP entries must use canonical relative paths')
                _relative(member_name)
                normalized = member_name.casefold()
                if normalized in seen:
                    raise MetadataError('duplicate or case-aliased ZIP entries')
                seen.add(normalized)
                if normalized == MANIFEST:
                    member_type = (item.external_attr >> 16) & 0o170000
                    if item.filename != MANIFEST or item.is_dir() or (item.compress_type != zipfile.ZIP_STORED and not allow_manifest_only) or member_type not in (0, stat.S_IFREG):
                        raise MetadataError('game manifest must be a regular stored root ZIP member')
                    entry = item
            from_comment = None
            if archive.comment.endswith(MAGIC):
                import io
                from_comment = _envelope_at(io.BytesIO(archive.comment), len(archive.comment))
                if from_comment and from_comment[1] != 0:
                    raise MetadataError('unexpected bytes before ZIP metadata comment')
            if entry is None:
                if from_comment:
                    raise MetadataError('ZIP metadata comment has no root manifest')
                return None
            if entry.file_size > MAX_PAYLOAD or entry.compress_size > MAX_PAYLOAD + 1024 or (not allow_manifest_only and entry.compress_size != entry.file_size):
                raise MetadataError('invalid ZIP metadata size')
            payload = archive.read(entry)
            document = decode_payload(payload)
            if from_comment is None and allow_manifest_only:
                return document
            if from_comment is None or canonical_payload(from_comment[0]) != payload:
                raise MetadataError('ZIP manifest and metadata comment disagree')
            return document
    except (zipfile.BadZipFile, NotImplementedError, RuntimeError, OSError) as error:
        raise MetadataError(f'invalid ZIP metadata: {error}') from error


def extract(path):
    """Return metadata or None. Malformed claimed metadata raises MetadataError."""
    path = Path(path)
    with _open(path) as stream:
        size = os.fstat(stream.fileno()).st_size
        if path.suffix.lower() in ('.bat', '.cmd'):
            found = _bat(stream)
            return found[0] if found else None
        if path.suffix.lower() == '.zip':
            return _zip_extract(stream, size)
        if path.suffix.lower() == '.exe':
            sections_end, certificate = _pe(stream, size)
            if certificate:
                for padding in range(8):
                    stream.seek(certificate - padding)
                    if stream.read(padding) != b'\0' * padding:
                        continue
                    found = _envelope_at(stream, certificate - padding)
                    if found:
                        if found[1] < sections_end:
                            raise MetadataError('metadata overlaps executable sections')
                        return found[0]
                return None
        found = _envelope_at(stream, size)
        if path.suffix.lower() == '.exe' and found and found[1] < sections_end:
            raise MetadataError('metadata overlaps executable sections')
        return found[0] if found else None


def _atomic(path, writer, close_source=None):
    path = Path(path)
    if path.is_symlink():
        raise MetadataError('cannot stamp a symlink')
    mode = path.stat().st_mode
    descriptor, temporary = tempfile.mkstemp(prefix='.' + path.name + '-', dir=path.parent)
    try:
        with os.fdopen(descriptor, 'w+b') as target:
            writer(target)
            target.flush()
            os.fsync(target.fileno())
        # Windows cannot replace a file while the CRT reader still owns it.
        if close_source is not None:
            close_source.close()
        os.chmod(temporary, stat.S_IMODE(mode))
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def embed(path, document, *, allow_inno=False):
    """Stamp owned output before signing/hashing. No executable is ever launched.

    allow_inno is only for a caller that just compiled this Inno installer. It is
    not a way to permit arbitrary self-extracting or previously signed binaries.
    """
    path, payload = Path(path), canonical_payload(document)
    envelope = make_envelope(document)
    suffix = path.suffix.lower()
    with _open(path) as source:
        size = os.fstat(source.fileno()).st_size
        if suffix == '.zip':
            _zip_bounds(source, size)
            # Validate existing metadata before replacing it; preserve all payload entries.
            _zip_extract(source, size, allow_manifest_only=True)
            source.seek(0)
            def write_zip(target):
                with zipfile.ZipFile(source) as previous, zipfile.ZipFile(target, 'w') as output:
                    for item in previous.infolist():
                        if item.filename == MANIFEST:
                            continue
                        with previous.open(item) as reader, output.open(item, 'w', force_zip64=item.file_size >= zipfile.ZIP64_LIMIT) as writer:
                            shutil.copyfileobj(reader, writer, 1024 * 1024)
                    output.writestr(MANIFEST, payload, compress_type=zipfile.ZIP_STORED)
                    output.comment = envelope
            _atomic(path, write_zip, source)
            return document
        if suffix in ('.bat', '.cmd'):
            existing = _bat(source)
            start = existing[1] if existing else 0
            source.seek(0)
            prefix = source.read(4)
            if prefix.startswith((b'\xef\xbb\xbf', b'\xff\xfe', b'\xfe\xff')) or b'\0' in prefix:
                raise MetadataError('stamp an ASCII-compatible batch file without a byte-order mark')
            encoded = base64.b64encode(payload)
            block = BAT_BEGIN + b'\r\n' + b''.join(b'@rem ' + encoded[index:index + 76] + b'\r\n' for index in range(0, len(encoded), 76)) + BAT_END + b'\r\n'
        elif suffix == '.exe':
            sections_end, certificate = _pe(source, size)
            if certificate:
                raise MetadataError('refusing to modify a signed executable; embed metadata before signing')
            previous = _envelope_at(source, size)
            start = previous[1] if previous else size
            if not allow_inno:
                source.seek(sections_end)
                gap = start - sections_end
                if gap < 0 or gap > 7 or source.read(gap) != b'\0' * gap:
                    raise MetadataError('refusing to modify an unknown executable overlay or self-extracting installer')
            block = envelope
        else:
            raise MetadataError('metadata stamping supports .exe, .bat, .cmd and .zip')
        def write_output(target):
            if suffix in ('.bat', '.cmd'):
                target.write(block)
                source.seek(start)
                shutil.copyfileobj(source, target, 1024 * 1024)
            else:
                source.seek(0)
                remaining = start
                while remaining:
                    chunk = source.read(min(remaining, 1024 * 1024))
                    if not chunk:
                        raise MetadataError('executable changed while stamping')
                    target.write(chunk)
                    remaining -= len(chunk)
                target.write(block)
        _atomic(path, write_output, source)
    return document


def _json(path):
    try:
        with _open(path) as stream:
            data = stream.read(MAX_PAYLOAD + 1)
        return json.loads(data.decode('utf-8-sig')) if len(data) <= MAX_PAYLOAD else {}
    except (OSError, ValueError, UnicodeError):
        return {}


def repository_binding(directory, *, game=False):
    """Public committed source only; dirty/untracked projects never acquire a pin."""
    if directory is None:
        return None
    directory = Path(directory).resolve()
    def git(*arguments):
        done = subprocess.run(['git', '-C', str(directory), *arguments], capture_output=True, text=True, encoding='utf-8', timeout=15)
        return done.stdout.strip() if done.returncode == 0 else None
    try:
        root, revision, remote = git('rev-parse', '--show-toplevel'), git('rev-parse', 'HEAD'), git('remote', 'get-url', 'origin')
        if not root or not revision or not remote:
            return None
        relative = directory.relative_to(Path(root).resolve()).as_posix()
        if git('status', '--porcelain', '--untracked-files=all', '--', relative) != '':
            return None
        if not git('ls-tree', 'HEAD', '--', relative) and relative != '.':
            return None
        match = re.fullmatch(r'(?:https://github\.com/|git@github\.com:)([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+?)(?:\.git)?/?', remote)
        if not match:
            return None
        result = {'repository': match[1], 'commit': revision}
        if game:
            result['game_path'] = relative
            tree = git('rev-parse', 'HEAD^{tree}' if relative == '.' else 'HEAD:' + relative)
            if not tree or not re.fullmatch(r'(?:[0-9a-fA-F]{40}|[0-9a-fA-F]{64})', tree) or git('cat-file', '-t', tree) != 'tree':
                return None
            result['tree'] = tree
        return result
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def build_document(project_root, *, slug=None, title=None, version=None, description=None,
                   source=_AUTO, engine=_AUTO, lineage=None, wiki=None, cargo=None):
    """Collect public authoring data. Explicit null source/engine disables inference."""
    root = Path(project_root)
    identity, project = _json(root / 'assets/identity.json'), _json(root / 'game.project.json')
    if cargo is None:
        cargo = {}
        if (root / 'Cargo.toml').is_file():
            try:
                import tomllib
            except ImportError as error:
                raise MetadataError('Creating metadata directly from Cargo.toml requires Python 3.11+; '
                                    'use the existing scripts/ship.py package command on Python 3.10, '
                                    'or supply already parsed cargo data to build_document') from error
            try:
                cargo = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))
            except (OSError, ValueError) as error:
                raise MetadataError('Cannot read the game Cargo.toml; repair it before exporting metadata') from error
    if not isinstance(cargo, dict) or not isinstance(cargo.get('package', {}), dict):
        raise MetadataError('parsed cargo data must contain a package table')
    cargo = cargo.get('package', {})
    title = title if title is not None else identity.get('title') or cargo.get('name') or root.name
    slug = slug if slug is not None else project.get('id') or re.sub('[^a-z0-9]+', '-', (cargo.get('name') or title).lower()).strip('-') or 'game'
    authors = []
    declared_authors = identity.get('authors', cargo.get('authors', []))
    if not isinstance(declared_authors, list):
        raise MetadataError('public authors must be an array of names')
    for author in declared_authors:
        if isinstance(author, str):
            # Cargo commonly stores private email addresses. Names are sufficient.
            name = re.sub(r'\s*<[^>]*>\s*', '', author).strip()
            if name and '@' not in name:
                authors.append(name)
    result = {'format': FORMAT, 'schema_version': 1,
              'game': {'slug': slug, 'title': title,
                       'version': version if version is not None else cargo.get('version', ''),
                       'description': description if description is not None else project.get('description') or identity.get('tagline') or cargo.get('description', ''),
                       'controls': identity.get('controls', ''), 'authors': authors,
                       'tags': identity.get('tags', cargo.get('keywords', []))},
              'source': repository_binding(root, game=True) if source is _AUTO else source,
              'engine': None if engine is _AUTO else engine}
    for key in ('presentation', 'networking', 'input', 'targets', 'runtime'):
        if key in project:
            result['game'][key] = project[key]
    if lineage is None:
        receipt = _json(root / 'community-source.json')
        if isinstance(receipt, dict) and isinstance(receipt.get('lineage'), dict):
            lineage = {key: receipt['lineage'][key] for key in ('root_game', 'parent_game', 'parent_revision', 'revision_id') if key in receipt['lineage']}
    if lineage is not None:
        result['lineage'] = lineage
    if wiki is None:
        for candidate in (root / 'wiki.json', root.parent.parent / 'site/wiki' / slug / 'wiki.json'):
            value = _json(candidate)
            if isinstance(value, dict) and value:
                wiki = value
                break
    if wiki is not None:
        result['wiki'] = wiki
    canonical_payload(result)
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    inspect = commands.add_parser('inspect', help='read metadata without executing the file')
    inspect.add_argument('path', type=Path)
    build = commands.add_parser('create', help='derive a canonical manifest from a source project')
    build.add_argument('project', type=Path)
    build.add_argument('--output', type=Path, required=True)
    stamp = commands.add_parser('embed', help='embed a reviewed manifest into an owned unsigned output')
    stamp.add_argument('path', type=Path)
    stamp.add_argument('--manifest', type=Path, required=True)
    stamp.add_argument('--inno-output', action='store_true', help='this file was just compiled by your Inno Setup pipeline')
    args = parser.parse_args(argv)
    try:
        if args.command == 'inspect':
            print(json.dumps(extract(args.path), ensure_ascii=False, indent=2))
        elif args.command == 'create':
            args.output.write_bytes(canonical_payload(build_document(args.project)))
        else:
            document = decode_payload(args.manifest.read_bytes())
            embed(args.path, document, allow_inno=args.inno_output)
        return 0
    except (OSError, MetadataError) as error:
        parser.exit(2, f'Game metadata: {error}\n')


if __name__ == '__main__':
    raise SystemExit(main())
