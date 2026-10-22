#!/usr/bin/env python3
"""Read-only GitHub epic projection for the existing SGD board, never a task queue."""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import subprocess
import tempfile

import rewrite


def projection(issues, mapping, evidence, now):
    numbers = {row['number'] for row in mapping.values()}
    epic = mapping['RM-RUST']['number']
    if {row['number'] for row in issues} != numbers:
        raise ValueError('incomplete managed GitHub scope')
    states = rewrite.validate_remote(issues, epic)
    by_number = {row['number']: row for row in issues}
    ids = {row['number']: key for key, row in mapping.items()}
    rows = []
    for issue in sorted(issues, key=lambda x: x['number']):
        number = issue['number']
        if number == epic:
            continue
        body = issue.get('body') or ''
        dependencies = sorted(set(int(n) for n in re.findall(r'Requires\s+#(\d+)', body)))
        accepted = []
        for key, value in evidence.items():
            if isinstance(value, dict) and value.get('issue') == number:
                accepted.append({'id': key, 'status': value.get('status', ''), 'revision': value.get('source_revision', ''), 'report': value.get('report', '')})
        blocked = re.search(r'^Flow-Blocked:[ \t]*(.+)$', body, re.M)
        rows.append({'id': ids[number], 'number': number, 'title': issue['title'], 'url': issue['url'],
                     'state': issue['state'].lower(), 'flow': states[number], 'updated_at': issue['updatedAt'],
                     'remaining': re.findall(r'^\s*- \[ \]\s+(.+)$', body, re.M),
                     'dependencies': dependencies,
                     'open_dependencies': [n for n in dependencies if n in by_number and by_number[n]['state'].lower() != 'closed'],
                     'blocked_reason': blocked.group(1) if blocked else '', 'evidence': accepted})
    return {'format': 'sgd-github-overview/1', 'title': 'Rentemester · Rust-omskrivning', 'read_only_projection': True, 'authority': 'GitHub issues',
            'updated_at': now, 'epic': {k: by_number[epic][k] for k in ['number', 'title', 'url', 'state']},
            'issues': rows, 'total': len(rows), 'remaining': sum(i['state'] != 'closed' for i in rows),
            'complete': sum(i['state'] == 'closed' for i in rows)}


def fetch(repo, numbers):
    owner, name = repo.split('/')
    query = 'query { repository(owner:' + json.dumps(owner) + ', name:' + json.dumps(name) + ') {'
    query += ' '.join(f'i{n}: issue(number:{n}) {{ number title url state body updatedAt labels(first:100) {{ nodes {{ name }} }} }}' for n in sorted(numbers))
    query += '} }'
    result = subprocess.run(['gh', 'api', 'graphql', '-f', 'query=' + query], check=True, capture_output=True, text=True, timeout=60)
    value = json.loads(result.stdout)
    if value.get('errors'):
        raise ValueError('GitHub query incomplete; existing overview preserved')
    issues = list(value['data']['repository'].values())
    for issue in issues:
        issue['labels'] = issue['labels']['nodes']
    return issues


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--project', required=True, type=Path)
    args = parser.parse_args()
    base = rewrite.root()
    mapping = json.loads((base / rewrite.MAP).read_text())
    numbers, _, repo = rewrite.mapping(base)
    progress = json.loads((base / 'docs/architecture/rust-target/implementation/progress.json').read_text())
    value = projection(fetch(repo, numbers), mapping, progress['evidence'], datetime.now(timezone.utc).isoformat())
    destination = args.project.resolve() / '.sgd/overview.json'
    if not destination.parent.is_dir():
        raise ValueError('existing SGD project required')
    with tempfile.NamedTemporaryFile(mode='w', dir=destination.parent, prefix='.overview-', delete=False) as handle:
        json.dump(value, handle, ensure_ascii=False, indent=2)
        handle.write('\n')
        temporary = Path(handle.name)
    temporary.replace(destination)
    print(json.dumps({'overview': str(destination), 'total': value['total'], 'remaining': value['remaining'], 'complete': value['complete']}))


if __name__ == '__main__':
    main()
