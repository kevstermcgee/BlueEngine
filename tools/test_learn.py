"""Tests for tools/learn.py. Synthetic fixtures only: no network, no real session logs, no real repos except one
smoke test that skips cleanly when the game checkouts are absent. Every fake secret below is invented."""
import contextlib
import io
import json
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
        self.assertEqual(summary['tokens']['output'], parent_output + 200)
        self.assertEqual(summary['tokens']['input'], 100 + 10 * 10 + 100)
        self.assertEqual(summary['turns'], 11 + 2)
        self.assertEqual(summary['side_turns'], 2)
        self.assertAlmostEqual(summary['side_share'], summary['side_work_tokens'] / summary['work_tokens'], places=3)
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
                            ('password is hunter2', 'credential-word'), ('Authorization: Bearer abcdefg', 'credential-assignment'),
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
            self.assertEqual(summary['tokens']['output'], 2)
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
