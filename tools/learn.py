#!/usr/bin/env python3
"""Blue learns from the games built on it (ADR 0038). Python 3.10+, standard library only, no network.

  python tools/learn.py sessions [--logs DIR ...] [--since DATE] [--out DIR]   where development effort went
  python tools/learn.py dupes    [--roots DIR ...] [--min-lines N]            what the games copied (promotion candidates)
  python tools/learn.py eval     [--tasks FILE] [-k N]                        does `be2.py context` find the right tools
  python tools/learn.py report                                                one page: docs/learning/REPORT.md
  python tools/learn.py record   --game G --area A --tokens N --note TEXT     one friction-ledger entry
  python tools/learn.py modules  [--write|--check]                            per-file summaries that `context` uses to route to a file
  python tools/learn.py scan     PATH ...                                     leak scan: counts of secret-like values

Privacy by design (docs/learning/README.md): `sessions` reads Claude Code session logs, which contain private
conversation text, tool output and real secrets. It extracts only structure: counts, token usage numbers, tool NAMES,
file paths, timestamps, working directory, branch and the sidechain flag. It never stores, prints or copies message
text, tool results, thinking, command text or search patterns, and writes only to a git-ignored folder (.learning/).
"""
import argparse
import collections
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LEARNING = ROOT / 'docs' / 'learning'
LOCAL = ROOT / '.learning'
HOME = Path.home()


class LearnError(RuntimeError):
    """A user-facing failure; the message never contains log content."""


# ---------------------------------------------------------------------------------------------------------------
# Secret detection, shared by `record`, `sessions` (paths and names) and `scan`.
# ---------------------------------------------------------------------------------------------------------------
UUID_RE = re.compile(r'(?<![0-9A-Za-z])[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}(?![0-9A-Za-z])')
HEX_RE = re.compile(r'(?<![0-9A-Za-z])[0-9a-fA-F]{32,}(?![0-9A-Za-z])')
B64_RE = re.compile(r'[A-Za-z0-9+_\-]{32,}={0,2}')
PREFIX_RE = re.compile(r'(?<![A-Za-z0-9])(ghp_|gho_|ghs_|github_pat_|sk-[A-Za-z0-9]|AKIA[0-9A-Z]{8}|xox[abp]-|AIza[0-9A-Za-z_\-]{8})')
ASSIGN_RE = re.compile(r'(?i)(?:key|token|secret|passw(?:or)?d|authorization|auth)["\']?\s*(?:=\s*["\']?[^\s"\',;]{4,}'
                       r'|:\s*["\']?(?=[A-Za-z0-9_\-+/=.]*[0-9])[A-Za-z0-9_\-+/=.]{12,})')
KEYEQ_RE = re.compile(r'(?i)key\s*=')
WORD_RE = re.compile(r'(?i)(password|passwd|secret|bearer|private[_ -]?key)')
STRICT_WORD_RE = re.compile(r'(?i)(token|password|passwd|secret|key=)')


def secret_like(text, strict=False):
    """Name of the first secret-like pattern in `text`, or None.

    Default (ledger text, paths): UUIDs, 32+ hex runs, long mixed-case base64-looking runs, well-known credential
    prefixes, `name=value` assignments of key/token/secret/password names, and the words password/secret/bearer.
    `strict` (the leak scan of generated files) also flags any value containing "token" or "key=".
    """
    if not isinstance(text, str):
        return None
    if UUID_RE.search(text):
        return 'uuid'
    if HEX_RE.search(text):
        return 'long-hex'
    if PREFIX_RE.search(text):
        return 'credential-prefix'
    if ASSIGN_RE.search(text):
        return 'credential-assignment'
    for match in B64_RE.finditer(text):
        run = match.group(0)
        if re.search(r'[A-Z]', run) and re.search(r'[a-z]', run) and re.search(r'[0-9]', run):
            return 'long-base64'
    if KEYEQ_RE.search(text):
        return 'key-assignment'
    if (STRICT_WORD_RE if strict else WORD_RE).search(text):
        return 'credential-word'
    return None


def short_hash(value):
    """Stable 8-hex stand-in for an identifier (session or agent id) that must not be stored verbatim."""
    return hashlib.sha256(str(value).encode('utf-8', 'replace')).hexdigest()[:8]


NAME_RE = re.compile(r'^[A-Za-z0-9_:.\-]{1,80}$')


def safe_name(value, default='other'):
    """A short enum-like name (tool, record type, model, program) or `default`; never free text."""
    if isinstance(value, str) and NAME_RE.fullmatch(value) and secret_like(value, strict=True) is None:
        return value
    return default


def tidy_path(raw, home=None):
    """Home-relative (~/...) path with no secret-like content, or None.

    A path that looks like it embeds a credential is replaced by the literal "<redacted-path>" so the count of
    such paths survives but the text does not.
    """
    if not isinstance(raw, str) or not raw or len(raw) > 1000 or '\n' in raw or '\0' in raw:
        return None
    home = str(home or HOME).rstrip('/')
    path = raw.replace('\\', '/')
    if path == home:
        path = '~'
    elif path.startswith(home + '/'):
        path = '~' + path[len(home):]
    # Claude Code stores big tool outputs and subagent logs under UUID-named folders: keep the shape, not the ids.
    path = UUID_RE.sub('<uuid>', path)
    path = re.sub(r'(/tool-results/)[^/]+$', r'\1<file>', path)
    path = re.sub(r'(/subagents/)[^/]+$', r'\1<agent-log>', path)
    if secret_like(path, strict=True):
        return '<redacted-path>'
    return path[:240]


# Worktree and branch copies of a repo (BlueEngineGames-df-audio, SpookyKart-fb-x, X-shadows ...): the same project.
COPY_SUFFIX_RE = re.compile(r'-(fb|df|eng|target|wt|fc|notes|shadows|env|publish)(-.*)?$')
NON_GAME_AREAS = {'BlueEngine', 'BlueEngineGames', 'claude-config', 'workspace', '(outside-home)', 'other', '~'}


def area_of(path):
    """Repo or game a path belongs to: BlueEngine, BlueEngineGames/games/<game>, SpookyKart, <dir> ..."""
    path = path if isinstance(path, str) else ''
    if not path.startswith('~/'):
        if path == '~':
            return '~'
        return '(outside-home)' if path else 'other'
    parts = [p for p in path[2:].split('/') if p]
    if not parts:
        return '~'
    top = parts[0]
    if top.startswith('.claude'):
        return 'claude-config'
    if top.startswith('BlueEngineGames'):
        if len(parts) >= 3 and parts[1] == 'games':
            return 'BlueEngineGames/games/' + parts[2]
        return 'BlueEngineGames'
    if top.startswith('BlueEngine'):
        return 'BlueEngine'
    if top.startswith('SpookyKart'):
        return 'SpookyKart'
    return COPY_SUFFIX_RE.sub('', top)[:60] or 'other'


def parse_ts(value):
    if not isinstance(value, str):
        return None
    try:
        stamp = datetime.datetime.fromisoformat(value.replace('Z', '+00:00'))
    except ValueError:
        return None
    return stamp if stamp.tzinfo else stamp.replace(tzinfo=datetime.timezone.utc)


def iso(stamp):
    return stamp.astimezone(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ') if stamp else None


def write_jsonl(path, rows):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('w', encoding='utf-8') as stream:
        for row in rows:
            stream.write(json.dumps(row, sort_keys=True) + '\n')


def git_head():
    try:
        out = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], cwd=ROOT, capture_output=True, text=True)
        # The eval log itself is appended to by every run; it does not make the engine "dirty".
        dirty = subprocess.run(['git', 'status', '--porcelain', '--untracked-files=no', '--', '.',
                                ':!docs/learning/eval_runs.jsonl'], cwd=ROOT, capture_output=True, text=True).stdout.strip()
        return (out.stdout.strip() or 'unknown'), bool(dirty)
    except OSError:
        return 'unknown', False


# ---------------------------------------------------------------------------------------------------------------
# sessions: aggregate structure of Claude Code session logs
# ---------------------------------------------------------------------------------------------------------------
USAGE_KINDS = ('input_tokens', 'output_tokens', 'cache_creation_input_tokens', 'cache_read_input_tokens')
KIND_SHORT = {'input_tokens': 'input', 'output_tokens': 'output',
              'cache_creation_input_tokens': 'cache_write', 'cache_read_input_tokens': 'cache_read'}
BASH_KNOWN = frozenset('''git cargo python python3 grep rg find ls cat head tail sed awk wc sort uniq curl ssh scp rsync cd
echo mkdir rm cp mv chmod xvfb-run timeout sleep tee diff du df ps kill pkill which tar unzip md5sum sha256sum stat touch
gh npm node make bash sh export test true xargs jq convert ffmpeg systemctl journalctl docker nohup rustc rustup bc date
tr cut sed ag fd file ln readlink basename dirname pwd env printf read tree zip pip pip3 apt dpkg gzip xdotool scrot import
ffprobe nproc free uname whoami id seq paste comm cmp realpath mktemp'''.split())
SEARCH_PROGRAMS = frozenset({'grep', 'rg', 'ag', 'fd', 'find'})
SKIP_PROGRAMS = frozenset({'cd', 'export', 'set', 'true', 'pushd', 'popd', 'source', ':'})
SEP_RE = re.compile(r'\s*(?:&&|\|\||;|\||\n)\s*')
CODE_SUFFIXES = frozenset({'.rs', '.py', '.js', '.ts', '.c', '.h', '.cpp', '.glsl', '.wgsl', '.go', '.java', '.cs'})
EXIT1_RE = re.compile(r'^\s*Exit code 1\s*$')
DEFAULT_WRITE_THRESHOLD = 3000
TOP_N = 5


def primary_program(command):
    """Name of the first real program in a shell command (from a fixed list, else "other"). Nothing else is kept."""
    if not isinstance(command, str):
        return 'other'
    for segment in SEP_RE.split(command[:2000]):
        words = segment.strip().split()
        while words and ('=' in words[0] or words[0] in ('sudo', 'time', 'nohup', 'env')):
            words.pop(0)
        if not words:
            continue
        program = os.path.basename(words[0].strip('()"\'`'))
        if program in SKIP_PROGRAMS:
            continue
        return program if program in BASH_KNOWN else 'other'
    return 'other'


EMPTY_MARKERS = frozenset({'(Bash completed with no output)', 'No files found', 'No matches found'})


