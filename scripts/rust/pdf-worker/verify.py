#!/usr/bin/env python3
"""Actual subprocess bootstrap proof. No JS/system PDF tooling or bookkeeping fixtures."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile
import shutil
import time

p = argparse.ArgumentParser()
p.add_argument('--worker', required=True, type=pathlib.Path)
p.add_argument('--evidence', required=True, type=pathlib.Path)
a = p.parse_args()
if a.evidence.exists():
    raise SystemExit('evidence already exists')

def pdf(stream, encrypted=False):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>', b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>', b'<< /Length ' + str(len(stream)).encode() + b' >>\nstream\n' + stream + b'endstream', b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    if encrypted:
        objects.append(b'<< /Filter /Standard /V 1 /R 2 /Length 40 /O <' + b'00' * 32 + b'> /U <' + b'00' * 32 + b'> /P -4 >>')
    result = b'%PDF-1.4\n%\xe2\xe3\xcf\xd3\n'
    offsets = []
    for i, obj in enumerate(objects, 1):
        offsets.append(len(result))
        result += str(i).encode() + b' 0 obj\n' + obj + b'\nendobj\n'
    xref = len(result)
    result += b'xref\n0 ' + str(len(objects) + 1).encode() + b'\n0000000000 65535 f \n'
    result += b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets)
    result += b'trailer\n<< /Size ' + str(len(objects) + 1).encode() + b' /Root 1 0 R'
    if encrypted:
        result += b' /Encrypt 6 0 R /ID [<00112233445566778899aabbccddeeff><00112233445566778899aabbccddeeff>]'
    return result + b' >>\nstartxref\n' + str(xref).encode() + b'\n%%EOF\n'

def run(body):
    child = subprocess.run([str(a.worker)], input=body, capture_output=True, timeout=17, env={})
    assert child.returncode == 0, (child.returncode, child.stderr.decode())
    assert child.stdout.count(b'\n') == 1 and len(child.stdout) <= 16 * 1024 * 1024
    result = json.loads(child.stdout)
    assert result['inputSha256'] == hashlib.sha256(body).hexdigest()
    return result

text_pdf = pdf(b'BT\n/F1 18 Tf\n72 720 Td\n(Synthetic parser evidence) Tj\n0 -28 Td\n(Invoice SYN-001) Tj\nET\n')
started = time.monotonic()
text = run(text_pdf)
assert text['status'] == 'ok'
assert text == run(text_pdf), 'nondeterministic worker output'
page = text['pages'][0]
assert page['text'] == 'Synthetic parser evidence\nInvoice SYN-001', repr(page['text'])
assert (page['width'], page['height'], page['rotation']) == (612, 792, 0)
assert len(page['layout']) > 2 and all(item['font'] for item in page['layout'])
assert any(item['width'] > 0 and item['height'] > 0 for item in page['layout'])
assert page['layout'][0]['x'] < page['layout'][1]['x']
assert page['layout'][0]['y'] > page['layout'][-1]['y']
empty = run(pdf(b''))
assert empty['status'] == 'no_text_layer'
malformed = run(b'%PDF-1.4\nthis is deliberately malformed\n')
assert malformed['status'] == 'malformed_pdf'
encrypted = run(pdf(b'', encrypted=True))
assert encrypted['status'] == 'encrypted_pdf'
# Actual image-only page; no OCR or fabricated empty success.
image = pdf(b'q 1 0 0 1 72 72 cm BI /W 1 /H 1 /CS /RGB /BPC 8 ID \xff\x00\x00 EI Q\n')
assert run(image)['status'] == 'no_text_layer'
for bad in (b'', b'x' * (10 * 1024 * 1024 + 1)):
    child = subprocess.run([str(a.worker)], input=bad, capture_output=True, timeout=17, env={})
    assert child.returncode != 0 and child.stdout == b''
# Even with an installed system library, a bundle without the exact pinned library refuses.
with tempfile.TemporaryDirectory(prefix='rm66-missing-artifact-') as directory:
    root = pathlib.Path(directory)
    worker = root / a.worker.name
    shutil.copyfile(a.worker, worker)
    worker.chmod(0o700)
    child = subprocess.run([str(worker)], input=text_pdf, capture_output=True, timeout=17, env={})
    assert child.returncode == 1 and child.stdout == b'' and child.stderr == b'pdf_worker:artifact_unavailable\n'
    artifact = json.loads((a.worker.parent / 'pdfium' / 'artifact.json').read_text())
    library = root / 'pdfium' / artifact['library']
    library.parent.mkdir(parents=True)
    library.write_bytes(b'corrupt pinned library')
    child = subprocess.run([str(worker)], input=text_pdf, capture_output=True, timeout=17, env={})
    assert child.returncode == 1 and child.stdout == b'' and child.stderr == b'pdf_worker:artifact_hash_mismatch\n'
evidence = {'ok': True, 'elapsedSeconds': time.monotonic() - started, 'workerSha256': hashlib.sha256(a.worker.read_bytes()).hexdigest(), 'checks': ['exactSyntheticText', 'deterministic', '612x792', 'realCharacterGeometry', 'emptyAndImageOnly', 'malformed', 'encrypted', 'emptyAndOversizedRefused', 'missingAndCorruptArtifactRefused'], 'textResult': text, 'malformed': malformed, 'encrypted': encrypted, 'memoryEnforcementQualified': False}
a.evidence.write_text(json.dumps(evidence, indent=2) + '\n')
print(json.dumps({'ok': True, 'evidence': str(a.evidence), 'checks': evidence['checks']}))
