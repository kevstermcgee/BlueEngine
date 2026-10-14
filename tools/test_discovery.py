"""Public authoring answers, runtime limits and measured UTF-8 intake budgets."""
import json
from pathlib import Path
import unittest

from tools import learn, springboard, workflow

ROOT = Path(__file__).resolve().parents[1]
TASKS = (
    ('Make the player jump higher', ['movement']),
    ('Build an offline 2D game with enemies and scoring', ['two_dimensional']),
    ('Build spatial 3D enemies with custom rules', ['custom_simulation']),
    ('Save and restore a custom simulation', ['save_state']),
    ('Create a distinctive HUD and original audio', ['client_kit', 'audio_authoring']),
    ('Package a Windows installer', ['game_shipping']),
    ('Add multiplayer to a custom simulation', ['netplay']),
    ('Understand the limits of replicating a larger custom world', ['netplay']),
    ('Determine whether browser gameplay is supported', ['browser_games']),
)


def compact_bytes(packet):
    return len(json.dumps(packet, separators=(',', ':'), ensure_ascii=False).encode('utf-8'))


class PublicDiscoveryTests(unittest.TestCase):
    def test_real_authoring_tasks_find_supported_public_contracts(self):
        for query, expected in TASKS:
            with self.subTest(query=query):
                packet = workflow.context(ROOT, query, level=1)
                self.assertEqual([m['id'] for m in packet['matches']], expected)
                self.assertFalse(packet['confidence'].startswith('low'))
                if expected == ['browser_games']:
                    self.assertEqual(packet['matches'][0]['status'], 'retired')
                    self.assertIn('retired', ' '.join(packet['matches'][0]['limitations']))
                    self.assertNotIn('public_api', packet['matches'][0])
                else:
                    for match in packet['matches']:
                        answer = match['answer']
                        self.assertTrue(answer['signature'])
                        self.assertTrue(answer['example'])
                        self.assertTrue(answer['verify'])
                        self.assertTrue(answer['runtime'])
                        self.assertTrue((ROOT / answer['source']).is_file())

    def test_exact_identifiers_resolve_fields_and_qualified_public_apis(self):
        for query, feature in [('jump_height', 'movement'), ('ControllerProfile::jump_height', 'movement'),
                               ('two_d::GameLogic', 'two_dimensional'), ('NetGame', 'netplay'),
                               ('devkit::Snapshot', 'save_state'), ('hud::text_outlined', 'client_kit')]:
            packet = workflow.context(ROOT, query, level=1)
            self.assertEqual([m['id'] for m in packet['matches']], [feature])
            self.assertTrue(packet['confidence'].startswith('high'))
            self.assertIn('signature', packet['matches'][0]['answer'])

    def test_compact_packets_keep_complete_required_api_and_network_warnings(self):
        for query, _ in TASKS:
            small = workflow.context(ROOT, query, level=1)
            full = workflow.context(ROOT, query, level=2)
            self.assertLessEqual(compact_bytes(small), 3600, query)
            self.assertLessEqual(compact_bytes(full), 8000, query)
            for match in small['matches']:
                expanded = next(m for m in full['matches'] if m['id'] == match['id'])
                self.assertEqual(match.get('answer'), expanded.get('answer'))
                self.assertEqual(match.get('limitations'), expanded.get('limitations'))
        net = workflow.context(ROOT, 'Add multiplayer to a custom simulation', level=1)['matches'][0]
        text = json.dumps(net)
        for term in ('partial-world replication', '1100', 'does not fragment', '1..=16',
                     'not demonstrated', 'not remote delivery', 'opt-in', 'QUIC/TLS', 'development UDP'):
            self.assertIn(term, text)
        self.assertNotIn('...', net['answer']['signature'])
        shipping = workflow.context(ROOT, 'Package a Windows installer', level=1)['matches'][0]['answer']
        self.assertIn('does not build an installer', ' '.join(shipping['warnings']))
        self.assertIn('installer build --iscc', shipping['example'])
        self.assertIn('installer test --stage', shipping['example'])
        self.assertEqual(shipping['source'], 'templates/game_installer.py')

    def test_native_review_finds_the_shared_command_and_preserves_evidence_limits(self):
        for query in ('scripts/ship.py review', 'Review physical controls and audio devices'):
            packet = workflow.context(ROOT, query, level=1)
            answer = next(match['answer'] for match in packet['matches'] if match['id'] == 'game_shipping')
            self.assertEqual(answer['source'], 'templates/game_review.py')
            self.assertIn('review record --observations', answer['signature'])
            self.assertIn('not independent measurement', ' '.join(answer['warnings']))
            self.assertIn('cannot certify physical devices', ' '.join(answer['warnings']))

    def test_start_and_context_retrieve_the_same_natural_language_learning_hints(self):
        for query, _ in TASKS:
            with self.subTest(query=query):
                packet = springboard.start(ROOT, query, kind='engine', persist=False)
                expected = workflow.context(ROOT, query, level=1).get('learned', [])
                self.assertEqual(packet['context']['traps'], expected)
                self.assertEqual(workflow.context(ROOT, query, limit=1).get('learned', []), expected)

    def test_initial_reading_path_is_bounded_including_game_instructions(self):
        # This measures emitted/read UTF-8 bytes, not model tokens or billed cost.
        for query, template in [('Build an offline 2D game with enemies and scoring', 'two-d'),
                                ('Build spatial 3D enemies with custom rules', 'custom-sim'),
                                ('Build a stock declarative game', 'stock'),
                                ('Build a custom simulation game with multiplayer', 'custom-sim')]:
            packet = springboard.start(ROOT, query, kind='new-game', template=template, persist=False)
            self.assertLessEqual(compact_bytes(packet), 8500, query)
            references = packet['context']['references']
            self.assertEqual(references, ['docs/GAME_AUTHORING_ENTRY.md'])
            engine_intake = compact_bytes(packet) + sum((ROOT / p).stat().st_size for p in ['AGENTS.md', *references])
            self.assertLessEqual(engine_intake, 22000, query)
            game_agents = ROOT / 'templates' / template / 'AGENTS.md'
            if game_agents.is_file():
                self.assertLessEqual(engine_intake + game_agents.stat().st_size, 30000, query)
        custom = (ROOT / 'templates/custom-sim/AGENTS.md').read_text()
        self.assertNotIn('and read docs/CUSTOM_CLIENT.md', custom)
        self.assertNotIn('and docs/SHARED_GAMEPLAY.md', custom)

    def test_retired_learning_recipes_remain_archived_and_do_not_outrank_current_paths(self):
        entries = [json.loads(line) for line in (ROOT / 'docs/learning/ledger.jsonl').read_text().splitlines()]
        retired = [e for e in entries if e.get('scope') == 'retired-workflow']
        self.assertGreaterEqual(len(retired), 4)
        for entry in retired:
            self.assertFalse(learn.validate_entry(entry))
        hints = workflow.context(ROOT, 'browser gameplay support audio', level=1).get('learned', [])
        self.assertNotIn('Use web build', ' '.join(hints))
        self.assertNotIn('portable browser client', ' '.join(hints))
        self.assertIn('retired', ' '.join(hints))
        self.assertTrue(learn.validate_entry({**retired[0], 'scope': 'anything'}))

    def test_low_confidence_remains_honest_and_suggests_refinement(self):
        packet = workflow.context(ROOT, 'zyxquantumunknown', level=1)
        self.assertEqual(packet['matches'], [])
        self.assertTrue(packet['confidence'].startswith('low'))
        self.assertIn('runtime', packet['next'])
        self.assertIn('exact public identifier', packet['next'])

    def test_advertised_signatures_and_limits_match_current_public_sources(self):
        profile = (ROOT / 'src/viewer/profile.rs').read_text()
        self.assertIn('pub jump_height: f32', profile)
        self.assertIn('(0.0..=3.).contains(&self.jump_height)', profile)
        self.assertIn('jump_height: 0.35', profile)
        snapshot = (ROOT / 'src/viewer/devkit/snapshot.rs').read_text()
        self.assertIn('fn restore(&mut self, state: Self::State) -> Result<(), String>', snapshot)
        audio = (ROOT / 'src/viewer/kit/audio.rs').read_text()
        for part in ('pub async fn load(', 'muted: bool', 'sfx_volume: f32', 'music_volume: f32'):
            self.assertIn(part, audio)
        net = (ROOT / 'src/viewer/netplay/mod.rs').read_text()
        self.assertIn('fn snapshot_with_budget(', net)
        self.assertIn('participant: Option<usize>', net)
        self.assertIn('max_bytes: usize', net)
        server = (ROOT / 'src/viewer/netplay/server.rs').read_text()
        self.assertIn('G::MAX_SEATS >= 1 && G::MAX_SEATS <= 16', server)


if __name__ == '__main__':
    unittest.main()
