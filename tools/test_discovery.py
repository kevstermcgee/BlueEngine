"""Public authoring answers, runtime limits and measured UTF-8 intake budgets."""
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

from tools import learn, springboard, workflow

ROOT = Path(__file__).resolve().parents[1]
TASKS = (
    ('Make the player jump higher', ['movement']),
    ('Build an offline 2D game with enemies and scoring', ['two_dimensional']),
    ('Build a stock declarative game', ['game_documents']),
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
    def test_mcp_launcher_uses_shared_freshness_and_validates_explicit_override(self):
        from tools import mcp_server
        expected = ROOT / 'shared target with spaces' / 'itest' / 'be2-tools'
        with patch.dict(os.environ, {'BE2_TOOLS': str(expected)}), \
                patch.object(mcp_server, 'fresh_tools', return_value=str(expected)) as fresh:
            self.assertEqual(mcp_server.find_or_build_binary(), expected)
            fresh.assert_called_once_with(ROOT, str(expected))
        with patch.object(mcp_server, 'fresh_tools', side_effect=ValueError('incompatible or stale')):
            with self.assertRaisesRegex(ValueError, 'incompatible or stale'):
                mcp_server.find_or_build_binary()

    def test_workshop_questions_find_the_existing_optional_public_route(self):
        cases = (
            ('inventories', 'resource_transactions', 'pub fn transact'),
            ('atomic inventory transactions', 'resource_transactions', 'pub fn transact'),
            ('machine production', 'automation_workshop', 'Command::Queue'),
            ('blocked outputs', 'automation_workshop', 'pub fn inspect'),
            ('full destination', 'automation_workshop', 'pub fn inspect'),
            ('save migration', 'automation_workshop', 'pub fn migrate_content'),
            ('online workshop support', 'automation_workshop', 'NetGame'),
            ('online support', 'automation_workshop', 'NetGame'),
        )
        for query, feature_id, signature_part in cases:
            with self.subTest(query=query):
                packet = workflow.context(ROOT, query, level=1)
                self.assertEqual([item['id'] for item in packet['matches']], [feature_id])
                self.assertFalse(packet['confidence'].startswith('low'))
                answer = packet['matches'][0]['answer']
                self.assertIn(signature_part, answer['signature'])
                self.assertIn('game-owned', answer['runtime'])
                self.assertTrue('automation-workshop/Cargo.toml' in answer['example'] or
                                'automation_workshop::persistence::migrate_content' in answer['example'])
                self.assertTrue(answer['verify'].startswith('cargo test --locked'))
                guide = 'docs/RESOURCES.md' if feature_id == 'resource_transactions' else 'docs/WORKSHOP.md'
                self.assertEqual(packet['matches'][0]['read_first'], [guide])
                self.assertLessEqual(compact_bytes(packet), 3600)
                full = workflow.context(ROOT, query, level=2)
                self.assertEqual(full['matches'][0]['answer'], answer)
                self.assertEqual(full['matches'][0]['limitations'], packet['matches'][0]['limitations'])
                self.assertLessEqual(compact_bytes(full), 8000)

    def test_compound_start_returns_both_presentation_and_audio_contracts(self):
        query = 'Create a distinctive HUD and original audio'
        packet = springboard.start(ROOT, query, kind='engine')
        self.assertEqual([m['id'] for m in packet['context']['matches']], ['client_kit', 'audio_authoring'])
        context = workflow.context(ROOT, query, level=1)
        self.assertEqual(packet['context']['matches'], context['matches'])
        wording = springboard.start(ROOT, 'Create distinctive presentation and authored audio', kind='engine')
        self.assertEqual({m['id'] for m in wording['context']['matches']}, {'client_kit', 'audio_authoring'})
        self.assertEqual(len(wording['context']['matches']), 2)


    def test_workshop_discovery_preserves_failure_and_runtime_boundaries(self):
        resource = workflow.context(ROOT, 'Inventory::transact', level=1)['matches'][0]
        self.assertEqual(resource['id'], 'resource_transactions')
        self.assertIn('resources feature', ' '.join(resource['answer']['warnings']))
        self.assertIn('64 resource', json.dumps(resource))
        self.assertIn('128 combined', json.dumps(resource))
        blocked = workflow.context(ROOT, 'blocked outputs', level=1)['matches'][0]['answer']
        self.assertIn('RESOURCE-CAPACITY', ' '.join(blocked['warnings']))
        self.assertIn('pending', ' '.join(blocked['warnings']))
        migration = workflow.context(ROOT, 'save migration', level=1)['matches'][0]['answer']
        self.assertIn('not globally weakened', ' '.join(migration['warnings']))
        self.assertIn('original files', ' '.join(migration['warnings']))
        online = workflow.context(ROOT, 'online workshop support', level=1)['matches'][0]['answer']
        for required in ('1100', 'stock', 'local', 'not remote delivery', 'QUIC/TLS', 'development UDP'):
            self.assertIn(required, json.dumps(online))
        # Native CLI search and MCP search consume the same compile-time feature index.
        native = (ROOT / 'src/viewer/capabilities.rs').read_text()
        mcp = (ROOT / 'src/viewer/mcp.rs').read_text()
        self.assertIn('../../tools/FEATURES.json', native)
        self.assertIn('capabilities::search(q)', mcp)

    def test_workshop_author_intake_counts_complete_instructions_and_warnings(self):
        # UTF-8 reading bytes, not model tokens; authored game source/content are separate inputs.
        required = ['AGENTS.md', 'games/automation-workshop/AGENTS.md', 'docs/WORKSHOP.md']
        self.assertLessEqual((ROOT / required[1]).stat().st_size, 3000)
        self.assertLessEqual((ROOT / required[2]).stat().st_size, 8000)
        for query in ('machine production', 'blocked outputs', 'save migration', 'online workshop support'):
            packet = workflow.context(ROOT, query, level=1)
            reading_bytes = compact_bytes(packet) + sum((ROOT / path).stat().st_size for path in required)
            self.assertLessEqual(reading_bytes, 30000, query)

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
                               ('devkit::Snapshot', 'save_state'), ('hud::text_outlined', 'client_kit'),
                               ('GameDocument', 'game_documents'),
                               ('viewer::game::GameDocument', 'game_documents'),
                               ('GameDocument::load', 'game_documents'),
                               ('GameDocument::validate', 'game_documents'),
                               ('AudioBank::load', 'audio_authoring'),
                               ('kit::AudioBank::load', 'audio_authoring'),
                               ('Lifecycle::quick_save', 'save_state'),
                               ('devkit::Lifecycle::quick_save', 'save_state'),
                               ('Lifecycle::quick_load', 'save_state'),
                               ('devkit::Lifecycle::quick_load', 'save_state')]:
            packet = workflow.context(ROOT, query, level=1)
            self.assertEqual([m['id'] for m in packet['matches']], [feature])
            self.assertTrue(packet['confidence'].startswith('high'))
            self.assertIn('signature', packet['matches'][0]['answer'])

    def test_stock_entry_and_exact_save_methods_expose_their_runtime_contracts(self):
        stock = workflow.context(ROOT, 'Build a stock declarative game', level=1)['matches'][0]
        self.assertEqual(stock['read_first'], ['docs/GAME_AUTHORING_ENTRY.md'])
        self.assertIn('stock', stock['answer']['example'])
        self.assertEqual(stock['answer']['runtime'], 'Stock GameDocument / authoritative fixed-step server')
        self.assertIn('no scripting', ' '.join(stock['answer']['warnings']))
        self.assertIn('visual/collision bounds must agree', ' '.join(stock['answer']['warnings']))
        for method, signature in (
                ('quick_save', 'pub fn quick_save<S: Snapshot>(&self, sim: &S, label: &str) -> Notice'),
                ('quick_load', 'pub fn quick_load<S: Snapshot>(&mut self, sim: &mut S) -> Notice')):
            packet = workflow.context(ROOT, f'Lifecycle::{method}', level=1)
            answer = packet['matches'][0]['answer']
            self.assertEqual(answer['source'], 'src/viewer/devkit/lifecycle.rs')
            self.assertEqual(answer['signature'], signature)
            self.assertIn(f'lifecycle.{method}', answer['example'])
            self.assertEqual(answer['runtime'], 'Rendering-free custom simulation with devkit::Lifecycle')
            self.assertIn('Notice.ok', ' '.join(answer['warnings']))
            self.assertIn('Snapshot::KIND', ' '.join(answer['warnings']))
            self.assertLessEqual(compact_bytes(packet), 3600)

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

    def test_repaired_scaffold_hints_preserve_history_and_stop_recommending_copies(self):
        entries = {e['id']: e for e in (json.loads(line) for line in
                   (ROOT / 'docs/learning/ledger.jsonl').read_text().splitlines())}
        for entry_id in ('L-131', 'L-132'):
            entry = entries[entry_id]
            self.assertFalse(learn.validate_entry(entry))
            self.assertEqual(entry['status'], 'promoted')
            self.assertEqual(entry['ref'], '03c3ba8')
        # The original friction remains evidence; only current guidance changes.
        self.assertIn('omitted game.project.json', entries['L-131']['note'])
        self.assertIn('could not discover', entries['L-132']['note'])
        metadata = workflow.context(ROOT, 'custom-sim scaffold project metadata', level=1).get('learned', [])
        tools = workflow.context(ROOT, 'custom-sim checker itest tool discovery', level=1).get('learned', [])
        self.assertIn('solved: Custom-sim now emits metadata/shared tooling', ' '.join(metadata))
        self.assertNotIn('game_project.py copied', ' '.join(metadata))
        self.assertIn('solved: Pinned checks find itest/shared-target tools', ' '.join(tools))
        self.assertIn('explicit --tools remains validated', ' '.join(tools))

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
        game = (ROOT / 'src/viewer/game.rs').read_text()
        self.assertIn('pub fn load(path: &Path) -> Result<LoadedGame>', game)
        self.assertIn('pub fn validate(&self, map: &MapDocument) -> Result<()>', game)
        lifecycle = (ROOT / 'src/viewer/devkit/lifecycle.rs').read_text()
        for method in ('quick_save', 'quick_load'):
            answer = workflow.context(ROOT, f'Lifecycle::{method}', level=1)['matches'][0]['answer']
            self.assertIn(answer['signature'], lifecycle)
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
