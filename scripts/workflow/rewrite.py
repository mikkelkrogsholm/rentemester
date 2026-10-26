#!/usr/bin/env python3
"""Explicit Rust epic workflow controls. Issue text is data, never executable input."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
from urllib.parse import urlparse

STATES = ('backlog', 'ready', 'developing', 'integrating', 'acceptance', 'done')
ACTIVE = {'developing', 'integrating', 'acceptance'}
MAP = 'docs/architecture/rust-target/epic/github-map.json'


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def root():
    return Path(run('git', 'rev-parse', '--show-toplevel'))


def mapping(base):
    data = json.loads((base / MAP).read_text())
    numbers = {v['number'] for v in data.values()}
    repos = {urlparse(v['url']).path.split('/issues/')[0].strip('/') for v in data.values()}
    if len(repos) != 1 or not numbers or any(type(n) is not int for n in numbers):
        raise ValueError('invalid managed issue mapping')
    epic = data['RM-RUST']['number']
    return numbers, epic, repos.pop()


def gh(repo, endpoint, method='GET', payload=None):
    args = ['gh', 'api', '--method', method, f'repos/{repo}/{endpoint}']
    if payload is None:
        return json.loads(run(*args))
    result = subprocess.run(args + ['--input', '-'], input=json.dumps(payload), text=True,
                            check=True, capture_output=True)
    return json.loads(result.stdout)


def state(issue):
    labels = [v['name'] if isinstance(v, dict) else v for v in issue['labels']]
    statuses = [s for s in STATES if f'flow:{s}' in labels]
    if len(statuses) != 1:
        raise ValueError(f"#{issue['number']} requires exactly one flow status")
    current = statuses[0]
    if (current == 'done') != (issue['state'].lower() == 'closed'):
        raise ValueError(f"#{issue['number']} done/closed mismatch")
    if 'flow:blocked' in labels and not re.search(r'^Flow-Blocked:[ \t]*[^\s][^\n]*$', issue.get('body') or '', re.M):
        raise ValueError(f"#{issue['number']} blocked requires Flow-Blocked: concrete reason in body")
    return current


def validate_remote(issues, epic):
    states = {i['number']: state(i) for i in issues}
    active = [n for n, s in states.items() if n != epic and s in ACTIVE]
    if len(active) > 1:
        raise ValueError(f'WIP limit is one child; active: {active}')
    return states


def transition(old, new, reason, evidence):
    if not reason.strip():
        raise ValueError('a concrete reason is required')
    allowed = list(zip(STATES, STATES[1:])) + [('acceptance', 'developing'), ('done', 'developing')]
    if (old, new) not in allowed:
        raise ValueError(f'unsupported transition {old} -> {new}')
    if new in {'acceptance', 'done'} and not (evidence or '').strip():
        raise ValueError('acceptance/done requires a nonempty evidence reference')


def valid_path(value):
    p = PurePosixPath(value)
    if (not value or value.startswith('/') or '\\' in value or any(c in value for c in '*?[]\n\r\0')
            or any(part in {'.', '..'} for part in value.split('/'))
            or str(p) in {'.', ''} or '//' in value):
        raise ValueError(f'invalid ownership path: {value!r}; use exact files or directory/ prefixes')
    return value


def packet_path(base):
    # --git-path resolves to the linked worktree's own git dir, not common-dir.
    path = Path(run('git', 'rev-parse', '--git-path', 'rentemester-work.json', cwd=base))
    return path if path.is_absolute() else base / path


def validate_packet(packet, numbers):
    if type(packet.get('issue')) is not int or packet['issue'] not in numbers:
        raise ValueError('packet issue must be a mapped issue number')
    for name in ('owner', 'acceptance'):
        if not isinstance(packet.get(name), str) or not packet[name].strip():
            raise ValueError(f'packet {name} is required')
    for name in ('paths', 'checks'):
        values = packet.get(name)
        if not isinstance(values, list) or not values or any(not isinstance(v, str) or not v.strip() for v in values):
            raise ValueError(f'packet {name} must contain nonempty strings')
    for path in packet['paths']:
        valid_path(path)
    return packet


def staged_paths(base):
    # No rename detection means both deletion and addition are checked, even for renames.
    output = subprocess.check_output(['git', 'diff', '--cached', '--name-only', '--no-renames', '-z'], cwd=base)
    return [os.fsdecode(p) for p in output.split(b'\0') if p]


def managed_paths(paths):
    return any(
        p.startswith(('crates/', 'rust/', 'scripts/rust/', 'docs/architecture/rust-target/', 'scripts/workflow/'))
        or p in {'Cargo.toml', 'Cargo.lock', 'AGENTS.md', '.github/workflows/rewrite-workflow.yml'}
        for p in paths
    )


def check_paths(paths, owned):
    for path in paths:
        valid_path(path)
        if not any(path.startswith(p) if p.endswith('/') else path == p for p in owned):
            raise ValueError(f'staged path outside packet ownership: {path}')


def check_message(message, issue):
    refs = re.findall(r'^Work-Issue: #([0-9]+)\s*$', message, re.M)
    if refs != [str(issue)]:
        raise ValueError(f'commit requires exactly one Work-Issue: #{issue} line matching packet')


def hooks_dir(base):
    configured = subprocess.run(['git', 'config', '--path', '--get', 'core.hooksPath'], cwd=base,
                                text=True, capture_output=True)
    if configured.returncode == 0:
        directory = Path(configured.stdout.strip())
        return directory if directory.is_absolute() else base / directory
    directory = Path(run('git', 'rev-parse', '--git-path', 'hooks', cwd=base))
    return directory if directory.is_absolute() else base / directory


def install_hooks(base):
    directory = hooks_dir(base)
    directory.mkdir(parents=True, exist_ok=True)
    contents = {
        'pre-commit': 'check-staged',
        'commit-msg': 'check-message "$1"',
    }
    marker = '# rentemester rewrite workflow hook v1'
    scripts = {name: '#!/bin/sh\n' + marker + '\nset -eu\n'
               'cd "$(git rev-parse --show-toplevel)"\n'
               '[ -f scripts/workflow/rewrite.py ] || exit 0\n'
               'exec python3 scripts/workflow/rewrite.py ' + command + '\n'
               for name, command in contents.items()}
    for name, content in scripts.items():
        path = directory / name
        if path.exists() or path.is_symlink():
            if path.is_symlink() or path.read_text() != content:
                raise ValueError(f'existing hook preserved: {path}; chain it manually before installing')
    for name, content in scripts.items():
        path = directory / name
        path.write_text(content)
        path.chmod(0o755)
    print(f'Installed hooks in {directory}; other hooks preserved')


def check_commits(base, revision):
    numbers, _, _ = mapping(base)
    commits = run('git', 'rev-list', revision, cwd=base).splitlines()
    checked = 0
    for commit in commits:
        present = subprocess.run(['git', 'cat-file', '-e', commit + ':scripts/workflow/rewrite.py'],
                                 cwd=base, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if present.returncode != 0:
            continue  # Existing migration history predates this policy.
        checked += 1
        message = run('git', 'show', '-s', '--format=%B', commit, cwd=base)
        refs = re.findall(r'^Work-Issue: #([0-9]+)\s*$', message, re.M)
        if len(refs) != 1 or int(refs[0]) not in numbers:
            raise ValueError(f'{commit} requires one mapped Work-Issue: #N trailer')
    print(f'Validated {checked} commit issue references; {len(commits) - checked} pre-policy commits exempt')


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    for cmd in ('status', 'check-remote'):
        sub.add_parser(cmd).add_argument('--include-epic', action='store_true')
    move = sub.add_parser('move')
    move.add_argument('issue', type=int)
    move.add_argument('state', choices=STATES)
    move.add_argument('--reason', required=True)
    move.add_argument('--evidence')
    packet = sub.add_parser('packet')
    packet.add_argument('--issue', type=int, required=True)
    packet.add_argument('--owner', required=True)
    packet.add_argument('--path', action='append', required=True, dest='paths')
    packet.add_argument('--acceptance', required=True)
    packet.add_argument('--check', action='append', required=True, dest='checks')
    for cmd in ('check-staged', 'install-hooks'):
        sub.add_parser(cmd)
    sub.add_parser('check-message').add_argument('file')
    sub.add_parser('check-commits').add_argument('revision', help='git revision range, e.g. BASE..HEAD')
    sub.add_parser('check-pr').add_argument('event_file')
    args = parser.parse_args(argv)
    base = root()
    numbers, epic, repo = mapping(base)
    if args.command == 'install-hooks':
        install_hooks(base)
    elif args.command == 'check-pr':
        event = json.loads(Path(args.event_file).read_text())
        pr = event['pull_request']
        revision = pr['base']['sha'] + '..' + pr['head']['sha']
        paths = run('git', 'diff', '--name-only', revision, cwd=base).splitlines()
        text = (pr.get('title') or '') + '\n' + (pr.get('body') or '')
        refs = {int(n) for n in re.findall(r'#([0-9]+)\b', text)}
        managed = bool(refs & numbers) or managed_paths(paths)
        if managed:
            check_commits(base, revision)
        else:
            print('Unrelated PR: managed issue trailers do not apply')
    elif args.command == 'check-commits':
        check_commits(base, args.revision)
    elif args.command == 'packet':
        data = {k: getattr(args, k) for k in ('issue', 'owner', 'paths', 'acceptance', 'checks')}
        validate_packet(data, numbers)
        packet_path(base).write_text(json.dumps(data, indent=2) + '\n')
        print(packet_path(base))
    elif args.command in {'check-staged', 'check-message'}:
        paths = staged_paths(base)
        packet = packet_path(base)
        message = Path(args.file).read_text() if args.command == 'check-message' else ''
        has_work_reference = bool(re.search(r'^Work-Issue:\s*#\d+\s*$', message, re.M))
        if not packet.exists() and not managed_paths(paths) and not has_work_reference:
            print('Unrelated changes: Rust work packet does not apply')
            return
        data = validate_packet(json.loads(packet.read_text()), numbers)
        if args.command == 'check-staged':
            check_paths(paths, data['paths'])
        else:
            check_message(message, data['issue'])
    else:
        if args.command == 'move' and args.issue not in numbers:
            raise ValueError('issue is outside managed map')
        selected = sorted(numbers if args.command == 'move' or args.include_epic else numbers - {epic})
        issues = [gh(repo, f'issues/{n}') for n in selected]
        states = validate_remote(issues, epic)
        if args.command != 'move':
            print(json.dumps(states, indent=2))
            return
        old = states[args.issue]
        transition(old, args.state, args.reason, args.evidence)
        if args.issue != epic and args.state in ACTIVE:
            if any(n != args.issue and n != epic and s in ACTIVE for n, s in states.items()):
                raise ValueError('another child already occupies the single WIP slot')
        issue = next(i for i in issues if i['number'] == args.issue)
        labels = [v['name'] for v in issue['labels'] if v['name'] not in {f'flow:{s}' for s in STATES}]
        labels.append(f'flow:{args.state}')
        comment = f"Workflow: {old} -> {args.state}\n\nReason: {args.reason.strip()}"
        if args.evidence:
            comment += f'\n\nEvidence reference (not automatically verified): {args.evidence.strip()}'
        # Save the decision before mutation; API failures remain visible and are never replayed automatically.
        gh(repo, f'issues/{args.issue}/comments', 'POST', {'body': comment})
        gh(repo, f'issues/{args.issue}', 'PATCH', {'labels': labels,
           'state': 'closed' if args.state == 'done' else 'open'})
        print(f'#{args.issue}: {old} -> {args.state}')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as exc:
        print(f'workflow: {exc}', file=sys.stderr)
        sys.exit(1)
