import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('rewrite', Path(__file__).with_name('rewrite.py'))
w = importlib.util.module_from_spec(spec)
spec.loader.exec_module(w)


def issue(number=2, status='backlog', closed=False, blocked=False, body=''):
    return {'number': number, 'state': 'closed' if closed else 'open', 'body': body,
            'labels': [{'name': 'flow:' + status}] + ([{'name': 'flow:blocked'}] if blocked else [])}


class Rules(unittest.TestCase):
    def test_transitions(self):
        for old, new in zip(w.STATES, w.STATES[1:]):
            w.transition(old, new, 'concrete reason', 'test report')
        w.transition('acceptance', 'developing', 'defect at boundary', None)
        w.transition('done', 'developing', 'regression', None)
        for old in ('developing', 'acceptance'):
            w.transition(old, 'backlog', 'remaining launcher dependency; retain accepted report', 'report.json')
            with self.assertRaises(ValueError):
                w.transition(old, 'backlog', 'remaining dependency', None)
        for old, new in [('backlog', 'done'), ('ready', 'acceptance'), ('done', 'ready')]:
            with self.assertRaises(ValueError):
                w.transition(old, new, 'reason', 'evidence')
        for new in ('acceptance', 'done'):
            with self.assertRaises(ValueError):
                w.transition(w.STATES[w.STATES.index(new) - 1], new, 'reason', '  ')
        with self.assertRaises(ValueError):
            w.transition('backlog', 'ready', ' ', None)

    def test_remote(self):
        w.validate_remote([issue(1, 'developing'), issue(2, 'done', True)], 1)
        with self.assertRaises(ValueError):
            w.validate_remote([issue(2, 'developing'), issue(3, 'acceptance')], 1)
        for invalid in [issue(status='done'), issue(closed=True), issue(blocked=True),
                        issue(blocked=True, body='Flow-Blocked:   '),
                        issue(blocked=True, body='Flow-Blocked:\nUnrelated prose')]:
            with self.assertRaises(ValueError):
                w.state(invalid)
        w.state(issue(blocked=True, body='Flow-Blocked: Waiting for dependency #3'))
        duplicate = issue()
        duplicate['labels'].append({'name': 'flow:ready'})
        with self.assertRaises(ValueError):
            w.state(duplicate)

    def test_paths(self):
        for invalid in ['/', '.', './', '../x', 'a/../b', '/tmp/a', 'a/*', 'a//b', 'a\\b']:
            with self.assertRaises(ValueError):
                w.valid_path(invalid)
        w.check_paths(['crates/core/a.rs', 'Cargo.toml'], ['crates/core/', 'Cargo.toml'])
        with self.assertRaises(ValueError):
            w.check_paths(['crates/core2/a.rs'], ['crates/core/'])

    def test_message_and_packet(self):
        w.check_message('Implement\n\nWork-Issue: #12\n', 12)
        for text in ['Work-Issue: #13', 'Work-Issue: #12\nWork-Issue: #12', 'mentions #12']:
            with self.assertRaises(ValueError):
                w.check_message(text, 12)
        packet = dict(issue=12, owner='worker', paths=['crates/'], acceptance='result', checks=['python test.py'])
        w.validate_packet(packet, {12})
        for key, value in [('issue', 13), ('owner', ''), ('paths', []), ('checks', []), ('acceptance', '')]:
            with self.assertRaises(ValueError):
                w.validate_packet({**packet, key: value}, {12})

    def test_sgd_hooks_derive_ownership_and_issue_from_the_current_dispatch(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            dispatch = base / 'dispatch.json'
            task = dict(id='invoice', write_paths=['crates/client/'],
                        deliverable='Preserve typed refusal', checks=['client'],
                        commit_trailers=['Work-Issue: #12'])
            data = dict(task=task, worktree=str(base), work=dict(acceptance=['flow']))
            dispatch.write_text(json.dumps(data))
            env = dict(SGD_WORK_PACKAGE=str(dispatch), SGD_TASK_ID='invoice',
                       SGD_PHASE='worker', SGD_WORKTREE=str(base))
            with patch.dict(os.environ, env, clear=True):
                packet = w.active_packet(base, {12})
                w.check_paths(['crates/client/error.rs'], packet['paths'])
                w.check_message('sgd: invoice\n\nWork-Issue: #12', packet['issue'])
                with self.assertRaises(ValueError):
                    w.check_paths(['crates/ledger/error.rs'], packet['paths'])
                for key, value in [('SGD_TASK_ID', 'other'), ('SGD_PHASE', 'other'),
                                   ('SGD_WORK_PACKAGE', 'relative.json')]:
                    with patch.dict(os.environ, {key: value}):
                        with self.assertRaises(ValueError):
                            w.active_packet(base, {12})
                with patch.dict(os.environ, {'SGD_PHASE': 'integration',
                                            'SGD_INTEGRATION_BASE': 'synthetic-base'}):
                    self.assertEqual(w.active_packet(base, {12})['issue'], 12)
                with patch.dict(os.environ, {'SGD_PHASE': 'integration'}):
                    with self.assertRaises(ValueError):
                        w.active_packet(base, {12})
                task['commit_trailers'] = ['Work-Issue: #13']
                dispatch.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    w.active_packet(base, {12})


class MoveTests(unittest.TestCase):
    def test_parking_retains_open_issue_and_other_labels(self):
        target = issue(2, 'acceptance')
        target['labels'].append({'name': 'domain:money'})
        writes = self.invoke({1: issue(1), 2: target, 3: issue(3)},
                             ['move', '2', 'backlog', '--reason', 'pending launcher dependency',
                              '--evidence', 'accepted-report.json'])
        self.assertIn('accepted-report.json', writes[0][2]['body'])
        self.assertEqual(writes[1][2], {'labels': ['domain:money', 'flow:backlog'], 'state': 'open'})

    def invoke(self, records, args):
        writes = []
        def fake_gh(repo, endpoint, method='GET', payload=None):
            if method == 'GET':
                return records[int(endpoint.split('/')[-1])]
            writes.append((endpoint, method, payload))
            return {}
        with patch.object(w, 'root', return_value=Path('/unused')), patch.object(
                w, 'mapping', return_value=({1, 2, 3}, 1, 'owner/repo')), patch.object(
                w, 'gh', side_effect=fake_gh):
            w.main(args)
        return writes

    def test_explicit_move_preserves_labels_and_records_evidence(self):
        target = issue(2, 'integrating')
        target['labels'].append({'name': 'domain:money'})
        writes = self.invoke({1: issue(1), 2: target, 3: issue(3)},
                             ['move', '2', 'acceptance', '--reason', 'checks passed', '--evidence', 'report.json'])
        self.assertEqual([item[1] for item in writes], ['POST', 'PATCH'])
        self.assertIn('report.json', writes[0][2]['body'])
        self.assertEqual(writes[1][2], {'labels': ['domain:money', 'flow:acceptance'], 'state': 'open'})

    def test_reject_unauthorized_issue_before_remote(self):
        with self.assertRaises(ValueError):
            self.invoke({}, ['move', '99', 'ready', '--reason', 'reason'])

    def test_no_writes_when_wip_or_evidence_invalid(self):
        records = {1: issue(1), 2: issue(2, 'ready'), 3: issue(3, 'developing')}
        with self.assertRaises(ValueError):
            self.invoke(records, ['move', '2', 'developing', '--reason', 'reason'])
        records[2] = issue(2, 'integrating')
        records[3] = issue(3)
        with self.assertRaises(ValueError):
            self.invoke(records, ['move', '2', 'acceptance', '--reason', 'reason'])


class GitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.base = Path(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        (self.base / 'old.txt').write_text('hello\n')
        self.git('add', '.')
        self.git('-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'base')

    def tearDown(self):
        self.temp.cleanup()

    def git(self, *args):
        return w.run('git', *args, cwd=self.base)

    def test_pre_policy_commits_grandfathered(self):
        baseline = self.git('rev-parse', 'HEAD')
        (self.base / 'old.txt').write_text('historical change')
        self.git('add', '.')
        self.git('-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'pre-policy without trailer')
        with patch.object(w, 'mapping', return_value=({2}, 1, 'owner/repo')):
            w.check_commits(self.base, baseline + '..HEAD')
            script = self.base / 'scripts/workflow/rewrite.py'
            script.parent.mkdir(parents=True)
            script.write_text('# policy introduction')
            self.git('add', '.')
            self.git('-c', 'core.hooksPath=/dev/null', 'commit', '-qm', 'policy without trailer')
            with self.assertRaises(ValueError):
                w.check_commits(self.base, baseline + '..HEAD')
            self.git('-c', 'core.hooksPath=/dev/null', 'commit', '--amend', '-qm', 'policy\n\nWork-Issue: #2')
            w.check_commits(self.base, baseline + '..HEAD')

    def test_rename_both_sides(self):
        self.git('mv', 'old.txt', 'new.txt')
        self.assertEqual(set(w.staged_paths(self.base)), {'old.txt', 'new.txt'})
        with self.assertRaises(ValueError):
            w.check_paths(w.staged_paths(self.base), ['new.txt'])

    def test_worktree_packet_isolation(self):
        other = self.base / 'other'
        self.git('worktree', 'add', '-qb', 'other', str(other))
        self.assertNotEqual(w.packet_path(self.base).resolve(), w.packet_path(other).resolve())
        w.packet_path(other).write_text('other')
        self.assertFalse(w.packet_path(self.base).exists())

    def test_hooks_skip_pre_policy_tree_but_require_packet_when_enabled(self):
        w.install_hooks(self.base)
        hook = w.hooks_dir(self.base) / 'pre-commit'
        self.assertEqual(subprocess.run([str(hook)], cwd=self.base).returncode, 0)
        script = self.base / 'scripts/workflow/rewrite.py'
        script.parent.mkdir(parents=True)
        script.write_text(Path(w.__file__).read_text())
        mapping = self.base / w.MAP
        mapping.parent.mkdir(parents=True)
        mapping.write_text(json.dumps({'RM-RUST': {'number': 1, 'url': 'https://github.com/owner/repo/issues/1'}}))
        result = subprocess.run([str(hook)], cwd=self.base, capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('rentemester-work.json', result.stderr)

    def test_existing_hooks_preserved_and_no_partial_install(self):
        directory = self.base / 'custom-hooks'
        directory.mkdir()
        self.git('config', 'core.hooksPath', 'custom-hooks')
        (directory / 'commit-msg').write_text('original')
        (directory / 'post-commit').write_text('graphify')
        with self.assertRaises(ValueError):
            w.install_hooks(self.base)
        self.assertFalse((directory / 'pre-commit').exists())
        self.assertEqual((directory / 'commit-msg').read_text(), 'original')
        (directory / 'commit-msg').unlink()
        w.install_hooks(self.base)
        self.assertEqual((directory / 'post-commit').read_text(), 'graphify')
        self.assertTrue((directory / 'pre-commit').stat().st_mode & 0o111)
        w.install_hooks(self.base)


if __name__ == '__main__':
    unittest.main()
