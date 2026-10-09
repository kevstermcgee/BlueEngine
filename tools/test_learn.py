"""Tests for tools/learn.py. Synthetic fixtures only: no network, no real session logs, no real repos except one
smoke test that skips cleanly when the game checkouts are absent. Every fake secret below is invented."""
import contextlib
import io
import json
import subprocess
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

from tools import learn, workflow

ROOT = Path(__file__).resolve().parents[1]

# Invented credentials, planted in message text, tool input and tool output. They must never reach any output.
FAKE_UUID = '5d41402a-bc4b-4a76-b971-9e5d4a2c1f00'
FAKE_KEY = 'sk-FAKEfakeFAKE1234567890abcdefGHIJ'
FAKE_PASSWORD = 'hunter2-correct-horse-battery'
FAKE_HEX = 'deadbeefcafebabe0123456789abcdef0123456789'
SECRETS = (FAKE_UUID, FAKE_KEY, FAKE_PASSWORD, FAKE_HEX, 'FAKE-DUCKDNS-TOKEN')


def run_cli(*argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = learn.main(list(argv))
    return code, out.getvalue(), err.getvalue()


# ---------------------------------------------------------------------------------------------------------------
# synthetic session logs
# ---------------------------------------------------------------------------------------------------------------
HOME = '/home/tester'


class Log:
    """Builds one JSONL log in the shape Claude Code writes (verified against real logs by structure only)."""

    def __init__(self, session='11111111-2222-4333-8444-555555555555'):
        self.session = session
        self.records = []
        self.clock = 0

    def stamp(self):
        self.clock += 7
        return f'2026-10-01T10:{self.clock // 60:02d}:{self.clock % 60:02d}.000Z'

    def common(self, kind, cwd, side, agent):
        record = {'type': kind, 'sessionId': self.session, 'cwd': cwd, 'gitBranch': 'main',
                  'isSidechain': side, 'timestamp': self.stamp(), 'uuid': f'u{len(self.records)}'}
        if agent:
            record['agentId'] = agent
        return record

    def assistant(self, message_id, blocks, usage, cwd=HOME + '/BlueEngine', side=False, agent=None):
        record = self.common('assistant', cwd, side, agent)
        record['message'] = {'id': message_id, 'role': 'assistant', 'model': 'claude-test-1',
                             'content': blocks, 'usage': usage}
        self.records.append(record)
        return self

    def result(self, tool_id, content, cwd=HOME + '/BlueEngine', side=False, agent=None):
        record = self.common('user', cwd, side, agent)
        record['message'] = {'role': 'user', 'content': [{'type': 'tool_result', 'tool_use_id': tool_id,
                                                          'content': content}]}
        self.records.append(record)
        return self

    def prompt(self, text, cwd=HOME + '/BlueEngine'):
        record = self.common('user', cwd, False, None)
        record['message'] = {'role': 'user', 'content': text}
        self.records.append(record)
        return self

    def write(self, path, extra_lines=()):
        path = Path(path)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('w', encoding='utf-8') as stream:
            for record in self.records:
                stream.write(json.dumps(record) + '\n')
            for line in extra_lines:
                stream.write(line + '\n')
        return path


def usage(i=1, o=1, cw=0, cr=0):
    return {'input_tokens': i, 'output_tokens': o, 'cache_creation_input_tokens': cw, 'cache_read_input_tokens': cr}


def tool(tool_id, name, **arguments):
    return {'type': 'tool_use', 'id': tool_id, 'name': name, 'input': arguments}


def aggregate(*paths, **options):
    options.setdefault('home', HOME)
    return learn.scan_logs(list(paths), **options)


def build_fixture(directory):
    """A parent session, one subagent log, repeated reads, a big write, empty and non-empty searches."""
    log = Log()
    log.prompt('please fix it, my password is ' + FAKE_PASSWORD)
    log.assistant('m1', [{'type': 'thinking', 'thinking': 'secret thought ' + FAKE_KEY},
                         {'type': 'text', 'text': 'I will use ' + FAKE_UUID}],
                  usage(100, 10, 1000, 5000))
    # The same message streamed again with a higher output count: counted once, with the maximum.
    log.assistant('m1', [tool('t1', 'Read', file_path=HOME + '/BlueEngine/src/viewer/net.rs')], usage(100, 40, 1000, 5000))
    log.result('t1', 'file body containing ' + FAKE_KEY)
    log.assistant('m2', [tool('t2', 'Read', file_path=HOME + '/BlueEngine/src/viewer/net.rs')], usage(10, 5, 0, 6000))
    log.result('t2', 'file body again ' + FAKE_HEX)
    log.assistant('m3', [tool('t3', 'Edit', file_path=HOME + '/BlueEngine/src/viewer/net.rs',
                              old_string=FAKE_KEY, new_string=FAKE_PASSWORD)], usage(10, 5, 0, 6100))
    log.assistant('m4', [tool('t4', 'Read', file_path=HOME + '/BlueEngine/src/viewer/net.rs')], usage(10, 5, 0, 6200))
    log.assistant('m5', [tool('t5', 'Grep', pattern=FAKE_KEY, path=HOME)], usage(10, 5, 0, 6300))
    log.result('t5', 'No matches found')
    log.assistant('m6', [tool('t6', 'Bash', command=f'cd /x && grep -rn "{FAKE_PASSWORD}" src | head')], usage(10, 5, 0, 6400))
    log.result('t6', '(Bash completed with no output)')
    log.assistant('m7', [tool('t7', 'Bash', command=f'grep -rn needle src')], usage(10, 5, 0, 6400))
    log.result('t7', 'src/a.rs:1: needle ' + FAKE_KEY)
    log.assistant('m8', [tool('t8', 'Bash', command=f'curl -H "Authorization: Bearer {FAKE_KEY}" https://example.invalid')],
                  usage(10, 5, 0, 6400))
    log.result('t8', FAKE_KEY)
    big = 'fn main() {}\n' * 400
    log.assistant('m9', [tool('t9', 'Write', file_path=HOME + '/SpookyKart/src/big.rs', content=big)], usage(10, 500, 0, 6400),
                  cwd=HOME + '/SpookyKart')
    log.assistant('m10', [tool('t10', 'Write', file_path=HOME + '/BlueEngine/src/viewer/big.rs', content=big)],
                  usage(10, 5, 0, 6400))
    log.assistant('m11', [tool('t11', 'Write', file_path=HOME + '/SpookyKart/notes.md', content='x' * 9000)],
                  usage(10, 5, 0, 6400), cwd=HOME + '/SpookyKart')
    parent = log.write(Path(directory) / '-home-tester' / f'{log.session}.jsonl')
    sub = Log(log.session)
    sub.assistant('s1', [tool('a1', 'Read', file_path=HOME + '/BlueEngine/src/viewer/net.rs')], usage(50, 100, 0, 3000),
                  side=True, agent='agent-one')
    sub.assistant('s2', [tool('a2', 'Read', file_path=HOME + '/BlueEngine/src/viewer/net.rs')], usage(50, 100, 0, 3000),
                  side=True, agent='agent-one')
    sub.write(Path(directory) / '-home-tester' / log.session / 'subagents' / 'agent-one.jsonl')
    return parent


class SessionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.logs = Path(self.tmp.name) / 'logs'
        build_fixture(self.logs)

    def rows(self):
        agg = aggregate(self.logs)
        return learn.build_rows(agg)

    def test_tokens_are_deduplicated_per_message_with_the_maximum_seen(self):
        sessions, areas, summary = self.rows()
        self.assertEqual(summary['sessions'], 1)
        # Parent: output 40+5*8+500+5+5 ... computed from the maximum per message id, not summed per record.
        parent_output = 40 + 5 + 5 + 5 + 5 + 5 + 5 + 5 + 500 + 5 + 5
        self.assertEqual(summary['usage']['output'], parent_output + 200)
        self.assertEqual(summary['usage']['input'], 100 + 10 * 10 + 100)
        self.assertEqual(summary['turns'], 11 + 2)
        self.assertEqual(summary['side_turns'], 2)
        self.assertAlmostEqual(summary['side_share'], summary['side_fresh_work'] / summary['fresh_work'], places=3)
        self.assertGreater(summary['side_share'], 0)

    def test_tool_names_reads_repeats_and_searches(self):
        _, _, summary = self.rows()
        self.assertEqual(summary['tools']['Read'], 5)
        self.assertEqual(summary['tools']['Edit'], 1)
        self.assertEqual(summary['tools']['Write'], 3)
        # Parent reads net.rs 3 times (one repeat with no edit between, one after the edit), the subagent twice
        # in its own context (one repeat): 3 repeats in all, 2 of them with no edit in between.
        self.assertEqual(summary['reads'], 5)
        self.assertEqual(summary['repeat_reads'], 3)
        self.assertEqual(summary['repeat_reads_unedited'], 2)
        self.assertEqual(summary['top_repeated_paths'][0]['path'], '~/BlueEngine/src/viewer/net.rs')
        self.assertEqual(summary['searches'], 3)
        self.assertEqual(summary['empty_searches'], 2)
        self.assertEqual(summary['bash_programs']['grep'], 2)
        self.assertEqual(summary['bash_programs']['curl'], 1)

    def test_large_new_source_files_are_game_repo_only_and_path_plus_length_only(self):
        _, areas, summary = self.rows()
        self.assertEqual(summary['from_scratch_candidates'], 1)
        item = summary['from_scratch_top'][0]
        self.assertEqual(set(item), {'path', 'chars', 'area'})
        self.assertEqual((item['path'], item['chars'], item['area']), ('~/SpookyKart/src/big.rs', 5200, 'SpookyKart'))
        kart = next(row for row in areas if row['area'] == 'SpookyKart')
        self.assertEqual(kart['large_writes'], 1)

    def test_areas_come_from_touched_paths_before_cwd(self):
        sessions, areas, _ = self.rows()
        names = {row['area'] for row in areas}
        self.assertTrue({'BlueEngine', 'SpookyKart'} <= names)
        self.assertEqual(learn.area_of('~/BlueEngineGames-df-audio/games/deadfall/src/x.rs'), 'BlueEngineGames/games/deadfall')
        self.assertEqual(learn.area_of('~/BlueEngine-eng-hub/src/a.rs'), 'BlueEngine')
        self.assertEqual(learn.area_of('~/SpookyKart-fb-x/src/a.rs'), 'SpookyKart')
        self.assertEqual(learn.area_of('~/deadfall-target-ch2/a.rs'), 'deadfall')
        self.assertEqual(learn.area_of('/tmp/x'), '(outside-home)')

    def test_session_ids_are_hashed_not_stored(self):
        sessions, _, _ = self.rows()
        self.assertEqual(len(sessions[0]['session']), 8)
        self.assertNotIn('11111111', json.dumps(sessions))

    def test_since_filters_records_by_timestamp(self):
        import datetime
        later = datetime.datetime(2026, 10, 2, tzinfo=datetime.timezone.utc)
        agg = aggregate(self.logs, since=later)
        self.assertEqual(learn.build_rows(agg)[2]['turns'], 0)


class PrivacyTests(unittest.TestCase):
    """HARD RULE: nothing from message text, tool input, tool output, commands or names reaches any output."""

    def test_planted_secrets_appear_nowhere_in_any_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            logs, out = Path(tmp) / 'logs', Path(tmp) / 'out'
            parent = build_fixture(logs)
            # More hiding places: a secret in the cwd, branch, a tool name, an unknown record type, a model name,
            # a file path, a malformed line and a truncated final line.
            hostile = Log('22222222-3333-4444-8555-666666666666')
            hostile.assistant('h1', [tool('x1', 'Tool' + FAKE_KEY, command=FAKE_KEY),
                                     tool('x2', 'Read', file_path=HOME + '/proj/' + FAKE_UUID + '/notes.txt'),
                                     tool('x3', 'Read', file_path=f'{HOME}/proj/password={FAKE_PASSWORD}/a.rs'),
                                     tool('x4', 'Bash', command=f'{FAKE_KEY}=1 ./run --token {FAKE_KEY}')],
                              usage(1, 1), cwd=HOME + '/' + FAKE_UUID)
            hostile.records[-1]['gitBranch'] = 'feature/' + FAKE_KEY
            hostile.records[-1]['message']['model'] = FAKE_KEY
            hostile.records.append({'type': 'weird-' + FAKE_KEY, 'sessionId': 's', 'content': FAKE_KEY})
            hostile.write(Path(logs) / 'other' / 'hostile.jsonl',
                          extra_lines=['{"type": "user", "message": {"content": "' + FAKE_PASSWORD + ' broken',
                                       FAKE_KEY + ' not json at all'])
            for extra in ([], ['--json']):
                code, stdout, stderr = run_cli('sessions', '--logs', str(logs), '--out', str(out), *extra)
                self.assertEqual(code, 0, stderr)
                blobs = [stdout, stderr] + [p.read_text(encoding='utf-8') for p in out.iterdir()]
                for blob in blobs:
                    for secret in SECRETS + (FAKE_KEY[3:], FAKE_PASSWORD.split('-')[0]):
                        self.assertNotIn(secret, blob)
            summary = json.loads((out / 'summary.json').read_text())
            self.assertEqual(summary['malformed_lines'], 2)
            self.assertIn('<redacted-path>', json.dumps(summary) + (out / 'sessions.jsonl').read_text()
                          + (out / 'areas.jsonl').read_text())
            self.assertEqual(learn.scan_paths(sorted(out.iterdir()))['total'], 0)

    def test_errors_never_contain_log_content(self):
        with tempfile.TemporaryDirectory() as tmp:
            empty = Path(tmp) / 'empty'
            empty.mkdir()
            code, stdout, stderr = run_cli('sessions', '--logs', str(empty), '--out', str(Path(tmp) / 'o'))
            self.assertEqual(code, 2)
            self.assertIn('No *.jsonl', stderr)
            code, _, stderr = run_cli('sessions', '--logs', str(empty), '--since', FAKE_KEY)
            self.assertEqual(code, 2)
            logfile = Path(tmp) / 'one.jsonl'
            logfile.write_text(FAKE_KEY + '\n{"type":"assistant"}\n[1,2]\n', encoding='utf-8')
            code, stdout, stderr = run_cli('sessions', '--logs', str(logfile), '--out', str(Path(tmp) / 'o2'))
            self.assertEqual(code, 0)
            self.assertNotIn(FAKE_KEY, stdout + stderr)

    def test_refuses_to_write_inside_the_repo_outside_the_ignored_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            logfile = Log().assistant('m', [], usage()).write(Path(tmp) / 'a.jsonl')
            code, _, stderr = run_cli('sessions', '--logs', str(logfile), '--out', str(ROOT / 'docs' / 'learning'))
            self.assertEqual(code, 2)
            self.assertIn('git-ignored', stderr)
        ignored = subprocess_check_ignore('.learning/x')
        self.assertTrue(ignored, '.learning/ must be git-ignored')

    def test_leak_scan_catches_value_shapes_and_the_output_is_withheld(self):
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / 'bad.jsonl'
            bad.write_text(json.dumps({'ok': 'fine', 'x': FAKE_UUID, 'y': 'api_key=' + FAKE_KEY, 'z': 'my token is here'}) + '\n')
            result = learn.scan_paths([bad])
            self.assertEqual(result['total'], 3)
            self.assertEqual(run_cli('scan', str(bad))[0], 1)
            clean = Path(tmp) / 'clean.jsonl'
            clean.write_text(json.dumps({'input_tokens': 5, 'path': '~/BlueEngine/src/viewer/net.rs', 'area': 'SpookyKart'}) + '\n')
            self.assertEqual(learn.scan_paths([clean])['total'], 0)

    def test_secret_like_classes(self):
        for text, label in [(FAKE_UUID, 'uuid'), (FAKE_HEX, 'long-hex'), (FAKE_KEY, 'credential-prefix'),
                            ('password is hunter2', 'credential-word'), ('Authorization: Bearer abcdefg', 'credential-word'), ('auth: abcd1234efgh5678', 'credential-assignment'), ('{"token": "abcd1234efgh5678"}', 'credential-assignment'),
                            ('xY7' * 12, 'long-base64'), ('api_key=abcdef123456', 'credential-assignment')]:
            self.assertEqual(learn.secret_like(text), label, text)
        for text in ('Template::quad_facing winds a quad', 'commit 7ca1536 fixed it', 'src/viewer/netplay/server.rs',
                     'tokens cost 50k', 'ADR 0036 shadows'):
            self.assertIsNone(learn.secret_like(text), text)
        self.assertEqual(learn.secret_like('tokens cost 50k', strict=True), 'credential-word')


def subprocess_check_ignore(path):
    import subprocess
    result = subprocess.run(['git', 'check-ignore', '-q', path], cwd=ROOT)
    return result.returncode == 0


class GarbageTests(unittest.TestCase):
    def test_malformed_truncated_and_unknown_records_are_counted_not_fatal(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'g.jsonl'
            good = Log().assistant('m1', [tool('t', 'Read', file_path=HOME + '/a.rs')], usage(1, 2, 3, 4))
            lines = [json.dumps(r) for r in good.records]
            lines += ['', '{"type":', 'null', '"text"', '[1, 2]', '\x00\x01\x02 binary', '{"type": "assistant"}',
                      '{"type": "assistant", "message": "not a dict", "sessionId": "z"}',
                      '{"type": "assistant", "sessionId": "z", "message": {"id": "q", "usage": "bad", "content": 5}}',
                      '{"type": "queue-operation", "operation": "enqueue"}', '{"type": 5}',
                      '{"type": "user", "sessionId": "z", "message": {"content": [{"type": "tool_result"}, 7, null]}}',
                      '{"type": "assistant", "sessionId": "z", "timestamp": "garbage", "message": {"id": "w", "usage":'
                      ' {"input_tokens": "9", "output_tokens": true, "cache_read_input_tokens": -1}, "content": ['
                      '{"type": "tool_use", "name": 5, "input": "str"}, {"type": "tool_use", "name": "Read", "input": {"file_path": 3}}]}}',
                      '{"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Read", "input": {"file_pa']
            path.write_text('\n'.join(lines), encoding='utf-8')
            (Path(tmp) / 'dir.jsonl').mkdir()
            agg = learn.scan_logs([path, Path(tmp)], home=HOME)
            _, _, summary = learn.build_rows(agg)
            self.assertGreaterEqual(summary['malformed_lines'], 4)
            self.assertIn('queue-operation', summary['ignored_record_types'])
            self.assertEqual(summary['usage']['output'], 2)
            self.assertEqual(summary['tools']['Read'], 2)

    def test_unreadable_file_is_counted(self):
        agg = learn.Aggregate()
        self.assertEqual(list(learn.iter_records(Path('/nonexistent/x.jsonl'), agg)), [])
        self.assertEqual(agg.stats['unreadable_files'], 1)

    def test_text_summary_has_no_content(self):
        with tempfile.TemporaryDirectory() as tmp:
            build_fixture(Path(tmp) / 'logs')
            _, _, summary = learn.build_rows(aggregate(Path(tmp) / 'logs'))
            text = learn.format_summary(summary)
            self.assertIn('repeat reads', text)
            self.assertIsNone(learn.secret_like(text))


# ---------------------------------------------------------------------------------------------------------------
# dupes
# ---------------------------------------------------------------------------------------------------------------
def make_physics(extra=''):
    body = ['pub struct Physics {', '    world: u32,', '}', '']
    for i in range(40):
        body += [f'pub fn step_{i}(&mut self, dt: f32) -> f32 {{', f'    let v = self.world as f32 * {i}.0 + dt;',
                 f'    if v > {i + 1}.5 {{ v - 1.0 }} else {{ v + 2.0 }}', '}']
    return '\n'.join(body) + extra + '\n'


def make_server(name):
    lines = [f'//! The dedicated {name} server. Different words every time {name}.', 'use std::env;', 'fn main() {']
    for i in range(30):
        lines += [f'    let flag_{i} = args.next().unwrap_or_default(); // note about {name}',
                  f'    if flag_{i} == "--opt{i}" {{ cfg.value_{i} = parse_value(&mut it, "{name}-{i}")?; }}']
    lines += ['    println!("[Server] {} listening", "' + name + '");', '}']
    return '\n'.join(lines) + '\n'


class DupesTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.games = base / 'games'
        for game in ('tumble-maze', 'wobble-tower', 'clockwork-pinball'):
            (self.games / game / 'src').mkdir(parents=True)
            (self.games / game / 'src' / 'physics.rs').write_text(make_physics(), encoding='utf-8')
        for game, label in (('prop-hunt', 'Prop Hunt'), ('slap-stick', 'Slap Stick'), ('kart-race', 'Kart Race')):
            (self.games / game / 'src' / 'bin').mkdir(parents=True)
            (self.games / game / 'src' / 'bin' / f'{game}-server.rs').write_text(make_server(label), encoding='utf-8')
        # a tiny shared helper is below the threshold; a copy in target/ and in a branch worktree must be ignored
        (self.games / 'tumble-maze' / 'target' / 'debug').mkdir(parents=True)
        (self.games / 'tumble-maze' / 'target' / 'debug' / 'physics.rs').write_text(make_physics(), encoding='utf-8')
        (self.games / 'wobble-tower' / 'src' / 'tiny.rs').write_text('fn a() {}\nfn b() {}\n', encoding='utf-8')
        (self.games / 'tumble-maze' / 'src' / 'tiny.rs').write_text('fn a() {}\nfn b() {}\n', encoding='utf-8')
        self.worktree = base / 'games-eng-copy'
        shutil.copytree(self.games / 'tumble-maze', self.worktree)
        # a published copy of a project under a different folder name collapses into the original
        self.published = base / 'TumbleMaze'
        (self.published / 'src').mkdir(parents=True)
        (self.published / 'src' / 'physics.rs').write_text(make_physics(), encoding='utf-8')
        # an engine with an equivalent of one cluster's type name, and none for the other
        self.engine = base / 'engine'
        (self.engine / 'src' / 'viewer').mkdir(parents=True)
        (self.engine / 'src' / 'viewer' / 'sim.rs').write_text('pub struct Physics {}\npub fn unrelated_thing() {}\n')
        (self.engine / 'tools').mkdir()
        (self.engine / 'tools' / 'FEATURES.json').write_text(json.dumps({'features': {}}))

    def run_dupes(self, **options):
        return learn.run_dupes([self.games, self.published, self.worktree], engine_root=self.engine, **options)

    def test_identical_files_and_near_duplicate_boilerplate_are_clustered(self):
        result = self.run_dupes(min_lines=20)
        by_kind = {c['kind']: c for c in result['clusters']}
        physics = next(c for c in result['clusters'] if any('physics.rs' in e['path'] for e in c['examples']))
        self.assertEqual(physics['kind'], 'identical')
        self.assertEqual(physics['copies'], 3)       # target/, the worktree and the published copy do not count
        self.assertEqual(physics['score'], physics['copies'] * physics['lines'])
        server = next(c for c in result['clusters'] if any('-server.rs' in e['path'] for e in c['examples']))
        self.assertEqual(server['kind'], 'near-duplicate')
        self.assertEqual(server['copies'], 3)
        self.assertEqual(result['published_copies_collapsed'], 1)
        self.assertEqual(len(result['clusters']), 2, [c['examples'] for c in result['clusters']])
        self.assertTrue(all('tiny.rs' not in e['path'] for c in result['clusters'] for e in c['examples']))
        self.assertTrue(all('target' not in e['path'] and 'eng-copy' not in e['path']
                            for c in result['clusters'] for e in c['examples']))

    def test_engine_cross_check_labels(self):
        result = self.run_dupes(min_lines=20)
        physics = next(c for c in result['clusters'] if c['kind'] == 'identical')
        server = next(c for c in result['clusters'] if c['kind'] == 'near-duplicate')
        self.assertEqual(physics['label'], 'engine-has-equivalent: adopt')
        self.assertEqual(physics['engine_matches'][0]['name'], 'Physics')
        self.assertEqual(server['label'], 'no-equivalent: promote candidate')

    def test_min_lines_and_ranking(self):
        self.assertEqual(self.run_dupes(min_lines=500)['clusters'], [])
        result = self.run_dupes(min_lines=20)
        scores = [c['score'] for c in result['clusters']]
        self.assertEqual(scores, sorted(scores, reverse=True))

    def test_normalisation_ignores_comments_imports_formatting_and_literals(self):
        a = learn.normalise_source('use a::b;\n// c\nlet x   =  5;\nfoo( "hi" ) ;\n', '.rs')
        b = learn.normalise_source('let x = 9;\n/* z */ foo("bye");\n', '.rs')
        self.assertEqual([l for l, _ in a], [l for l, _ in b])
        named = learn.normalise_source('let a = PropHunt::new(); let b = "prop-hunt"; prop_hunt()', '.rs', 'prop-hunt')
        self.assertNotIn('PropHunt', named[0][0])
        self.assertEqual([l for l, _ in learn.normalise_source('# comment\nx = 1\n', '.py')], ['x=0'])

    def test_text_and_markdown_output_carry_only_paths_names_and_counts(self):
        result = self.run_dupes(min_lines=20)
        text = learn.format_dupes(result) + learn.format_dupes_md(result)
        self.assertIn('score', text)
        self.assertIsNone(learn.secret_like(text), 'paths and counts only')

    def test_real_physics_triplicate_smoke(self):
        base = Path.home() / 'BlueEngineGames' / 'games'
        games = [base / g / 'src' / 'physics.rs' for g in ('tumble-maze', 'wobble-tower', 'clockwork-pinball')]
        if not all(p.is_file() for p in games):
            self.skipTest('game checkouts are not present on this machine')
        if len({p.read_bytes() for p in games}) != 1:
            self.skipTest('the physics.rs copies have diverged or been promoted since this was written')
        result = learn.run_dupes([base], min_lines=100)
        cluster = next(c for c in result['clusters'] if any(e['path'].endswith('/tumble-maze/src/physics.rs') for e in c['examples']))
        self.assertEqual((cluster['kind'], cluster['copies']), ('identical', 3))
        self.assertGreaterEqual(cluster['lines'], 150)


# ---------------------------------------------------------------------------------------------------------------
# ledger and record
# ---------------------------------------------------------------------------------------------------------------
def entry(**overrides):
    base = {'game': 'spooky-kart', 'area': 'networking', 'tokens': 1200, 'note': 'Lobby stalled under packet loss.',
            'status': 'open'}
    base.update(overrides)
    return base


class LedgerTests(unittest.TestCase):
    def test_close_and_merge_preserve_history_and_refuse_unknown_ids(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'ledger.jsonl'
            rows = [entry(id='L-001', date='2026-10-09', keywords=['audio']),
                    entry(id='L-002', date='2026-10-09', keywords=['mute'])]
            path.write_text(''.join(json.dumps(row) + '\n' for row in rows))
            before = path.read_bytes()
            with self.assertRaises(learn.LearnError):
                learn.close_entries(path, ['L-999'], '90949ab')
            self.assertEqual(path.read_bytes(), before)
            learn.merge_entry(path, 'L-002', 'L-001')
            merged, bad = learn.load_ledger(path)
            self.assertEqual(bad, 0)
            self.assertEqual(merged[1]['duplicate_of'], 'L-001')
            self.assertEqual(merged[1]['note'], rows[1]['note'])
            self.assertEqual(merged[0]['keywords'], ['audio', 'mute'])
            learn.close_entries(path, ['L-001'], '90949ab')
            closed, _ = learn.load_ledger(path)
            self.assertEqual(closed[0]['status'], 'promoted')
            self.assertEqual(closed[0]['ref'], '90949ab')
            self.assertEqual(closed[1]['id'], 'L-002')
            with self.assertRaises(learn.LearnError):
                learn.merge_entry(path, 'L-001', 'L-002')

    def test_fix_commit_trailer_closes_entries(self):
        from unittest.mock import patch
        import argparse
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            for key, value in [('user.name', 'Fixture'), ('user.email', 'fixture@example.invalid')]:
                subprocess.run(['git', '-C', str(root), 'config', key, value], check=True)
            path = root / 'ledger.jsonl'
            path.write_text(json.dumps(entry(id='L-001', date='2026-10-09')) + '\n')
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
            subprocess.run(['git', '-C', str(root), 'commit', '-qm', 'Fix fixture\n\nCloses-Learning: L-001'], check=True)
            with patch.object(learn, 'ROOT', root):
                self.assertEqual(learn.cmd_close(argparse.Namespace(commit='HEAD', entry=None, ledger=path)), 0)
            rows, _ = learn.load_ledger(path)
            self.assertEqual(rows[0]['status'], 'promoted')
            self.assertRegex(rows[0]['ref'], r'^[0-9a-f]{12}$')

    def test_malformed_ledger_cannot_be_partially_rewritten(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'ledger.jsonl'
            path.write_text(json.dumps(entry(id='L-001', date='2026-10-09')) + '\n{bad\n')
            before = path.read_bytes()
            with self.assertRaises(learn.LearnError):
                learn.close_entries(path, ['L-001'], '90949ab')
            self.assertEqual(path.read_bytes(), before)

    def test_a_good_entry_validates(self):
        self.assertEqual(learn.validate_entry(entry()), [])
        full = entry(workaround='use X', trap='silent', status='promoted', ref='7ca1536', duplicated=['~/G/src/a.rs'],
                     keywords=['lobby', 'ready'], features=['netplay'], hint='short hint')
        self.assertEqual(learn.validate_entry(full), [])

    def test_validation_rejects_bad_fields_with_clear_messages(self):
        cases = [
            (entry(area='nonsense'), 'area must be one of'),
            (entry(status='done'), 'status must be one of'),
            (entry(tokens=-1), 'tokens must be'),
            (entry(tokens='5'), 'tokens must be'),
            (entry(note=''), 'note is required'),
            (entry(note='x' * 401), 'under 400'),
            (entry(note='two\nlines'), 'one line'),
            (entry(status='promoted'), 'needs --ref'),
            (entry(game='bad/../name\n'), 'game must be'),
            (entry(hint='h' * 111), 'hint is 111'),
            (entry(duplicated=['a'] * 13), 'at most 12'),
            (entry(keywords=['Not Lowercase!']), 'keywords must be'),
            (entry(ref='bad(ref)'), 'ref may only hold'),
            (entry(extra='x'), 'unknown fields'),
        ]
        for bad, message in cases:
            errors = learn.validate_entry(bad)
            self.assertTrue(any(message in e for e in errors), (message, errors))

    def test_secret_like_text_is_rejected_without_echoing_it(self):
        for field in ('note', 'workaround', 'trap', 'hint'):
            for secret in (FAKE_UUID, FAKE_KEY, FAKE_HEX, 'password is ' + FAKE_PASSWORD, 'api_key=abcdef123456',
                           'Bearer abc.def'):
                errors = learn.validate_entry(entry(**{field: 'the server said ' + secret}))
                self.assertTrue(any('looks like a secret' in e for e in errors), (field, secret))
                self.assertFalse(any(secret in e for e in errors))
        self.assertTrue(learn.validate_entry(entry(duplicated=['~/x/' + FAKE_UUID + '.rs'])))
        self.assertEqual(learn.validate_entry(entry(status='promoted', ref='7ca1536, ADR 0035 and 1e2d2bc')), [])
        self.assertTrue(learn.validate_entry(entry(note='fixed in ' + 'a' * 40 + ' ok')), 'a bare 40-hex sha in prose is refused')

    def test_record_appends_assigns_ids_and_refuses_duplicates_and_secrets(self):
        with tempfile.TemporaryDirectory() as tmp:
            ledger = str(Path(tmp) / 'ledger.jsonl')
            code, out, err = run_cli('record', '--game', 'g', '--area', 'ai', '--tokens', '10', '--note', 'one',
                                     '--ledger', ledger)
            self.assertEqual(code, 0, err)
            self.assertIn('L-001', out)
            code, out, err = run_cli('record', '--game', 'g', '--area', 'ai', '--tokens', '10', '--note', 'two',
                                     '--status', 'promoted', '--ref', 'ADR 0036', '--keywords', 'Foo, bar',
                                     '--duplicated', '~/a.rs,~/b.rs', '--ledger', ledger, '--json')
            self.assertEqual(code, 0, err)
            stored = json.loads(out)
            self.assertEqual((stored['id'], stored['keywords'], stored['duplicated']), ('L-002', ['foo', 'bar'], ['~/a.rs', '~/b.rs']))
            self.assertEqual(list(stored)[:2], ['id', 'date'])
            code, _, err = run_cli('record', '--game', 'g', '--area', 'ai', '--tokens', '10', '--note', 'two', '--ledger', ledger)
            self.assertEqual(code, 2)
            self.assertIn('already recorded as L-002', err)
            code, _, err = run_cli('record', '--game', 'g', '--area', 'ai', '--tokens', '1', '--ledger', ledger,
                                   '--note', 'found token ' + FAKE_KEY)
            self.assertEqual(code, 2)
            self.assertIn('looks like a secret', err)
            self.assertNotIn(FAKE_KEY, err)
            self.assertEqual(len(Path(ledger).read_text().splitlines()), 2, 'a rejected entry writes nothing')
            code, out, _ = run_cli('record', '--game', 'g', '--area', 'ai', '--tokens', '1', '--note', 'dry', '--dry-run', '--ledger', ledger)
            self.assertEqual(code, 0)
            self.assertEqual(len(Path(ledger).read_text().splitlines()), 2)

    def test_help_for_every_subcommand(self):
        for command in ('sessions', 'dupes', 'eval', 'report', 'record', 'scan'):
            out = io.StringIO()
            with contextlib.redirect_stdout(out), self.assertRaises(SystemExit) as stop:
                learn.main([command, '--help'])
            self.assertEqual(stop.exception.code, 0)
            self.assertIn('usage: learn.py ' + command, out.getvalue())

    def test_committed_ledger_is_valid_unique_and_secret_free(self):
        entries, bad = learn.load_ledger(ROOT / 'docs' / 'learning' / 'ledger.jsonl')
        self.assertEqual(bad, 0)
        self.assertGreaterEqual(len(entries), 40)
        ids = [e['id'] for e in entries]
        self.assertEqual(len(ids), len(set(ids)))
        for item in entries:
            self.assertEqual(learn.validate_entry({k: v for k, v in item.items() if k not in ('id', 'date')}), [], item['id'])
        statuses = {e['status'] for e in entries}
        self.assertTrue({'open', 'promoted'} <= statuses)
        # The seed must carry the lessons the plan names, with their references.
        refs = {e['ref'] for e in entries if e.get('ref')}
        for needed in ('1e2d2bc', '7ca1536', '77f4d80', 'ADR 0036'):
            self.assertTrue(any(needed in ref for ref in refs), needed)


# ---------------------------------------------------------------------------------------------------------------
# eval
# ---------------------------------------------------------------------------------------------------------------
def packet(*ids, extra=''):
    return json.dumps({'matches': [{'id': i, 'read_first': [f'src/{i}.rs']} for i in ids], 'extra': extra},
                      separators=(',', ':'))


class EvalTests(unittest.TestCase):
    def test_scoring_recall_top1_paths_and_tokens(self):
        task = {'id': 't', 'prompt': 'p', 'expect_features': ['a', 'b'], 'expect_paths': ['src/a.rs', 'docs/X.md']}
        row = learn.score_task(task, packet('a', 'c', 'd', extra='see docs/X.md'), 3)
        self.assertEqual((row['feature_recall'], row['path_recall'], row['top1']), (0.5, 1.0, True))
        self.assertEqual(row['missed_features'], ['b'])
        self.assertEqual(row['tokens'], -(-len(packet('a', 'c', 'd', extra='see docs/X.md')) // 4))
        self.assertEqual(learn.score_task(task, packet('c', 'a'), 1)['feature_recall'], 0.0, 'only the top k count')
        self.assertFalse(learn.score_task(task, packet('c', 'a', 'b'), 3)['top1'])
        self.assertIsNone(learn.score_task({'id': 't', 'prompt': 'p', 'expect_paths': ['src/a.rs']}, packet('a'), 3)['feature_recall'])
        failed = learn.score_task(task, '', 3, error='boom')
        self.assertEqual((failed['feature_recall'], failed['error']), (0.0, 'boom'))
        self.assertEqual(learn.score_task(task, 'not json', 3)['error'], 'unparseable packet')

    def test_run_eval_aggregates_with_an_injected_runner(self):
        tasks = [{'id': 'one', 'prompt': 'x', 'expect_features': ['a'], 'expect_paths': ['src/a.rs']},
                 {'id': 'two', 'prompt': 'y', 'expect_features': ['b'], 'expect_paths': []},
                 {'id': 'three', 'prompt': 'z', 'expect_features': ['c'], 'expect_paths': []}]
        outputs = {'x': (packet('a'), None), 'y': (packet('z'), None), 'z': ('', 'The query must be 1..100 characters')}
        rows, total = learn.run_eval(tasks, 3, runner=lambda prompt, k: outputs[prompt])
        self.assertEqual((total['tasks'], total['full_hits'], total['errors']), (3, 1, 1))
        self.assertAlmostEqual(total['feature_recall'], round(1 / 3, 3))
        self.assertEqual(total['path_recall'], 1.0)
        self.assertEqual(total['top1_rate'], round(1 / 3, 3))
        text = learn.format_eval(total, rows, tasks, 3, 'abc1234')
        self.assertIn('misses:', text)
        self.assertIn('missing features: b (got z)', text)
        self.assertIn('error: The query must be', text)

    def test_cli_eval_records_rows_and_sets_a_floor(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            tasks = Path(tmp) / 'tasks.jsonl'
            tasks.write_text(json.dumps({'id': 'one', 'prompt': 'x', 'expect_features': ['a'], 'expect_paths': []}) + '\n')
            with patch.object(learn, 'subprocess_context', lambda prompt, k: (packet('a'), None)), \
                    patch.object(learn, 'RUNS', Path(tmp) / 'runs.jsonl'), patch.object(learn, 'FLOOR', Path(tmp) / 'floor.json'):
                code, out, err = run_cli('eval', '--tasks', str(tasks), '--note', 'baseline', '--set-floor')
                self.assertEqual(code, 0, err)
                run_cli('eval', '--tasks', str(tasks), '--no-record')
                rows = [json.loads(line) for line in (Path(tmp) / 'runs.jsonl').read_text().splitlines()]
                self.assertEqual([r['kind'] for r in rows], ['task', 'run'])
                self.assertEqual(rows[1]['note'], 'baseline')
                self.assertEqual((rows[1]['feature_recall'], rows[1]['k']), (1.0, 3))
                self.assertIn('commit', rows[1])
                self.assertEqual(json.loads((Path(tmp) / 'floor.json').read_text())['feature_recall'], 1.0)
                self.assertEqual(run_cli('eval', '--tasks', str(tasks), '-k', '9')[0], 2)

    def test_task_file_validation(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 't.jsonl'
            for text, message in [('{bad', 'not valid JSON'), ('{"id": "a"}', 'prompt must be'),
                                  ('{"id": "a", "prompt": "p"}', 'expect_features or expect_paths'),
                                  ('{"id": "a", "prompt": "p", "expect_paths": ["x"]}\n{"id": "a", "prompt": "q", "expect_paths": ["x"]}', 'duplicate id'),
                                  ('{"id": "a", "prompt": "p", "expect_features": "netplay"}', 'must be a list')]:
                path.write_text(text)
                with self.assertRaisesRegex(learn.LearnError, message):
                    learn.load_tasks(path)

    def test_committed_tasks_have_real_expectations(self):
        tasks = [t for name in learn.TASK_FILES for t in learn.load_tasks(ROOT / 'docs' / 'learning' / name)]
        self.assertGreaterEqual(len(tasks), 25)
        features = workflow.index(ROOT)
        for task in tasks:
            self.assertLessEqual(len(task['prompt']), 100, task['id'])
            for feature in task.get('expect_features', []):
                self.assertIn(feature, features, task['id'])
            for path in task.get('expect_paths', []):
                self.assertTrue((ROOT / path).exists(), f"{task['id']}: {path}")

    def test_discovery_does_not_regress_below_the_recorded_floor(self):
        floor_path = ROOT / 'docs' / 'learning' / 'eval_floor.json'
        if not floor_path.is_file():
            self.skipTest('no floor recorded yet')
        floor = json.loads(floor_path.read_text())

        def in_process(prompt, k):
            try:
                return json.dumps(workflow.context(ROOT, prompt, k), separators=(',', ':')) + '\n', None
            except ValueError as error:
                return '', str(error)
        history = (ROOT / 'docs/learning/eval_runs.jsonl').read_bytes()
        self.assertEqual(set(floor['sets']), set(learn.TASK_FILES))
        for name in learn.TASK_FILES:
            with self.subTest(task_set=name):
                tasks = learn.load_tasks(ROOT / 'docs/learning' / name)
                expected = floor['sets'][name]
                rows, total = learn.run_eval(tasks, expected['k'], runner=in_process)
                for metric in ('feature_recall', 'path_recall', 'full_hits'):
                    self.assertGreaterEqual(total[metric], expected[metric], f'{name}: {metric} regressed')
                self.assertEqual(total['errors'], 0)
                self.assertEqual(set(expected['cases']), {t['id'] for t in tasks})
                for row in rows:
                    case = expected['cases'][row['task']]
                    self.assertFalse(set(case['features']) & set(row['missed_features']), row['task'])
                    self.assertFalse(set(case['paths']) & set(row['missed_paths']), row['task'])
        self.assertEqual((ROOT / 'docs/learning/eval_runs.jsonl').read_bytes(), history)


# ---------------------------------------------------------------------------------------------------------------
# report
# ---------------------------------------------------------------------------------------------------------------
class ReportTests(unittest.TestCase):
    def make(self, tmp):
        learning = Path(tmp) / 'learning'
        learning.mkdir()
        (learning / 'tasks.jsonl').write_text(json.dumps({'id': 'one', 'prompt': 'find text box', 'expect_features': ['a'], 'expect_paths': ['src/a.rs']}) + '\n')
        runs = [{'kind': 'run', 'run': 'r1', 'k': 3, 'commit': 'aaa', 'tasks': 1, 'feature_recall': 0.5, 'path_recall': 0.0,
                 'top1_rate': 0.0, 'full_hits': 0, 'mean_tokens': 900.0},
                {'kind': 'run', 'run': 'r2', 'k': 3, 'commit': 'bbb', 'tasks': 1, 'feature_recall': 1.0, 'path_recall': 0.5,
                 'top1_rate': 1.0, 'full_hits': 0, 'mean_tokens': 950.0},
                {'kind': 'task', 'run': 'r2', 'k': 3, 'task': 'one', 'missed_features': [], 'missed_paths': ['src/a.rs']}]
        (learning / 'eval_runs.jsonl').write_text('\n'.join(json.dumps(r) for r in runs) + '\n')
        learn.append_entry(entry(note='open friction one', tokens=60000), learning / 'ledger.jsonl', today='2026-10-01')
        learn.append_entry(entry(note='solved thing', tokens=5, status='promoted', ref='ADR 1'), learning / 'ledger.jsonl', today='2026-10-01')
        (learning / 'dupes.json').write_text(json.dumps({'files_scanned': 10, 'engine_commit': 'ccc', 'clusters_found': 2, 'clusters': [
            {'label': 'no-equivalent: promote candidate', 'score': 648, 'copies': 3, 'lines': 216, 'projects': ['a', 'b'],
             'examples': [{'path': '~/G/physics.rs'}], 'engine_matches': []},
            {'label': 'engine-has-equivalent: adopt', 'score': 90, 'copies': 2, 'lines': 45, 'projects': ['c'],
             'examples': [{'path': '~/G/x.rs'}], 'engine_matches': [{'name': 'ClosedPath'}]}]}))
        return learning

    def test_report_sections_and_no_local_content_in_the_committed_text(self):
        with tempfile.TemporaryDirectory() as tmp:
            learning = self.make(tmp)
            local = Path(tmp) / 'local'
            local.mkdir()
            text, local_text = learn.build_report(learning, local)
            self.assertIsNone(local_text, 'no local section without local session output')
            for needle in ('| feature recall | 0.50 | 1.00 |', 'Top discovery misses', '`one` find text box: missing src/a.rs',
                           'score 648: 3 copies x 216 lines', 'ClosedPath', '2 entries: 1 open, 1 promoted',
                           'open friction one', '60,000 tokens', 'python3 tools/learn.py sessions'):
                self.assertIn(needle, text)
            (local / 'summary.json').write_text(json.dumps({
                'sessions': 3, 'turns': 10, 'fresh_work': 1234, 'side_share': 0.5, 'repeat_reads': 1, 'reads': 2,
                'repeat_reads_unedited': 1, 'empty_searches': 0, 'searches': 4, 'from_scratch_candidates': 1,
                'from_scratch_chars': 5000, 'areas': [{'area': 'SpookyKart', 'sessions': 1, 'turns': 5, 'fresh_work': 99, 'side_share': 0.1}]}))
            text2, local_text = learn.build_report(learning, local)
            self.assertEqual(text2, text, 'the committed report never changes because local data exists')
            self.assertIn('local only, never committed', local_text)
            self.assertIn('SpookyKart', local_text)
            self.assertNotIn('SpookyKart', text)

    def test_report_command_writes_the_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            learning = self.make(tmp)
            from unittest.mock import patch
            with patch.object(learn, 'LEARNING', learning), patch.object(learn, 'LOCAL', Path(tmp) / 'nolocal'):
                code, out, err = run_cli('report', '--out', str(Path(tmp) / 'R.md'))
            self.assertEqual(code, 0, err)
            self.assertTrue((Path(tmp) / 'R.md').read_text().startswith('# What the engine has learned'))

    def test_committed_report_has_no_session_derived_section(self):
        report = ROOT / 'docs' / 'learning' / 'REPORT.md'
        if report.is_file():
            text = report.read_text(encoding='utf-8')
            self.assertNotIn('local only, never committed', text)
            self.assertIsNone(learn.secret_like(text))

    def test_real_report_cli_scores_all_sets_without_appending_history(self):
        import subprocess
        before = (ROOT / 'docs/learning/eval_runs.jsonl').read_bytes()
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / 'report.md'
            result = subprocess.run([sys.executable, str(ROOT / 'tools/learn.py'), 'report', '--out', str(out)],
                                    cwd=ROOT, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = out.read_text(encoding='utf-8')
            for name in learn.TASK_FILES:
                self.assertIn('| ' + name + ' |', report)
            self.assertIn('development/regression fixtures', report)
        self.assertEqual((ROOT / 'docs/learning/eval_runs.jsonl').read_bytes(), before)


# ---------------------------------------------------------------------------------------------------------------
# the be2.py context hook (docs/learning/ledger.jsonl -> `learned`)
# ---------------------------------------------------------------------------------------------------------------
class ContextHookTests(unittest.TestCase):
    def test_stock_routing_and_hud_capabilities_and_new_lessons_are_discoverable(self):
        route = workflow.context(ROOT, 'route around obstacles and repeat a control after walking')
        packet = json.dumps(route)
        self.assertIn('simulation_scenarios', packet)
        self.assertIn('src/viewer/scenario.rs', packet)
        self.assertIn('wait_ticks:1', ' '.join(route.get('learned', [])))
        hud = json.dumps(workflow.context(ROOT, 'stock battery countdown HUD palette success wording'))
        self.assertIn('src/viewer/stock_presentation.rs', hud)
        recovery = workflow.context(ROOT, 'rollback recovery updater receipt')
        self.assertIn('restored artifacts resume', ' '.join(recovery.get('learned', [])))

    def test_server_guidance_uses_the_supported_path_without_obsolete_workarounds(self):
        packet = workflow.context(ROOT, 'limit how many players can join my netplay server')
        hints = ' '.join(packet.get('learned', []))
        self.assertIn('netplay::cli::serve', hints)
        self.assertIn('NetGame::MAX_SEATS', hints)
        self.assertNotIn('copy an existing', hints.lower())
        self.assertNotIn('in progress', hints.lower())

    def test_recorded_lesson_is_retrievable_and_missing_keywords_are_explicit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self.root_with(tmp, [])
            ledger = root / 'docs/learning/ledger.jsonl'
            code, _, err = run_cli('record', '--game', 'observatory', '--area', 'simulation', '--tokens', '0',
                                  '--note', 'Repeated controls need separate executed ticks after walking finishes.',
                                  '--trap', 'Wait one executed tick between repeated interactions.',
                                  '--keywords', 'repeated,control,interaction,tick,walking', '--ledger', str(ledger))
            self.assertEqual(code, 0, err)
            hints = workflow.context(root, 'repeat interaction with a control after walking')['learned']
            self.assertIn('executed tick', ' '.join(hints))
            code, _, err = run_cli('record', '--game', 'observatory', '--area', 'docs', '--tokens', '0',
                                  '--note', 'An archival note.', '--ledger', str(ledger))
            self.assertEqual(code, 0, err)
            self.assertIn('archive-only', err)

    def test_scripted_screenshot_case_finds_playback(self):
        packet = workflow.context(ROOT, 'a scripted run that saves screenshots at chosen frames')
        self.assertIn('custom_simulation', [m['id'] for m in packet['matches']])
        self.assertIn('src/viewer/devkit/playback.rs', json.dumps(packet))

    def test_streamed_world_and_recorded_ambience_have_actionable_entry_points(self):
        packet = workflow.context(ROOT, 'infinite procedural world streaming chunks day night cycle')
        self.assertEqual(packet['matches'][0]['id'], 'procedural_gen')
        self.assertIn('src/viewer/devkit/procedural.rs', packet['matches'][0]['read_first'])
        self.assertIn('DayCycle', packet['matches'][0]['public_api'])
        packet = workflow.context(ROOT, 'recorded birdsong ambience crossfade loop music')
        self.assertEqual(packet['matches'][0]['id'], 'audio_authoring')
        self.assertIn('docs/AUDIO.md', packet['matches'][0]['read_first'])

    def root_with(self, tmp, entries):
        root = Path(tmp)
        (root / 'tools').mkdir()
        (root / 'tools' / 'FEATURES.json').write_bytes((ROOT / 'tools' / 'FEATURES.json').read_bytes())
        (root / 'docs' / 'learning').mkdir(parents=True)
        (root / 'docs' / 'learning' / 'ledger.jsonl').write_text('\n'.join(json.dumps(e) for e in entries) + '\n')
        return root

    def test_matching_entries_become_short_solved_trap_and_open_lines(self):
        entries = [
            {'note': 'n', 'status': 'promoted', 'ref': 'ADR 0036', 'hint': 'Use kit::Shadows for shadows.', 'keywords': ['shadow', 'blob']},
            {'note': 'n', 'status': 'open', 'trap': 'Quads wound the wrong way vanish.', 'keywords': ['quad', 'winding', 'culled']},
            {'note': 'Lobby stalled.', 'status': 'open', 'keywords': ['lobby', 'ready', 'stall']},
        ]
        with tempfile.TemporaryDirectory() as tmp:
            root = self.root_with(tmp, entries)
            lines = workflow.ledger_hints(root, 'add shadows to my kit game')
            self.assertEqual(lines, ['solved: Use kit::Shadows for shadows. (ADR 0036)'])
            self.assertEqual(workflow.ledger_hints(root, 'quad winding looks culled'), ['trap: Quads wound the wrong way vanish.'])
            self.assertEqual(workflow.ledger_hints(root, 'lobby ready button stalls'), ['open: Lobby stalled.'])
            self.assertEqual(workflow.ledger_hints(root, 'something unrelated entirely'), [])
            packet_with = workflow.context(root, 'add shadows to my kit game')
            self.assertEqual(packet_with['learned'], lines)
            self.assertNotIn('learned', workflow.context(root, 'zyxquantumunknown'))

    def test_hints_are_bounded_to_five_lines_and_600_characters(self):
        entries = [{'note': 'n' * 300, 'status': 'promoted', 'ref': 'ADR %d' % i, 'workaround': 'w' * 300,
                    'keywords': ['shadow', 'blob', 'light']} for i in range(30)]
        with tempfile.TemporaryDirectory() as tmp:
            root = self.root_with(tmp, entries)
            lines = workflow.ledger_hints(root, 'shadow blob light')
            self.assertLessEqual(len(lines), 5)
            self.assertLessEqual(sum(len(line) for line in lines), 600)
            self.assertTrue(all(len(line) <= 118 for line in lines))

    def test_single_generic_word_or_feature_match_alone_never_hints(self):
        entries = [{'note': 'Spatial audio is missing.', 'status': 'open', 'keywords': ['play', 'audio', 'pan'], 'features': ['netplay']}]
        with tempfile.TemporaryDirectory() as tmp:
            root = self.root_with(tmp, entries)
            self.assertEqual(workflow.ledger_hints(root, 'add online play and a lobby', ['netplay']), [])
            self.assertEqual(workflow.ledger_hints(root, 'netplay', ['netplay']), [])

    def test_missing_or_garbage_ledger_is_silent(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(workflow.ledger_hints(tmp, 'shadows'), [])
            root = self.root_with(tmp, [])
            (root / 'docs' / 'learning' / 'ledger.jsonl').write_text('not json\n[1]\n{"note": 5}\n\x00\n')
            self.assertEqual(workflow.ledger_hints(root, 'shadows please'), [])
            self.assertNotIn('learned', workflow.context(root, 'shared_gameplay'))

    def test_real_packets_stay_within_budget_with_the_committed_ledger(self):
        for task in learn.load_tasks(ROOT / 'docs' / 'learning' / 'tasks.jsonl'):
            result = workflow.context(ROOT, task['prompt'])
            self.assertLess(len(json.dumps(result)), 8000, task['id'])
            if 'learned' in result:
                self.assertLessEqual(len(result['learned']), 5)
                self.assertLessEqual(sum(len(line) for line in result['learned']), 600)
        self.assertNotIn('learned', workflow.context(ROOT, 'zyxquantumunknown'))
        self.assertNotIn('learned', workflow.context(ROOT, 'movement', 1))


# ---------------------------------------------------------------------------------------------------------------
# modules: per-file summaries, and context routing to a file by what it does
# ---------------------------------------------------------------------------------------------------------------
class ModuleTests(unittest.TestCase):
    def test_summaries_come_from_rust_docs_python_docstrings_and_markdown_titles(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'a.rs').write_text('//! A tiny [`Thing`] with a [link](https://x.y/z) inside. More text here.\n//! Second line.\nfn f() {}\n')
            (root / 'b.rs').write_text('fn f() {}\n//! not a module doc\n')
            (root / 'c.py').write_text('#!/usr/bin/env python3\n"""Count the things:\nacross lines. Then more."""\nimport os\n')
            (root / 'd.md').write_text('\n# ADR 0036: Shadows for kit games\n\ntext\n')
            (root / 'e.rs').write_text('//! ' + 'word ' * 60 + '\n')
            self.assertEqual(learn.module_summary(root / 'a.rs'), 'A tiny Thing with a link inside.')
            self.assertIsNone(learn.module_summary(root / 'b.rs'))
            self.assertEqual(learn.module_summary(root / 'c.py'), 'Count the things: across lines.')
            self.assertEqual(learn.module_summary(root / 'd.md'), 'ADR 0036: Shadows for kit games')
            self.assertLessEqual(len(learn.module_summary(root / 'e.rs')), learn.MODULE_SUMMARY_CHARS)
            self.assertIsNone(learn.module_summary(root / 'missing.rs'))

    def test_build_modules_uses_only_indexed_source_and_guides(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel, body in {'src/viewer/a.rs': '//! Alpha module.\n', 'src/viewer/tests.rs': '//! tests\n',
                              'tools/test_x.py': '"""test"""\n', 'docs/G.md': '# Guide\n', 'other/z.rs': '//! Z.\n',
                              'tools/t.py': '"""Tool."""\n'}.items():
                (root / rel).parent.mkdir(parents=True, exist_ok=True)
                (root / rel).write_text(body)
            (root / 'tools' / 'FEATURES.json').write_text(json.dumps({'features': {'f': {
                'files': ['src/viewer/a.rs', 'src/viewer/tests.rs', 'tools/test_x.py', 'docs/G.md', 'other/z.rs', 'tools/t.py', 'src/gone.rs']}}}))
            self.assertEqual(learn.build_modules(root), {'docs/G.md': 'Guide', 'src/viewer/a.rs': 'Alpha module.', 'tools/t.py': 'Tool.'})

    def test_modules_command_checks_and_writes(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'src').mkdir()
            (root / 'src' / 'a.rs').write_text('//! Alpha.\n')
            (root / 'tools').mkdir()
            features = root / 'tools' / 'FEATURES.json'
            features.write_text(json.dumps({'format': 1, 'features': {'f': {'files': ['src/a.rs'], 'checks': [], 'note': 'n'}}}))
            with patch.object(learn, 'ROOT', root), patch.object(learn, 'FEATURES_FILE', features):
                self.assertEqual(run_cli('modules', '--check')[0], 1)
                self.assertEqual(run_cli('modules', '--write')[0], 0)
                self.assertEqual(json.loads(features.read_text())['modules'], {'src/a.rs': 'Alpha.'})
                self.assertEqual(run_cli('modules', '--check')[0], 0)

    def test_committed_modules_exist_and_are_short(self):
        modules = workflow.modules(ROOT)
        self.assertGreater(len(modules), 100)
        for path, summary in modules.items():
            self.assertTrue((ROOT / path).is_file(), path)
            self.assertTrue(summary and len(summary) <= learn.MODULE_SUMMARY_CHARS + 3, path)

    def test_context_leads_with_the_file_that_matches_the_task(self):
        feature = {'files': ['src/a/rng.rs', 'src/a/text_field.rs', 'src/a/mod.rs'], 'checks': ['c'], 'note': 'umbrella',
                   'read_first': ['src/a/mod.rs']}
        summaries = {'src/a/rng.rs': 'Tiny deterministic random numbers for simulations.',
                     'src/a/text_field.rs': 'A single-line text box: typing, pasting, caret movement.',
                     'src/a/mod.rs': 'Building blocks for games.'}
        words = workflow.learned_words
        self.assertEqual(workflow.module_picks(summaries, feature, words('random numbers that replay')), ['src/a/rng.rs'])
        self.assertEqual(workflow.module_picks(summaries, feature, words('add a text box with paste')), ['src/a/text_field.rs'])
        self.assertEqual(workflow.module_picks(summaries, feature, words('something about games')), [])
        self.assertEqual(workflow.module_picks(summaries, feature, words('deterministic')), [], 'one shared word that is not in the path is not enough')
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'tools').mkdir()
            (root / 'tools' / 'FEATURES.json').write_text(json.dumps({'format': 1, 'features': {'umbrella': {
                **feature, 'keywords': ['simulation']}}, 'modules': summaries}))
            packet = workflow.context(root, 'simulation random numbers')
            self.assertEqual(packet['matches'][0]['read_first'][0], 'src/a/rng.rs')
            plain = workflow.context(root, 'simulation')
            self.assertEqual(plain['matches'][0]['read_first'], ['src/a/mod.rs', ], 'no file match: the curated list, unchanged')

    def test_a_generic_word_does_not_outvote_a_specific_one(self):
        features = {'big': {'files': ['a'], 'checks': [], 'note': 'player server game level map camera sound ' * 3},
                    'big2': {'files': ['b'], 'checks': [], 'note': 'player server game level map camera sound'},
                    'big3': {'files': ['c'], 'checks': [], 'note': 'player server game level'},
                    'niche': {'files': ['d'], 'checks': [], 'note': 'quic certificate pinned'}}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'tools').mkdir()
            (root / 'tools' / 'FEATURES.json').write_text(json.dumps({'format': 1, 'features': features}))
            self.assertEqual(workflow.context(root, 'player quic', 1)['matches'][0]['id'], 'niche')