def result_is_empty(block, exit1_means_empty):
    """True when a tool_result body is empty or one of the tool's fixed "nothing found" markers.

    The body is compared to fixed strings and measured; none of it is kept.
    """
    content = block.get('content')
    if isinstance(content, list):
        content = ''.join(part['text'] for part in content
                          if isinstance(part, dict) and isinstance(part.get('text'), str))
    if content is None:
        return True
    if not isinstance(content, str):
        return False
    stripped = content.strip()
    return not stripped or stripped in EMPTY_MARKERS or (exit1_means_empty and bool(EXIT1_RE.match(content)))


class Message:
    """What one assistant message cost and did; usage is the max seen across its streamed records."""
    __slots__ = ('usage', 'side', 'cwd_area', 'path_areas', 'tools', 'bash', 'reads', 'repeat_reads',
                 'repeat_unedited', 'searches', 'empty_searches', 'large_writes', 'model')

    def __init__(self):
        self.usage = dict.fromkeys(USAGE_KINDS, 0)
        self.side = False
        self.cwd_area = 'other'
        self.path_areas = []
        self.tools = collections.Counter()
        self.bash = collections.Counter()
        self.reads = self.repeat_reads = self.repeat_unedited = self.searches = self.empty_searches = 0
        self.large_writes = []
        self.model = None

    def area(self):
        if self.path_areas:
            return collections.Counter(self.path_areas).most_common(1)[0][0]
        return self.cwd_area


class Session:
    def __init__(self, key):
        self.key = key
        self.first = self.last = None
        self.cwds = collections.Counter()
        self.branches = collections.Counter()
        self.messages = {}
        self.seen_tools = set()
        self.pending = {}
        self.read_state = {}          # (agent, path) -> [reads, edited_since_last_read]
        self.read_counts = collections.Counter()
        self.repeat_counts = collections.Counter()
        self.models = collections.Counter()
        self.result_stats = collections.Counter()

    def touch(self, stamp, cwd, branch):
        if stamp:
            self.first = stamp if self.first is None or stamp < self.first else self.first
            self.last = stamp if self.last is None or stamp > self.last else self.last
        if cwd:
            self.cwds[cwd] += 1
        if branch:
            self.branches[branch] += 1


class Aggregate:
    def __init__(self, write_threshold=DEFAULT_WRITE_THRESHOLD, home=None, since=None):
        self.sessions = {}
        self.write_threshold = write_threshold
        self.home = home
        self.since = since
        self.stats = collections.Counter()
        self.ignored_types = collections.Counter()

    def tidy(self, path):
        return tidy_path(path, self.home)


def iter_records(path, aggregate):
    """Yield JSON objects from one log file; malformed or truncated lines and unreadable files are only counted."""
    try:
        stream = open(path, 'rb')
    except OSError:
        aggregate.stats['unreadable_files'] += 1
        return
    with stream:
        while True:
            try:
                raw = stream.readline()
            except OSError:
                aggregate.stats['unreadable_files'] += 1
                return
            if not raw:
                return
            if not raw.strip():
                continue
            try:
                record = json.loads(raw)
            except (ValueError, RecursionError):
                aggregate.stats['malformed_lines'] += 1
                continue
            if not isinstance(record, dict):
                aggregate.stats['malformed_lines'] += 1
                continue
            yield record


def note_tool_use(aggregate, session, message, block, side_ctx):
    tool_id = block.get('id')
    if isinstance(tool_id, str):
        if tool_id in session.seen_tools:
            return
        session.seen_tools.add(tool_id)
    name = safe_name(block.get('name'))
    arguments = block.get('input') if isinstance(block.get('input'), dict) else {}
    message.tools[name] += 1
    raw_path = arguments.get('file_path') if name in ('Read', 'Edit', 'Write', 'MultiEdit') else \
        arguments.get('notebook_path') if name == 'NotebookEdit' else None
    path = aggregate.tidy(raw_path) if raw_path is not None else None
    # Identity of the file: the shown path, plus a hash when ids were masked so distinct files never merge.
    ident = (path, short_hash(raw_path) if path and '<' in path else '') if path else None
    if path:
        message.path_areas.append(area_of(path))
    if name == 'Read' and path:
        state = session.read_state.setdefault((side_ctx, ident), [0, False])
        message.reads += 1
        session.read_counts[ident] += 1
        if state[0]:
            message.repeat_reads += 1
            session.repeat_counts[ident] += 1
            if not state[1]:
                message.repeat_unedited += 1
        state[0] += 1
        state[1] = False
    elif name in ('Edit', 'Write', 'MultiEdit', 'NotebookEdit') and path:
        session.read_state.setdefault((side_ctx, ident), [0, False])[1] = True
        body = arguments.get('content')
        if name == 'Write' and isinstance(body, str) and len(body) >= aggregate.write_threshold \
                and Path(path).suffix in CODE_SUFFIXES:
            message.large_writes.append((path, len(body)))
    elif name in ('Grep', 'Glob'):
        message.searches += 1
        if isinstance(tool_id, str):
            session.pending[tool_id] = (message, False)
    elif name == 'Bash':
        program = primary_program(arguments.get('command'))
        message.bash[program] += 1
        if program in SEARCH_PROGRAMS:
            message.searches += 1
            if isinstance(tool_id, str):
                session.pending[tool_id] = (message, program in ('grep', 'rg', 'ag'))


def note_tool_result(session, block):
    pending = session.pending.pop(block.get('tool_use_id'), None) if isinstance(block.get('tool_use_id'), str) else None
    if pending is None:
        return
    message, exit1_means_empty = pending
    if result_is_empty(block, exit1_means_empty):
        message.empty_searches += 1


def consume(aggregate, record, in_subagent_file):
    kind = record.get('type')
    if kind not in ('assistant', 'user'):
        aggregate.ignored_types[safe_name(kind, 'unnamed')] += 1
        return
    stamp = parse_ts(record.get('timestamp'))
    if aggregate.since and stamp and stamp < aggregate.since:
        aggregate.stats['before_since'] += 1
        return
    raw_session = record.get('sessionId')
    key = short_hash(raw_session) if isinstance(raw_session, str) else 'unknown'
    session = aggregate.sessions.get(key)
    if session is None:
        session = aggregate.sessions[key] = Session(key)
    cwd = aggregate.tidy(record.get('cwd'))
    branch = record.get('gitBranch')
    session.touch(stamp, cwd, safe_name(branch, '(odd)') if isinstance(branch, str) and branch else None)
    side = bool(record.get('isSidechain')) or in_subagent_file
    agent = record.get('agentId')
    side_ctx = short_hash(agent) if isinstance(agent, str) else 'main'
    message = record.get('message')
    if not isinstance(message, dict):
        aggregate.stats['records_without_message'] += 1
        return
    content = message.get('content')
    if kind == 'user':
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get('type') == 'tool_result':
                    note_tool_result(session, block)
        return
    message_id = message.get('id') if isinstance(message.get('id'), str) else record.get('uuid')
    entry = session.messages.get(message_id)
    if entry is None:
        entry = session.messages[message_id] = Message()
        entry.cwd_area = area_of(cwd)
        entry.model = safe_name(message.get('model'), 'unknown-model')
    entry.side = entry.side or side
    usage = message.get('usage')
    if isinstance(usage, dict):
        for field in USAGE_KINDS:
            value = usage.get(field)
            if isinstance(value, int) and not isinstance(value, bool) and value > entry.usage[field]:
                entry.usage[field] = value
    else:
        aggregate.stats['assistant_without_usage'] += 1
    if isinstance(content, list):
        for block in content:
            if isinstance(block, dict) and block.get('type') == 'tool_use':
                note_tool_use(aggregate, session, entry, block, side_ctx)


def discover_logs(paths):
    """Every *.jsonl under the given files/directories (subagent logs live in the same tree)."""
    found = []
    for item in paths:
        item = Path(item).expanduser()
        if item.is_file():
            found.append(item)
        elif item.is_dir():
            found.extend(sorted(item.rglob('*.jsonl')))
    seen, unique = set(), []
    for item in found:
        resolved = str(item.resolve())
        if resolved not in seen:
            seen.add(resolved)
            unique.append(item)
    return unique


def scan_logs(paths, since=None, write_threshold=DEFAULT_WRITE_THRESHOLD, home=None):
    aggregate = Aggregate(write_threshold, home, since)
    files = discover_logs(paths)
    aggregate.stats['files'] = len(files)
    for path in files:
        in_subagent_file = 'subagents' in path.parts
        for record in iter_records(path, aggregate):
            aggregate.stats['records'] += 1
            consume(aggregate, record, in_subagent_file)
    return aggregate


def total_work(tokens):
    """Tokens that were processed fresh (input + cache writes) or generated; cache reads are reported separately."""
    return tokens['input'] + tokens['output'] + tokens['cache_write']


def empty_tokens():
    return dict.fromkeys(KIND_SHORT.values(), 0)


def build_rows(aggregate):
    """(session rows, area rows, summary) from an Aggregate. Only counts, names and paths."""
    session_rows, areas = [], {}
    all_tools, all_bash = collections.Counter(), collections.Counter()
    read_counts, repeat_counts = collections.Counter(), collections.Counter()
    large_writes = []
    totals = {'usage': empty_tokens(), 'side_usage': empty_tokens(), 'turns': 0, 'side_turns': 0,
              'reads': 0, 'repeat_reads': 0, 'repeat_reads_unedited': 0, 'searches': 0, 'empty_searches': 0}
    for session in aggregate.sessions.values():
        row_tokens, side_tokens = empty_tokens(), empty_tokens()
        tools, bash = collections.Counter(), collections.Counter()
        counters = collections.Counter()
        session_areas = collections.Counter()
        session_large = []
        for message in session.messages.values():
            area = message.area()
            target = areas.setdefault(area, {'area': area, 'sessions': set(), 'turns': 0, 'side_turns': 0,
                                             'usage': empty_tokens(), 'side_usage': empty_tokens(),
                                             'tools': collections.Counter(), 'reads': 0, 'repeat_reads': 0,
                                             'repeat_reads_unedited': 0, 'searches': 0, 'empty_searches': 0,
                                             'large_writes': 0, 'large_write_chars': 0})
            target['sessions'].add(session.key)
            target['turns'] += 1
            totals['turns'] += 1
            for field, short in KIND_SHORT.items():
                row_tokens[short] += message.usage[field]
                target['usage'][short] += message.usage[field]
                totals['usage'][short] += message.usage[field]
                if message.side:
                    side_tokens[short] += message.usage[field]
                    target['side_usage'][short] += message.usage[field]
                    totals['side_usage'][short] += message.usage[field]
            if message.side:
                target['side_turns'] += 1
                totals['side_turns'] += 1
                counters['side_turns'] += 1
            session_areas[area] += sum(message.usage[f] for f in USAGE_KINDS if f != 'cache_read_input_tokens')
            tools.update(message.tools)
            bash.update(message.bash)
            target['tools'].update(message.tools)
            for field in ('reads', 'repeat_reads', 'searches', 'empty_searches'):
                counters[field] += getattr(message, field)
                target[field] += getattr(message, field)
                totals[field] += getattr(message, field)
            counters['repeat_reads_unedited'] += message.repeat_unedited
            target['repeat_reads_unedited'] += message.repeat_unedited
            totals['repeat_reads_unedited'] += message.repeat_unedited
            for path, length in message.large_writes:
                path_area = area_of(path)
                if path_area not in NON_GAME_AREAS:
                    session_large.append({'path': path, 'chars': length, 'area': path_area})
                    areas.setdefault(path_area, {'area': path_area, 'sessions': set(), 'turns': 0, 'side_turns': 0,
                                                 'usage': empty_tokens(), 'side_usage': empty_tokens(),
                                                 'tools': collections.Counter(), 'reads': 0, 'repeat_reads': 0,
                                                 'repeat_reads_unedited': 0, 'searches': 0, 'empty_searches': 0,
                                                 'large_writes': 0, 'large_write_chars': 0})
                    areas[path_area]['large_writes'] += 1
                    areas[path_area]['large_write_chars'] += length
        all_tools.update(tools)
        all_bash.update(bash)
        read_counts.update(session.read_counts)
        repeat_counts.update(session.repeat_counts)
        large_writes.extend(session_large)
        work = total_work(row_tokens)
        side_work = total_work(side_tokens)
        primary_cwd = session.cwds.most_common(1)[0][0] if session.cwds else None
        session_rows.append({
            'session': session.key,
            'first': iso(session.first), 'last': iso(session.last),
            'minutes': round((session.last - session.first).total_seconds() / 60, 1) if session.first and session.last else None,
            'cwd': primary_cwd, 'branch': session.branches.most_common(1)[0][0] if session.branches else None,
            'area': session_areas.most_common(1)[0][0] if session_areas else area_of(primary_cwd),
            'turns': len(session.messages), 'side_turns': counters['side_turns'],
            'usage': row_tokens, 'fresh_work': work,
            'side_fresh_work': side_work,
            'side_share': round(side_work / work, 3) if work else 0.0,
            'tools': dict(tools.most_common()), 'bash_programs': dict(bash.most_common(12)),
            'reads': counters['reads'], 'distinct_files_read': len(session.read_counts),
            'repeat_reads': counters['repeat_reads'], 'repeat_reads_unedited': counters['repeat_reads_unedited'],
            'top_reads': [{'path': p[0], 'reads': n} for p, n in session.read_counts.most_common(TOP_N)],
            'searches': counters['searches'], 'empty_searches': counters['empty_searches'],
            'large_writes': sorted(session_large, key=lambda item: -item['chars'])[:10],
            'large_write_count': len(session_large),
        })
    session_rows.sort(key=lambda row: (row['first'] or '', row['session']))
    area_rows = []
    for item in areas.values():
        work = total_work(item['usage'])
        side_work = total_work(item['side_usage'])
        area_rows.append({**item, 'sessions': len(item['sessions']), 'fresh_work': work,
                          'side_fresh_work': side_work,
                          'side_share': round(side_work / work, 3) if work else 0.0,
                          'tools': dict(item['tools'].most_common(10))})
    area_rows.sort(key=lambda row: -row['fresh_work'])
    work = total_work(totals['usage'])
    summary = {
        'files': aggregate.stats['files'], 'records': aggregate.stats['records'],
        'sessions': len(session_rows),
        'malformed_lines': aggregate.stats['malformed_lines'], 'unreadable_files': aggregate.stats['unreadable_files'],
        'records_without_message': aggregate.stats['records_without_message'],
        'assistant_without_usage': aggregate.stats['assistant_without_usage'],
        'ignored_record_types': dict(aggregate.ignored_types.most_common()),
        'turns': totals['turns'], 'side_turns': totals['side_turns'],
        'usage': totals['usage'], 'fresh_work': work,
        'side_fresh_work': total_work(totals['side_usage']),
        'side_share': round(total_work(totals['side_usage']) / work, 3) if work else 0.0,
        'tools': dict(all_tools.most_common()), 'bash_programs': dict(all_bash.most_common(15)),
        'reads': totals['reads'], 'distinct_files_read': len(read_counts),
        'repeat_reads': totals['repeat_reads'], 'repeat_reads_unedited': totals['repeat_reads_unedited'],
        'top_read_paths': [{'path': p[0], 'reads': n} for p, n in read_counts.most_common(10)],
        'top_repeated_paths': [{'path': p[0], 'repeats': n} for p, n in repeat_counts.most_common(10)],
        'searches': totals['searches'], 'empty_searches': totals['empty_searches'],
        'from_scratch_candidates': len(large_writes),
        'from_scratch_distinct_files': len({item['path'] for item in large_writes}),
        'from_scratch_chars': sum(item['chars'] for item in large_writes),
        'from_scratch_top': sorted(large_writes, key=lambda item: -item['chars'])[:10],
        'write_threshold_chars': aggregate.write_threshold,
        'areas': [{k: row[k] for k in ('area', 'sessions', 'turns', 'fresh_work', 'side_share')}
                  | {'output': row['usage']['output'], 'cache_read': row['usage']['cache_read']}
                  for row in area_rows[:12]],
    }
    return session_rows, area_rows, summary


def format_summary(summary):
    """Plain-text summary table (numbers, tool names and paths only; avoids the word that trips the leak scan)."""
    def k(n):
        return f'{n / 1000:,.0f}k' if n >= 10000 else f'{n:,}'
    lines = [f"sessions {summary['sessions']} from {summary['files']} files, {summary['records']:,} records "
             f"({summary['malformed_lines']} malformed lines, {summary['unreadable_files']} unreadable files, "
             f"{sum(summary['ignored_record_types'].values()):,} other record types ignored)",
             f"assistant turns {summary['turns']:,} (subagent {summary['side_turns']:,}); "
             f"fresh work (input + cache writes + output) {k(summary['fresh_work'])}, "
             f"output {k(summary['usage']['output'])}, cache reads {k(summary['usage']['cache_read'])}; "
             f"subagent share of fresh work {summary['side_share']:.0%}", '',
             f"{'area':<44}{'sessions':>9}{'turns':>8}{'fresh work':>12}{'output':>10}{'subagent':>10}"]
    for row in summary['areas']:
        lines.append(f"{row['area']:<44}{row['sessions']:>9}{row['turns']:>8}{k(row['fresh_work']):>12}"
                     f"{k(row['output']):>10}{row['side_share']:>10.0%}")
    top_tools = ', '.join(f'{name} {count}' for name, count in list(summary['tools'].items())[:10])
    lines += ['', f'tool calls: {top_tools}',
              f"reads {summary['reads']:,} of {summary['distinct_files_read']:,} distinct files; repeat reads "
              f"{summary['repeat_reads']:,} (of which with no edit in between {summary['repeat_reads_unedited']:,})",
              f"searches {summary['searches']:,}, empty results {summary['empty_searches']:,}",
              f"large new source files written in game repos (>= {summary['write_threshold_chars']} chars): "
              f"{summary['from_scratch_candidates']}, {k(summary['from_scratch_chars'])} chars"]
    if summary['top_repeated_paths']:
        lines += ['', 'most re-read files (repeat reads):']
        lines += [f"  {item['repeats']:>4}  {item['path']}" for item in summary['top_repeated_paths'][:8]]
    if summary['from_scratch_top']:
        lines += ['', 'largest new source files in game repos (chars):']
        lines += [f"  {item['chars']:>7}  {item['path']}" for item in summary['from_scratch_top'][:8]]
    return '\n'.join(lines)


def inside_repo_unignored(out):
    """True when `out` is inside this checkout but not under the git-ignored .learning/ folder."""
    out = out.resolve()
    try:
        out.relative_to(ROOT)
    except ValueError:
        return False
    try:
        out.relative_to(LOCAL.resolve())
        return False
    except ValueError:
        return True


def cmd_sessions(args):
    since = None
    if args.since:
        try:
            since = datetime.datetime.fromisoformat(args.since)
        except ValueError:
            raise LearnError(f'--since must be a date like 2026-09-30, got {args.since!r}') from None
        since = since if since.tzinfo else since.replace(tzinfo=datetime.timezone.utc)
    out = Path(args.out).expanduser() if args.out else LOCAL
    if inside_repo_unignored(out) and not args.allow_tracked_out:
        raise LearnError('Refusing to write session aggregates inside the repository outside the git-ignored '
                         '.learning/ folder (they derive from private logs). Use --out .learning/<name> or a path '
                         'outside the checkout.')
    logs = args.logs or [HOME / '.claude' / 'projects']
    aggregate = scan_logs(logs, since, args.write_threshold)
    if not aggregate.stats['files']:
        raise LearnError('No *.jsonl session logs found under: ' + ', '.join(str(p) for p in logs))
    session_rows, area_rows, summary = build_rows(aggregate)
    write_jsonl(out / 'sessions.jsonl', session_rows)
    write_jsonl(out / 'areas.jsonl', area_rows)
    (out / 'summary.json').write_text(json.dumps(summary, indent=2, sort_keys=True) + '\n', encoding='utf-8')
    text = format_summary(summary)
    (out / 'summary.txt').write_text(text + '\n', encoding='utf-8')
    leaks = scan_paths([out / name for name in ('sessions.jsonl', 'areas.jsonl', 'summary.json', 'summary.txt')])
    if leaks['total']:
        for name in ('sessions.jsonl', 'areas.jsonl', 'summary.json', 'summary.txt'):
            (out / name).unlink(missing_ok=True)
        raise LearnError(f"Leak scan flagged {leaks['total']} secret-like value(s) in the output "
                         f"({', '.join(f'{k}: {v}' for k, v in sorted(leaks['by_class'].items()))}); output deleted. "
                         f"Run `learn.py scan` on a copy with --where to locate the key path, and fix the extractor.")
    if args.json:
        print(json.dumps({**summary, 'out': str(out), 'leak_scan': leaks['total']}, indent=2, sort_keys=True))
    else:
        print(text)
        print(f'\nwrote {out}/sessions.jsonl, areas.jsonl, summary.json, summary.txt (leak scan: {leaks["total"]} matches)')
    return 0


# ---------------------------------------------------------------------------------------------------------------
# scan: count secret-like values in generated files (counts only, never the values)
# ---------------------------------------------------------------------------------------------------------------
def scan_value(value, where, strict, found, depth=0):
    if depth > 40:
        return
    if isinstance(value, str):
        label = secret_like(value, strict=strict)
        if label:
            found.append((label, where))
    elif isinstance(value, dict):
        for key, item in value.items():
            # Field names such as "input_tokens" are schema, not data: only value-like patterns apply to keys.
            label = secret_like(key, strict=False) if isinstance(key, str) else None
            if label and label != 'credential-word':
                found.append((label, where + '/<key>'))
            scan_value(item, where + '/' + (key if isinstance(key, str) and NAME_RE.fullmatch(key) else '<key>'),
                       strict, found, depth + 1)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            scan_value(item, where + '[]', strict, found, depth + 1)


def scan_paths(paths):
    """Counts of secret-like values in .jsonl/.json (strict on values) and text files (strict patterns minus
    the bare word "token", which the plain-text summary uses as a column name)."""
    by_class, where = collections.Counter(), collections.Counter()
    for path in paths:
        path = Path(path)
        if not path.is_file():
            continue
        found = []
        if path.suffix == '.jsonl':
            with path.open('rb') as stream:
                for number, raw in enumerate(stream, 1):
                    if not raw.strip():
                        continue
                    try:
                        doc = json.loads(raw)
                    except ValueError:
                        found.append(('unparseable-line', f'{path.name}:{number}'))
                        continue
                    scan_value(doc, path.name, True, found)
        elif path.suffix == '.json':
            try:
                scan_value(json.loads(path.read_text(encoding='utf-8')), path.name, True, found)
            except ValueError:
                found.append(('unparseable-file', path.name))
        else:
            for number, line in enumerate(path.read_text(encoding='utf-8', errors='replace').splitlines(), 1):
                label = secret_like(line, strict=False)
                if label:
                    found.append((label, f'{path.name}:{number}'))
        for label, location in found:
            by_class[label] += 1
            where[location] += 1
    return {'total': sum(by_class.values()), 'by_class': dict(by_class), 'where': dict(where.most_common(20))}


def cmd_scan(args):
    result = scan_paths(args.paths)
    if not args.where:
        result.pop('where')
    print(json.dumps(result, indent=2, sort_keys=True) if args.json else
          f"{result['total']} secret-like matches" + (
              ' (' + ', '.join(f'{k}: {v}' for k, v in sorted(result['by_class'].items())) + ')' if result['total'] else ''))
    return 1 if result['total'] else 0


# ---------------------------------------------------------------------------------------------------------------
# dupes: what did the games copy? (promotion candidates, cross-checked against what the engine already has)
# ---------------------------------------------------------------------------------------------------------------
DEFAULT_ROOTS = ('~/BlueEngineGames/games', '~/SpookyKart', '~/PropHunt', '~/Slapstick', '~/DeadAir', '~/PhysicsLab')
SKIP_DIRS = frozenset({'target', 'dist', '.git', 'node_modules', '__pycache__', '.be2-work', '.blue-check',
                       '.learning', 'evidence', 'site', 'launcher', 'deploy'})
SOURCE_SUFFIXES = frozenset({'.rs', '.py'})
MAX_SOURCE_BYTES = 1_000_000
BLOCK_COMMENT_RE = re.compile(r'/\*.*?\*/')
LINE_COMMENT_RE = re.compile(r'(?<![:"\'])//.*$')
TIGHTEN_RE = re.compile(r'\s*([{}()\[\],;:=<>+\-*/&|!])\s*')
TRIVIAL_RE = re.compile(r'^[\W_]*$')
IMPORT_RE = re.compile(r'^(pub(\(\w+\))?\s+)?(use|mod|extern\s+crate)\s|^(import|from)\s+\S+(\s+import\s|$)')
IDENT_RE = re.compile(r'\b(?:fn|struct|enum|trait|type|const|static|mod|def|class)\s+([A-Za-z_][A-Za-z0-9_]*)')
GENERIC_NAMES = frozenset('''new default main test tests from into drop clone fmt eq hash len get set run init update step
build load save parse read write render draw handle start stop reset next prev with value values name names data state
text size position origin color index error result config options apply insert remove clear push pop iter map filter
check test_helpers setup helper helpers helper_fn app tick frame input output draw_all kind label world player'''.split())
GAME_REPO_SUFFIXES = ('.rs', '.py')


def skippable_dir(name):
    return name in SKIP_DIRS or name.startswith('.') or bool(COPY_SUFFIX_RE.search(name)) or \
        (name.startswith('BlueEngineGames-') or name.startswith('BlueEngine-'))


def project_key(name):
    """`SpookyKart` (dev folder) and `spooky-kart` (published copy) are one project."""
    return re.sub(r'[^a-z0-9]', '', name.lower())


def collect_sources(roots, stats):
    """[(project, relpath, Path)] for every source file under the roots, skipping build output and branch copies."""
    seen, files = set(), []
    for root in roots:
        root = Path(root).expanduser()
        if not root.is_dir():
            stats['missing_roots'].append(tidy_path(str(root)))
            continue
        if skippable_dir(root.name) and root.name not in ('games',):
            stats['skipped_dirs'] += 1
            stats['skipped_roots'].append(tidy_path(str(root)))
            continue
        collection = root.name == 'games'
        for current, dirs, names in os.walk(root):
            kept = []
            for name in dirs:
                if skippable_dir(name):
                    stats['skipped_dirs'] += 1
                else:
                    kept.append(name)
            dirs[:] = sorted(kept)
            for name in sorted(names):
                path = Path(current) / name
                template_json = path.suffix == '.json' and any(part in ('templates', 'template') for part in path.parts)
                if path.suffix not in SOURCE_SUFFIXES and not template_json:
                    continue
                try:
                    if path.stat().st_size > MAX_SOURCE_BYTES or path.is_symlink():
                        continue
                except OSError:
                    continue
                rel = path.relative_to(root)
                project = rel.parts[0] if collection and len(rel.parts) > 1 else root.name
                inner = Path(*rel.parts[1:]) if collection and len(rel.parts) > 1 else rel
                key = (project_key(project), str(inner))
                if key in seen:
                    stats['published_copies_collapsed'] += 1
                    continue
                seen.add(key)
                files.append((project, str(inner), path))
    return files


STRING_RE = re.compile(r'"(?:\\.|[^"\\])*"')
NUMBER_RE = re.compile(r'\b\d[\d_]*(?:\.\d+)?[A-Za-z0-9_]*\b')


def project_name_regex(project):
    """Regex for a project's own name in any casing/separator (prop-hunt, PropHunt, PROP_HUNT), or None."""
    words = [w for w in re.findall(r'[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])', project) if w]
    if len(project_key(project)) < 4 or not words:
        return None
    return re.compile(r'[-_ ]?'.join(re.escape(w) for w in words), re.I)


def normalise_source(text, suffix, project=None):
    """[(normalised line, original line number)]: comments, blank/trivial lines and imports removed; string and
    number literals and the project's own name masked, so copies that differ only in names still match."""
    name_re = project_name_regex(project) if project else None
    lines, in_block = [], False
    for number, line in enumerate(text.splitlines(), 1):
        if suffix == '.rs':
            if in_block:
                if '*/' not in line:
                    continue
                line = line.split('*/', 1)[1]
                in_block = False
            line = BLOCK_COMMENT_RE.sub('', line)
            if '/*' in line:
                line = line.split('/*', 1)[0]
                in_block = True
            line = LINE_COMMENT_RE.sub('', line)
        elif suffix == '.py':
            stripped = line.lstrip()
            if stripped.startswith('#'):
                continue
            if '#' in line and '"' not in line and "'" not in line:
                line = line.split('#', 1)[0]
        line = STRING_RE.sub('""', line)
        line = NUMBER_RE.sub('0', line)
        if name_re:
            line = name_re.sub('GAME', line)
        collapsed = TIGHTEN_RE.sub(r'\1', ' '.join(line.split()))
        if not collapsed or TRIVIAL_RE.match(collapsed) or IMPORT_RE.match(collapsed):
            continue
        lines.append((collapsed, number))
    return lines


class DupNode:
    def __init__(self, ident, files, norm, raw_lines, engine=False):
        self.ident = ident
        self.files = files                  # [(project, relpath, path)]
        self.norm = norm
        self.raw_lines = raw_lines
        self.engine = engine
        self.windows = {}
        self.cover = set()                  # normalised line indexes shared with another node


def cover_indexes(starts, k):
    covered = set()
    for start in starts:
        covered.update(range(start, start + k))
    return covered


def find_clusters(files, k=4, min_lines=25, engine_files=(), max_group=40):
    """Cluster identical and near-identical sources. Returns (clusters, nodes) with clusters sorted by score."""
    by_hash = collections.OrderedDict()
    for project, rel, path in files:
        try:
            data = path.read_bytes()
        except OSError:
            continue
        by_hash.setdefault(hashlib.sha256(data).hexdigest(), []).append((project, rel, path, data))
    nodes = []
    for group in by_hash.values():
        data = group[0][3]
        text = data.decode('utf-8', 'replace')
        norm = normalise_source(text, group[0][2].suffix, group[0][0])
        node = DupNode(len(nodes), [(p, r, f) for p, r, f, _ in group], norm, text.splitlines())
        nodes.append(node)
    for path in engine_files:
        try:
            text = Path(path).read_text(encoding='utf-8', errors='replace')
        except OSError:
            continue
        node = DupNode(len(nodes), [('(engine)', str(path), Path(path))], normalise_source(text, Path(path).suffix),
                       text.splitlines(), engine=True)
        nodes.append(node)
    index = collections.defaultdict(list)
    for node in nodes:
        for i in range(len(node.norm) - k + 1):
            window = hash('\n'.join(line for line, _ in node.norm[i:i + k]))
            node.windows[i] = window
            index[window].append((node.ident, i))
    pair_starts = collections.defaultdict(lambda: ([], []))
    for occurrences in index.values():
        owners = {ident for ident, _ in occurrences}
        if len(owners) < 2 or len(owners) > max_group:
            continue
        first = {}
        for ident, i in occurrences:
            first.setdefault(ident, []).append(i)
        ids = sorted(first)
        for a_pos, a in enumerate(ids):
            for b in ids[a_pos + 1:]:
                pair_starts[(a, b)][0].extend(first[a])
                pair_starts[(a, b)][1].extend(first[b])
    parent = list(range(len(nodes)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    for (a, b), (starts_a, starts_b) in pair_starts.items():
        cov_a, cov_b = cover_indexes(starts_a, k), cover_indexes(starts_b, k)
        if min(len(cov_a), len(cov_b)) >= min_lines:
            nodes[a].cover |= cov_a
            nodes[b].cover |= cov_b
            parent[find(a)] = find(b)
    groups = collections.defaultdict(list)
    for node in nodes:
        groups[find(node.ident)].append(node)
    clusters = []
    for members in groups.values():
        copies = sum(len(n.files) for n in members if not n.engine)
        solo_identical = len(members) == 1 and len(members[0].files) >= 2
        if copies < 2 and not any(n.engine for n in members):
            continue
        if len(members) == 1 and not solo_identical:
            continue
        real = [n for n in members if not n.engine]
        if not real:
            continue
        sizes = []
        for node in members:
            shared = len(node.cover) if len(members) > 1 else len(node.norm)
            if len(node.files) >= 2 and not node.engine:
                shared = len(node.norm)
            sizes.append(shared)
        sizes.sort()
        lines = sizes[len(sizes) // 2]
        if lines < min_lines:
            continue
        projects = sorted({project_key(p) for n in real for p, _, _ in n.files})
        clusters.append({'members': members, 'copies': copies, 'lines': lines, 'score': copies * lines,
                         'kind': 'identical' if len(members) == 1 else 'near-duplicate',
                         'variants': len(real), 'projects': projects,
                         'engine_template': any(n.engine for n in members)})
    clusters.sort(key=lambda c: (-c['score'], -c['copies']))
    return clusters, nodes


def longest_region(node, indexes):
    """(first, last) original line numbers of the longest contiguous shared run in a node."""
    if not node.norm:
        return 1, len(node.raw_lines)
    marks = sorted(indexes) if indexes else list(range(len(node.norm)))
    best, run = (marks[0], marks[0]), (marks[0], marks[0])
    for index in marks[1:]:
        run = (run[0], index) if index == run[1] + 1 else (index, index)
        if run[1] - run[0] > best[1] - best[0]:
            best = run
    return node.norm[best[0]][1], node.norm[min(best[1], len(node.norm) - 1)][1]


def engine_api_names(root):
    """Public item names in the engine source and the feature index: what a game could adopt instead of copying."""
    names = {}
    pattern = re.compile(r'\bpub(?:\([a-z]+\))?\s+(?:async\s+)?(?:unsafe\s+)?(?:fn|struct|enum|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)')
    base = Path(root) / 'src' / 'viewer'
    for path in sorted(base.rglob('*.rs')) if base.is_dir() else []:
        try:
            text = path.read_text(encoding='utf-8', errors='replace')
        except OSError:
            continue
        for name in pattern.findall(text):
            names.setdefault(name, str(path.relative_to(root)))
    try:
        index = json.loads((Path(root) / 'tools' / 'FEATURES.json').read_text(encoding='utf-8'))['features']
    except (OSError, ValueError, KeyError):
        index = {}
    for feature_id, feature in index.items():
        for item in feature.get('public_api') or []:
            for part in re.split(r'::', item):
                if re.fullmatch(r'[A-Za-z_]\w*', part):
                    names.setdefault(part, 'tools/FEATURES.json#' + feature_id)
    return names


def distinctive_names(cluster):
    found = collections.Counter()
    for node in cluster['members']:
        if node.engine:
            continue
        if node.cover and len(cluster['members']) > 1:
            lines = [node.raw_lines[node.norm[i][1] - 1] for i in sorted(node.cover) if i < len(node.norm)]
        else:
            lines = node.raw_lines
        for line in lines:
            for name in IDENT_RE.findall(line):
                found[name] += 1
    return [name for name in found if len(name) >= 5 and name.lower() not in GENERIC_NAMES
            and not name.startswith(('test_', 'tests'))]


def label_cluster(cluster, api):
    names = distinctive_names(cluster)
    matched = [name for name in names if name in api]
    # A shared type name is strong evidence; method names alone are common ("gravity", "alive") and need a majority.
    types = [name for name in matched if name[0].isupper() and not name.isupper() and len(name) >= 6]
    cluster['identifiers'] = sorted(names)[:12]
    ordered = [n for n in types if n in matched] + [n for n in matched if n not in types]
    evidence = [{'name': name, 'where': api[name]} for name in ordered[:6]]
    if cluster['engine_template']:
        cluster['label'] = 'engine-template-copy: expected scaffold output, keep in sync'
    elif types or (len(matched) >= 3 and len(matched) / max(1, len(names)) >= 0.5):
        cluster['label'] = 'engine-has-equivalent: adopt'
    else:
        cluster['label'] = 'no-equivalent: promote candidate'
    adopt = not cluster['label'].startswith('no-equivalent')
    cluster['engine_matches'] = evidence if adopt else []
    cluster['weak_name_matches'] = [] if adopt else [item['name'] for item in evidence[:4]]
    return cluster


def cluster_row(rank, cluster, home=None):
    examples = []
    for node in cluster['members']:
        if node.engine:
            continue
        for project, rel, path in node.files:
            first, last = longest_region(node, node.cover if len(cluster['members']) > 1 else None)
            examples.append({'project': project, 'path': tidy_path(str(path), home), 'lines': f'{first}-{last}'})
    return {'rank': rank, 'label': cluster['label'], 'kind': cluster['kind'], 'copies': cluster['copies'],
            'lines': cluster['lines'], 'score': cluster['score'], 'variants': cluster['variants'],
            'projects': cluster['projects'], 'identifiers': cluster['identifiers'],
            'engine_matches': cluster['engine_matches'], 'weak_name_matches': cluster['weak_name_matches'],
            'examples': examples[:6],
            'more_examples': max(0, len(examples) - 6)}


def engine_template_files():
    base = ROOT / 'templates'
    found = [p for p in sorted(base.rglob('*')) if p.is_file() and p.suffix in SOURCE_SUFFIXES] if base.is_dir() else []
    return found


def run_dupes(roots, min_lines=25, shingle=4, top=40, engine_root=None):
    stats = {'missing_roots': [], 'skipped_dirs': 0, 'skipped_roots': [], 'published_copies_collapsed': 0}
    files = collect_sources(roots, stats)
    clusters, _ = find_clusters(files, k=shingle, min_lines=min_lines, engine_files=engine_template_files())
    api = engine_api_names(engine_root or ROOT)
    rows = [cluster_row(rank, label_cluster(c, api)) for rank, c in enumerate(clusters[:top], 1)]
    head, dirty = git_head()
    return {'engine_commit': head, 'roots': [tidy_path(str(Path(r).expanduser())) for r in roots],
            'files_scanned': len(files), 'min_lines': min_lines, 'shingle_lines': shingle,
            'clusters_found': len(clusters), 'clusters': rows, **{k: v for k, v in stats.items() if v}}


def format_dupes(result, limit=15):
    lines = [f"{result['files_scanned']} source files under {len(result['roots'])} roots; {result['clusters_found']} "
             f"duplicate clusters (min {result['min_lines']} shared lines, {result['shingle_lines']}-line shingles); "
             f"skipped {result.get('skipped_dirs', 0)} build/branch-copy dirs, collapsed "
             f"{result.get('published_copies_collapsed', 0)} published copies"]
    if result.get('missing_roots'):
        lines.append('roots not present: ' + ', '.join(result['missing_roots']))
    for row in result['clusters'][:limit]:
        lines.append('')
        lines.append(f"#{row['rank']} score {row['score']}: {row['copies']} copies x {row['lines']} lines, "
                     f"{row['kind']}" + (f", {row['variants']} variants" if row['variants'] > 1 else '') +
                     f" -> {row['label']}")
        lines.append('   projects: ' + ', '.join(row['projects'][:10]) + (' ...' if len(row['projects']) > 10 else ''))
        if row['engine_matches']:
            lines.append('   engine already has: ' + ', '.join(f"{m['name']} ({m['where']})" for m in row['engine_matches'][:4]))
        elif row['identifiers']:
            lines.append('   defines: ' + ', '.join(row['identifiers'][:8]))
        if row['weak_name_matches']:
            lines.append('   (only generic method names also exist in the engine: ' + ', '.join(row['weak_name_matches']) + ')')
        for example in row['examples'][:4]:
            lines.append(f"   {example['path']}:{example['lines']}")
        if row['more_examples'] or len(row['examples']) > 4:
            lines.append(f"   ... and {row['more_examples'] + max(0, len(row['examples']) - 4)} more")
    return '\n'.join(lines)


def format_dupes_md(result, limit=25):
    out = ['# Duplicate code across the games', '',
           f"Generated by `python3 tools/learn.py dupes --save` at engine commit `{result['engine_commit']}`. "
           f"{result['files_scanned']} source files, {result['clusters_found']} clusters of at least "
           f"{result['min_lines']} shared normalised lines (comments, imports and whitespace ignored; "
           f"{result['shingle_lines']}-line shingles). Score is copies x shared lines. Paths and identifier names "
           f"only; no log content.", '',
           '| # | score | copies | lines | verdict | where |', '|---|---|---|---|---|---|']
    for row in result['clusters'][:limit]:
        where = ', '.join(sorted({e['path'].replace('~/', '') for e in row['examples'][:3]}))
        engine = ''
        if row['engine_matches']:
            engine = ' (' + ', '.join(m['name'] for m in row['engine_matches'][:3]) + ')'
        out.append(f"| {row['rank']} | {row['score']} | {row['copies']} | {row['lines']} | {row['label']}{engine} | "
                   f"{where}{' ...' if len(row['examples']) > 3 else ''} |")
    return '\n'.join(out) + '\n'


def cmd_dupes(args):
    roots = args.roots or list(DEFAULT_ROOTS)
    result = run_dupes(roots, args.min_lines, args.shingle, args.top)
    if not result['files_scanned']:
        raise LearnError('No source files found under: ' + ', '.join(result['roots']))
    if args.save:
        LEARNING.mkdir(parents=True, exist_ok=True)
        (LEARNING / 'dupes.json').write_text(json.dumps(result, indent=1, sort_keys=True) + '\n', encoding='utf-8')
        (LEARNING / 'dupes.md').write_text(format_dupes_md(result), encoding='utf-8')
    print(json.dumps(result, indent=2, sort_keys=True) if args.json else format_dupes(result, args.limit))
    return 0


# ---------------------------------------------------------------------------------------------------------------
# ledger: small structured friction entries (docs/learning/ledger.jsonl)
# ---------------------------------------------------------------------------------------------------------------
LEDGER = LEARNING / 'ledger.jsonl'
AREAS = ('networking', 'rendering', 'geometry', 'input', 'audio', 'physics', 'ai', 'tooling', 'docs', 'workflow',
         'platform', 'assets', 'save', 'ui', 'simulation', 'process', 'other')
STATUSES = ('open', 'promoted', 'wontfix')
MAX_TEXT = 400
MAX_PATHS = 12
MAX_WORDS = 20
GAME_RE = re.compile(r'^[A-Za-z0-9][A-Za-z0-9 ._/\-]{0,59}$')
REF_RE = re.compile(r'^[A-Za-z0-9 ._#:/,+\-]{1,80}$')
WORD_LIST_RE = re.compile(r'^[a-z0-9][a-z0-9 _.\-]{0,39}$')
HASH_TOKEN_RE = re.compile(r'\b[0-9a-f]{7,40}\b')
ENTRY_FIELDS = ('id', 'date', 'game', 'area', 'tokens', 'note', 'workaround', 'duplicated', 'trap', 'hint', 'status',
                'ref', 'keywords', 'features')
MAX_HINT = 110


def check_text(field, value, errors, required=False):
    if value is None or value == '':
        if required:
            errors.append(f'{field} is required')
        return
    if not isinstance(value, str):
        errors.append(f'{field} must be text')
        return
    if len(value) > MAX_TEXT:
        errors.append(f'{field} is {len(value)} characters; keep it under {MAX_TEXT}')
    if '\n' in value or '\r' in value:
        errors.append(f'{field} must be one line')
    probe = HASH_TOKEN_RE.sub('commit', value) if field == 'ref' else value
    label = secret_like(probe)
    if label:
        errors.append(f'{field} contains something that looks like a secret ({label}); describe the problem without '
                      f'credentials, ids or tokens')


def validate_entry(entry):
    """List of human-readable problems (never echoing a suspicious value); empty when the entry is acceptable."""
    errors = []
    if not isinstance(entry, dict):
        return ['entry must be an object']
    unknown = sorted(set(entry) - set(ENTRY_FIELDS))
    if unknown:
        errors.append('unknown fields: ' + ', '.join(safe_name(name, '<odd>') for name in unknown))
    game = entry.get('game')
    if not isinstance(game, str) or not GAME_RE.fullmatch(game) or secret_like(game):
        errors.append('game must be a short name like spooky-kart or engine')
    if entry.get('area') not in AREAS:
        errors.append('area must be one of: ' + ', '.join(AREAS))
    tokens = entry.get('tokens')
    if not isinstance(tokens, int) or isinstance(tokens, bool) or tokens < 0:
        errors.append('tokens must be a whole number >= 0 (0 = not measured)')
    check_text('note', entry.get('note'), errors, required=True)
    for field in ('workaround', 'trap', 'hint', 'ref'):
        check_text(field, entry.get(field), errors)
    if isinstance(entry.get('hint'), str) and len(entry['hint']) > MAX_HINT:
        errors.append(f"hint is {len(entry['hint'])} characters; the context packet shows it as one short line, keep it under {MAX_HINT}")
    if entry.get('ref') and not REF_RE.fullmatch(entry['ref']):
        errors.append('ref may only hold a commit, ADR or short reference (letters, digits, space and . _ # : / , + -)')
    if entry.get('status') not in STATUSES:
        errors.append('status must be one of: ' + ', '.join(STATUSES))
    if entry.get('status') == 'promoted' and not entry.get('ref'):
        errors.append('a promoted entry needs --ref (the commit or ADR that did it)')
    duplicated = entry.get('duplicated', [])
    if not isinstance(duplicated, list) or len(duplicated) > MAX_PATHS or \
            any(not isinstance(item, str) or len(item) > 160 or secret_like(item) or '\n' in item for item in duplicated):
        errors.append(f'duplicated must be at most {MAX_PATHS} plain file paths')
    for field in ('keywords', 'features'):
        items = entry.get(field, [])
        if not isinstance(items, list) or len(items) > MAX_WORDS or \
                any(not isinstance(item, str) or not WORD_LIST_RE.fullmatch(item) or secret_like(item) for item in items):
            errors.append(f'{field} must be a list of at most {MAX_WORDS} short lowercase words or feature ids')
    return errors


def load_ledger(path=None):
    """(entries, malformed line count). Bad lines are skipped, never printed."""
    path = Path(path) if path else LEDGER
    entries, bad = [], 0
    if not path.is_file():
        return entries, bad
    for raw in path.read_text(encoding='utf-8', errors='replace').splitlines():
        if not raw.strip():
            continue
        try:
            entry = json.loads(raw)
        except ValueError:
            bad += 1
            continue
        if isinstance(entry, dict):
            entries.append(entry)
        else:
            bad += 1
    return entries, bad


def next_ledger_id(entries):
    numbers = [int(m.group(1)) for e in entries if isinstance(e.get('id'), str) and (m := re.fullmatch(r'L-(\d+)', e['id']))]
    return 'L-%03d' % (max(numbers, default=0) + 1)


def split_list(value):
    return [item for item in re.split(r'[,\s]+', value.strip()) if item] if value else []


def append_entry(entry, path=None, today=None):
    """Validate and append one entry; returns the stored entry. Raises LearnError listing every problem."""
    path = Path(path) if path else LEDGER
    errors = validate_entry(entry)
    if errors:
        raise LearnError('Ledger entry rejected:\n  - ' + '\n  - '.join(errors))
    entries, _ = load_ledger(path)
    for existing in entries:
        if (existing.get('game'), existing.get('area'), existing.get('note')) == (entry['game'], entry['area'], entry['note']):
            raise LearnError(f"Ledger entry rejected: the same game/area/note is already recorded as {existing.get('id')}")
    stored = {'id': next_ledger_id(entries),
              'date': today or datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d'), **entry}
    stored = {key: stored[key] for key in ENTRY_FIELDS if key in stored and stored[key] not in (None, '', [])}
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a', encoding='utf-8') as stream:
        stream.write(json.dumps(stored, sort_keys=False) + '\n')
    return stored


def cmd_record(args):
    entry = {'game': args.game, 'area': args.area, 'tokens': args.tokens, 'note': args.note,
             'status': args.status}
    if args.workaround:
        entry['workaround'] = args.workaround
    if args.trap:
        entry['trap'] = args.trap
    if args.hint:
        entry['hint'] = args.hint
    if args.duplicated:
        entry['duplicated'] = split_list(args.duplicated)
    if args.ref:
        entry['ref'] = args.ref
    if args.keywords:
        entry['keywords'] = [w.lower() for w in split_list(args.keywords)]
    if args.features:
        entry['features'] = split_list(args.features)
    if args.dry_run:
        errors = validate_entry(entry)
        if errors:
            raise LearnError('Ledger entry rejected:\n  - ' + '\n  - '.join(errors))
        print(json.dumps(entry))
        return 0
    stored = append_entry(entry, args.ledger)
    print(json.dumps(stored) if args.json else f"recorded {stored['id']} ({stored['game']}/{stored['area']}, {stored['status']})")
    return 0


# ---------------------------------------------------------------------------------------------------------------
# modules: a one-line description of every indexed source file, so `context` can route a task to a file
# ---------------------------------------------------------------------------------------------------------------
MODULE_SUMMARY_CHARS = 110
FEATURES_FILE = ROOT / 'tools' / 'FEATURES.json'


def module_summary(path):
    """First sentence of a Rust file's leading `//!` docs, a Python docstring or a Markdown title, or None."""
    try:
        text = Path(path).read_text(encoding='utf-8', errors='replace')
    except OSError:
        return None
    if str(path).endswith('.md'):
        title = re.search(r'^#\s+(.+)$', text, re.M)
        joined = title.group(1).strip() if title else ''
    elif str(path).endswith('.rs'):
        lines = []
        for line in text.splitlines():
            if line.startswith('//!'):
                lines.append(line[3:].strip())
            elif lines or line.strip():
                break
        joined = ' '.join(l for l in lines if l)
    else:
        match = re.match(r'\s*(?:#![^\n]*\n)?\s*(?:\'\'\'|""")(.*?)(?:\'\'\'|""")', text, re.S)
        joined = ' '.join(match.group(1).split()) if match else ''
    joined = re.sub(r'\[`?([^\]`]+)`?\]\([^)]*\)', r'\1', joined)      # [text](link) -> text
    joined = re.sub(r'\[`([^\]`]+)`\]', r'\1', joined).replace('`', '')   # [`Item`] -> Item
    first = re.split(r'(?<=[.!?])\s', joined, maxsplit=1)[0].strip()
    first = re.sub(r'^(AI-[A-Z]+ [A-Z0-9-]+: )', '', first)
    if not first:
        return None
    if len(first) > MODULE_SUMMARY_CHARS:
        first = first[:MODULE_SUMMARY_CHARS - 3].rsplit(' ', 1)[0].rstrip(' ,;:') + '...'
    return first


def build_modules(root=None):
    """{path: summary} for the source files (src/, tools/, scripts/, templates/) and docs/*.md guides that indexed features own."""
    root = Path(root) if root else ROOT
    index = json.loads((root / 'tools' / 'FEATURES.json').read_text(encoding='utf-8'))
    modules = {}
    for feature in index['features'].values():
        for rel in feature.get('files', []):
            name = Path(rel).name
            source = rel.startswith(('src/', 'tools/', 'scripts/', 'templates/')) and rel.endswith(('.rs', '.py'))
            guide = rel.startswith('docs/') and rel.endswith('.md')
            if not (source or guide):
                continue
            if name.startswith('test_') or name == 'tests.rs' or '/tests/' in rel or rel in modules:
                continue
            summary = module_summary(root / rel)
            if summary:
                modules[rel] = summary
    return dict(sorted(modules.items()))


def cmd_modules(args):
    built = build_modules()
    doc = json.loads(FEATURES_FILE.read_text(encoding='utf-8'))
    current = doc.get('modules', {})
    stale = sorted(path for path in set(built) | set(current) if built.get(path) != current.get(path))
    if args.write:
        doc['modules'] = built
        FEATURES_FILE.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + '\n', encoding='utf-8')
        print(f'wrote {len(built)} module summaries to tools/FEATURES.json ({len(stale)} changed)')
        return 0
    print(f'{len(built)} indexed source files have a summary; {len(stale)} differ from tools/FEATURES.json'
          + (' (run `learn.py modules --write`)' if stale else ''))
    for path in stale[:20]:
        print('  ' + path)
    return 1 if (args.check and stale) else 0


# ---------------------------------------------------------------------------------------------------------------
# eval: does `be2.py context` surface the right tools for realistic tasks?
# ---------------------------------------------------------------------------------------------------------------
TASKS = LEARNING / 'tasks.jsonl'
RUNS = LEARNING / 'eval_runs.jsonl'
FLOOR = LEARNING / 'eval_floor.json'
CHARS_PER_TOKEN = 4          # documented approximation: packet tokens = characters / 4


def load_tasks(path=None):
    path = Path(path) if path else TASKS
    if not path.is_file():
        raise LearnError(f'No task file at {path}')
    tasks, seen = [], set()
    for number, raw in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
        if not raw.strip():
            continue
        try:
            task = json.loads(raw)
        except ValueError:
            raise LearnError(f'{path.name} line {number}: not valid JSON') from None
        problems = []
        if not isinstance(task, dict):
            problems.append('not an object')
        else:
            if not isinstance(task.get('id'), str) or not task.get('id'):
                problems.append('id must be a non-empty string')
            elif task['id'] in seen:
                problems.append('duplicate id ' + task['id'])
            if not isinstance(task.get('prompt'), str) or not task.get('prompt', '').strip():
                problems.append('prompt must be a non-empty string')
            for field in ('expect_features', 'expect_paths'):
                if not isinstance(task.get(field, []), list) or any(not isinstance(i, str) for i in task.get(field, [])):
                    problems.append(f'{field} must be a list of strings')
            if not task.get('expect_features') and not task.get('expect_paths'):
                problems.append('expect_features or expect_paths is required')
        if problems:
            raise LearnError(f'{path.name} line {number}: ' + '; '.join(problems))
        seen.add(task['id'])
        tasks.append(task)
    if not tasks:
        raise LearnError(f'{path.name} has no tasks')
    return tasks


def subprocess_context(prompt, k):
    """Run the real `python3 tools/be2.py context PROMPT --compact --limit K`; returns (stdout, error text or None)."""
    command = [sys.executable, str(ROOT / 'tools' / 'be2.py'), 'context', '--compact', '--limit', str(k), '--', prompt]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=60)
    if result.returncode:
        message = result.stderr.strip().splitlines()[-1] if result.stderr.strip() else f'exit {result.returncode}'
        try:
            message = json.loads(message).get('error', message)
        except ValueError:
            pass
        return result.stdout, str(message)[:200]
    return result.stdout, None


def score_task(task, output, k, error=None):
    """Score one packet. Features are read from the ranked matches; paths from anywhere the packet shows them."""
    expect_features, expect_paths = task.get('expect_features', []), task.get('expect_paths', [])
    selected, packet = [], {}
    if not error:
        try:
            packet = json.loads(output)
            selected = [item['id'] for item in packet.get('matches', [])][:k]
        except (ValueError, KeyError, TypeError):
            error = 'unparseable packet'
    got_features = [f for f in expect_features if f in selected]
    got_paths = [p for p in expect_paths if p in output]
    return {'task': task['id'], 'k': k, 'selected': selected,
            'feature_recall': round(len(got_features) / len(expect_features), 3) if expect_features else None,
            'path_recall': round(len(got_paths) / len(expect_paths), 3) if expect_paths else None,
            'top1': (selected[0] in expect_features) if expect_features and selected else (False if expect_features else None),
            'tokens': -(-len(output) // CHARS_PER_TOKEN),
            'missed_features': [f for f in expect_features if f not in selected],
            'missed_paths': [p for p in expect_paths if p not in output],
            **({'error': error} if error else {})}


def mean(values):
    values = [v for v in values if v is not None]
    return round(sum(values) / len(values), 3) if values else None


def aggregate_scores(rows):
    full = sum(1 for r in rows if not r['missed_features'] and not r['missed_paths'] and 'error' not in r)
    top = [None if r['top1'] is None else float(r['top1']) for r in rows]
    return {'tasks': len(rows), 'feature_recall': mean(r['feature_recall'] for r in rows),
            'path_recall': mean(r['path_recall'] for r in rows), 'top1_rate': mean(top),
            'full_hits': full, 'mean_tokens': round(sum(r['tokens'] for r in rows) / len(rows), 1) if rows else 0,
            'errors': sum(1 for r in rows if 'error' in r)}


def run_eval(tasks, k, runner=None):
    runner = runner or subprocess_context
    rows = []
    for task in tasks:
        output, error = runner(task['prompt'], k)
        rows.append(score_task(task, output, k, error))
    return rows, aggregate_scores(rows)


def read_runs(path=None):
    path = Path(path) if path else RUNS
    rows = []
    if path.is_file():
        for raw in path.read_text(encoding='utf-8').splitlines():
            try:
                row = json.loads(raw)
            except ValueError:
                continue
            if isinstance(row, dict):
                rows.append(row)
    return rows


def format_eval(aggregate, rows, tasks, k, commit):
    def pct(v):
        return 'n/a' if v is None else f'{v:.2f}'
    lines = [f"eval k={k} on {aggregate['tasks']} tasks at {commit}: feature recall {pct(aggregate['feature_recall'])}, "
             f"path recall {pct(aggregate['path_recall'])}, top-1 {pct(aggregate['top1_rate'])}, all expectations "
             f"met {aggregate['full_hits']}/{aggregate['tasks']}, mean packet {aggregate['mean_tokens']:.0f} tokens "
             f"(chars/{CHARS_PER_TOKEN}){', ' + str(aggregate['errors']) + ' errors' if aggregate['errors'] else ''}"]
    by_id = {t['id']: t for t in tasks}
    misses = [r for r in rows if r['missed_features'] or r['missed_paths'] or 'error' in r]
    if misses:
        lines.append('misses:')
    for row in misses:
        lines.append(f"  {row['task']}: {by_id[row['task']]['prompt']}")
        if row.get('error'):
            lines.append(f"      error: {row['error']}")
        if row['missed_features']:
            lines.append('      missing features: ' + ', '.join(row['missed_features']) +
                         ' (got ' + (', '.join(row['selected']) or 'nothing') + ')')
        if row['missed_paths']:
            lines.append('      missing paths: ' + ', '.join(row['missed_paths']))
    return '\n'.join(lines)


def cmd_eval(args):
    tasks = load_tasks(args.tasks)
    if not 1 <= args.k <= 5:
        raise LearnError('-k must be 1..5 (be2.py context --limit accepts at most five features)')
    rows, aggregate = run_eval(tasks, args.k)
    commit, dirty = git_head()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    run_id = stamp.replace('-', '').replace(':', '')
    if args.record:
        RUNS.parent.mkdir(parents=True, exist_ok=True)
        with RUNS.open('a', encoding='utf-8') as stream:
            for row in rows:
                stream.write(json.dumps({'kind': 'task', 'run': run_id, 'ts': stamp, 'commit': commit,
                                         'dirty': dirty, **row}, sort_keys=True) + '\n')
            stream.write(json.dumps({'kind': 'run', 'run': run_id, 'ts': stamp, 'commit': commit, 'dirty': dirty,
                                     'k': args.k, 'task_file': Path(args.tasks).name if args.tasks else TASKS.name,
                                     **aggregate, **({'note': args.note} if args.note else {})},
                                    sort_keys=True) + '\n')
    if args.set_floor:
        floor = {'k': args.k, 'feature_recall': int((aggregate['feature_recall'] or 0) * 100) / 100,
                 'path_recall': int((aggregate['path_recall'] or 0) * 100) / 100, 'commit': commit,
                 'note': 'Regression floor for tools.test_learn: recall must not drop below these (rounded down).'}
        FLOOR.write_text(json.dumps(floor, indent=2) + '\n', encoding='utf-8')
    if args.json:
        print(json.dumps({'commit': commit, 'dirty': dirty, 'k': args.k, **aggregate, 'rows': rows}, indent=2))
    else:
        print(format_eval(aggregate, rows, tasks, args.k, commit + ('+dirty' if dirty else '')))
        if args.record:
            shown = RUNS.relative_to(ROOT) if RUNS.is_relative_to(ROOT) else RUNS
            print(f'recorded {len(rows) + 1} rows to {shown}')
    return 0


# ---------------------------------------------------------------------------------------------------------------
# report: one page from the latest eval, the last dupes run and the ledger
# ---------------------------------------------------------------------------------------------------------------
REPORT = LEARNING / 'REPORT.md'


def shorten(text, limit):
    text = ' '.join(str(text).split())
    return text if len(text) <= limit else text[:limit - 3].rstrip() + '...'


def latest_runs(runs, k=3):
    """(first run row, latest run row, its task rows) for a given k."""
    run_rows = [r for r in runs if r.get('kind') == 'run' and r.get('k') == k
                and r.get('task_file', 'tasks.jsonl') == 'tasks.jsonl']
    if not run_rows:
        return None, None, []
    first, latest = run_rows[0], run_rows[-1]
    return first, latest, [r for r in runs if r.get('kind') == 'task' and r.get('run') == latest['run']
                           and r.get('k') == k]


def local_sessions_section(local_dir):
    summary_path = Path(local_dir) / 'summary.json'
    if not summary_path.is_file():
        return None
    summary = json.loads(summary_path.read_text(encoding='utf-8'))
    out = ['## Where development effort went (local only, never committed)', '',
           f"From {summary['sessions']} sessions: {summary['turns']:,} assistant turns, fresh work "
           f"{summary['fresh_work']:,} tokens, subagent share {summary['side_share']:.0%}; repeat file reads "
           f"{summary['repeat_reads']:,} of {summary['reads']:,} (no edit in between: "
           f"{summary['repeat_reads_unedited']:,}); empty searches {summary['empty_searches']:,} of "
           f"{summary['searches']:,}; large new source files in game repos {summary['from_scratch_candidates']} "
           f"({summary['from_scratch_chars']:,} chars).", '',
           '| area | sessions | turns | fresh work | subagent share |', '|---|---|---|---|---|']
    for row in summary['areas'][:10]:
        out.append(f"| {row['area']} | {row['sessions']} | {row['turns']:,} | {row['fresh_work']:,} | {row['side_share']:.0%} |")
    out.append('')
    return '\n'.join(out)


def build_report(learning=None, local_dir=None):
    learning = Path(learning) if learning else LEARNING
    runs = read_runs(learning / 'eval_runs.jsonl')
    entries, bad = load_ledger(learning / 'ledger.jsonl')
    commit, _ = git_head()
    out = ['# What the engine has learned from the games built on it', '',
           f'Generated by `python3 tools/learn.py report` at engine commit `{commit}`. Method and privacy rules: '
           '[README.md](README.md); policy: [ADR 0038](../adr/0038-engine-learns-from-development.md). '
           'This file never contains anything derived from private session logs.', '']
    first, latest, task_rows = latest_runs(runs)
    out += ['## Discovery: does `be2.py context` find the right tool?', '']
    if latest:
        def pct(v):
            return 'n/a' if v is None else f'{v:.2f}'
        out += [f"Benchmark: {latest['tasks']} real tasks in [tasks.jsonl](tasks.jsonl), recall at k=3. Packet size is "
                f"characters / {CHARS_PER_TOKEN}.", '',
                '| metric | baseline | latest |', '|---|---|---|']
        for label, key in (('feature recall', 'feature_recall'), ('path recall', 'path_recall'),
                           ('top-1 is expected', 'top1_rate'), ('tasks fully met', 'full_hits'),
                           ('mean packet tokens', 'mean_tokens')):
            fmt = (lambda v: str(v)) if key in ('full_hits', 'mean_tokens') else pct
            out.append(f"| {label} | {fmt(first[key])} | {fmt(latest[key])} |")
        out += ['', f"Baseline run `{first['run']}` at `{first['commit']}`; latest `{latest['run']}` at `{latest['commit']}`.", '']
        by_id = {}
        try:
            by_id = {t['id']: t['prompt'] for t in load_tasks(learning / 'tasks.jsonl')}
        except LearnError:
            pass
        misses = sorted((r for r in task_rows if r['missed_features'] or r['missed_paths']),
                        key=lambda r: -(len(r['missed_features']) + len(r['missed_paths'])))
        if misses:
            out += ['Top discovery misses in the latest run:', '']
            for row in misses[:10]:
                want = ', '.join(row['missed_features'] + row['missed_paths'])
                out.append(f"- `{row['task']}` {shorten(by_id.get(row['task'], ''), 70)}: missing {want}")
            out.append('')
        else:
            out += ['No misses in the latest run.', '']
    else:
        out += ['No eval run recorded yet: `python3 tools/learn.py eval`.', '']
    out += ['## Promotion candidates (code the games copied)', '']
    dupes_path = learning / 'dupes.json'
    if dupes_path.is_file():
        dupes = json.loads(dupes_path.read_text(encoding='utf-8'))
        clusters = dupes.get('clusters', [])
        promote = [c for c in clusters if c['label'].startswith('no-equivalent')]
        adopt = [c for c in clusters if c['label'].startswith('engine-has-equivalent')]
        out += [f"{dupes['files_scanned']} source files scanned at `{dupes['engine_commit']}`; "
                f"{dupes['clusters_found']} duplicate clusters. Details: [dupes.md](dupes.md).", '',
                'No equivalent in the engine (promote):', '']
        for c in promote[:8]:
            where = ', '.join(sorted({e['path'].replace('~/', '') for e in c['examples'][:2]}))
            out.append(f"- score {c['score']}: {c['copies']} copies x {c['lines']} lines "
                       f"({', '.join(c['projects'][:5])}) e.g. `{where}`")
        out += ['', 'The engine already has it (games should adopt):', '']
        for c in adopt[:5]:
            names = ', '.join(m['name'] for m in c['engine_matches'][:3])
            where = ', '.join(sorted({e['path'].replace('~/', '') for e in c['examples'][:2]}))
            out.append(f"- score {c['score']}: {c['copies']} copies x {c['lines']} lines, engine: {names}; `{where}`")
        out.append('')
    else:
        out += ['No dupes run saved yet: `python3 tools/learn.py dupes --save`.', '']
    out += ['## Friction ledger', '']
    by_status = collections.Counter(e.get('status') for e in entries)
    tokens_by_status = collections.Counter()
    for e in entries:
        tokens_by_status[e.get('status')] += e.get('tokens', 0) if isinstance(e.get('tokens'), int) else 0
    out += [f"{len(entries)} entries: " + ', '.join(f"{by_status[s]} {s}" for s in STATUSES) +
            (f" ({bad} unreadable lines skipped)" if bad else '') + '. Measured tokens: ' +
            ', '.join(f"{tokens_by_status[s]:,} {s}" for s in STATUSES) + '.', '']
    opened = [e for e in entries if e.get('status') == 'open']
    if opened:
        out += ['Open items (the next promotion candidates):', '']
        for e in sorted(opened, key=lambda e: -(e.get('tokens') or 0)):
            ref = f" [{e['ref']}]" if e.get('ref') else ''
            out.append(f"- `{e.get('id')}` {e.get('game')}/{e.get('area')}"
                       f"{', ' + format(e['tokens'], ',') + ' tokens' if e.get('tokens') else ''}: "
                       f"{shorten(e.get('note', ''), 150)}{ref}")
        out.append('')
    costly = sorted((e for e in entries if e.get('tokens')), key=lambda e: -e['tokens'])[:5]
    if costly:
        out += ['Most expensive recorded friction:', '']
        out += [f"- `{e['id']}` {e['tokens']:,} tokens, {e['game']}/{e['area']}, {e['status']}: {shorten(e['note'], 110)}"
                for e in costly]
        out.append('')
    out += ['## Where development effort went', '',
            'Produced locally from your own session logs, so it is not part of this committed file: run '
            '`python3 tools/learn.py sessions` (writes only to the git-ignored `.learning/` folder), then '
            '`python3 tools/learn.py report`, which also writes `.learning/REPORT.md` with that section added.', '']
    text = '\n'.join(out)
    local = local_sessions_section(local_dir) if local_dir else None
    return text, (text.replace('## Where development effort went\n', local.rstrip() + '\n\n## How to produce the local effort section\n', 1)
                  if local else None)


def cmd_report(args):
    text, local_text = build_report(LEARNING, LOCAL)
    out = Path(args.out) if args.out else REPORT
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding='utf-8')
    print(f'wrote {out}')
    if local_text:
        LOCAL.mkdir(parents=True, exist_ok=True)
        (LOCAL / 'REPORT.md').write_text(local_text, encoding='utf-8')
        print(f'wrote {LOCAL / "REPORT.md"} (adds the local-only sessions section; git-ignored)')
    return 0


# ---------------------------------------------------------------------------------------------------------------
def build_parser():
    parser = argparse.ArgumentParser(prog='learn.py', description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    s = sub.add_parser('sessions', help='aggregate Claude Code session logs (structure only, never content)')
    s.add_argument('--logs', action='append', metavar='DIR', help='log directory or file (repeatable; default ~/.claude/projects)')
    s.add_argument('--since', metavar='DATE', help='only records on or after this date (YYYY-MM-DD)')
    s.add_argument('--out', metavar='DIR', help='output folder (default .learning/, which git ignores)')
    s.add_argument('--write-threshold', type=int, default=DEFAULT_WRITE_THRESHOLD, metavar='CHARS',
                   help=f'a Write of at least this many characters of source counts as a from-scratch candidate (default {DEFAULT_WRITE_THRESHOLD})')
    s.add_argument('--allow-tracked-out', action='store_true', help=argparse.SUPPRESS)
    s.add_argument('--json', action='store_true', help='print the aggregate summary as JSON')
    d = sub.add_parser('dupes', help='find code the games copied and check whether the engine already has it')
    d.add_argument('--roots', action='append', metavar='DIR', help='repo/games folder to scan (repeatable; default: the known game repos)')
    d.add_argument('--min-lines', type=int, default=25, help='shared normalised lines needed to report a cluster (default 25)')
    d.add_argument('--shingle', type=int, default=4, help='lines per shingle window (default 4)')
    d.add_argument('--top', type=int, default=40, help='clusters kept in the result (default 40)')
    d.add_argument('--limit', type=int, default=15, help='clusters printed in text mode (default 15)')
    d.add_argument('--save', action='store_true', help='write docs/learning/dupes.json and dupes.md')
    d.add_argument('--json', action='store_true')
    e = sub.add_parser('eval', help='score be2.py context against docs/learning/tasks.jsonl')
    e.add_argument('--tasks', metavar='FILE', help='task file (default docs/learning/tasks.jsonl)')
    e.add_argument('-k', type=int, default=3, help='features considered per packet (default 3, the context default; max 5)')
    e.add_argument('--no-record', dest='record', action='store_false', help='do not append rows to docs/learning/eval_runs.jsonl')
    e.add_argument('--note', help='label stored on the run row, e.g. "baseline"')
    e.add_argument('--set-floor', action='store_true', help='write docs/learning/eval_floor.json from this run')
    e.add_argument('--json', action='store_true')
    r = sub.add_parser('report', help='write docs/learning/REPORT.md')
    r.add_argument('--out', metavar='FILE', help='write the report here instead of docs/learning/REPORT.md')
    c = sub.add_parser('record', help='append one friction entry to docs/learning/ledger.jsonl')
    c.add_argument('--game', required=True, help='game or repo, e.g. spooky-kart, deadfall, engine')
    c.add_argument('--area', required=True, choices=AREAS)
    c.add_argument('--tokens', required=True, type=int, help='approximate tokens it cost (0 = not measured)')
    c.add_argument('--note', required=True, help=f'what happened, one line under {MAX_TEXT} characters')
    c.add_argument('--workaround', help='what you did instead')
    c.add_argument('--duplicated', help='comma-separated paths of code you had to copy or write that others will too')
    c.add_argument('--trap', help='the silent failure to warn the next agent about')
    c.add_argument('--hint', help=f'one line (under {MAX_HINT} chars) that be2.py context shows: the trap, or what already solves it')
    c.add_argument('--status', choices=STATUSES, default='open')
    c.add_argument('--ref', help='commit or ADR that fixed it (required when promoted)')
    c.add_argument('--keywords', help='comma-separated lowercase words a future task would use (helps retrieval)')
    c.add_argument('--features', help='comma-separated FEATURES.json ids this relates to')
    c.add_argument('--ledger', metavar='FILE', help=argparse.SUPPRESS)
    c.add_argument('--dry-run', action='store_true', help='validate and print without writing')
    c.add_argument('--json', action='store_true')
    m = sub.add_parser('modules', help='refresh the per-file summaries in tools/FEATURES.json that context uses to route to a file')
    m.add_argument('--write', action='store_true', help='write the "modules" map into tools/FEATURES.json')
    m.add_argument('--check', action='store_true', help='exit 1 when the committed map is stale')
    n = sub.add_parser('scan', help='count secret-like values in generated files (counts only)')
    n.add_argument('paths', nargs='+')
    n.add_argument('--where', action='store_true', help='also print key paths of matches (never the values)')
    n.add_argument('--json', action='store_true')
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    handler = {'sessions': cmd_sessions, 'dupes': cmd_dupes, 'eval': cmd_eval, 'report': cmd_report,
               'record': cmd_record, 'scan': cmd_scan, 'modules': cmd_modules}[args.command]
    try:
        return handler(args)
    except LearnError as error:
        print(f'learn.py: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
