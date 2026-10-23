#!/usr/bin/env python3
"""Work-Issue #698: supplied-artifact actual disposal qualifier; never builds/retries.

Arguments: --server-binary PATH --web-dir PATH --cli-binary PATH
--mcp-binary PATH --evidence NEW_PATH (must contain HEAD's first 12 digits).
Reuse the admitted batch lifecycle, replacing only the original170 business proof.
No old170 matrix is executed. Failed private profiles/phase receipts are retained.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

REPO = Path(__file__).resolve().parents[2]
CHECKS = ['partial', 'fullyDepreciated', 'noDepreciation', 'sameKeyReplay',
          'changedIntentRefused', 'closedPeriodRefused', 'stalePlanRefused',
          'revokedRefused', 'evidenceRefused', 'laterDepreciationRefused',
          'correctionRefused', 'restart', 'signedSourceFreeRestore', 'cliMcpSharedWeb']


def source_hashes():
    paths = subprocess.check_output(['git', 'ls-files', '-z', '--', 'crates',
        'app/src', 'rules', 'config', 'templates', 'Cargo.toml', 'Cargo.lock',
        'scripts/rust'], cwd=REPO).split(b'\0')
    return {p.decode(): hashlib.sha256((REPO / p.decode()).read_bytes()).hexdigest()
            for p in paths if p}


def replace_once(source, old, new):
    if source.count(old) != 1:
        raise RuntimeError('Lifecycle adapter source drift: ' + old[:100])
    return source.replace(old, new, 1)


def prepare_lifecycle():
    runner = REPO / 'scripts/rust/run-bookkeeping-batch.py'
    source = runner.read_text()
    # Keep ownership, scanner, MFA, artifact pinning, signatures and space policy.
    source = replace_once(source, "'scripts/rust/check-native-asset-original.ts' if args.original_asset",
                           "'scripts/rust/asset-disposal-journey.ts' if args.original_asset")
    source = replace_once(source,
        "tables += ['assets', 'asset_depreciation_entries', 'asset_writeoffs', 'exceptions']",
        "tables += ['assets', 'asset_depreciation_entries', 'asset_writeoffs', 'native_asset_disposals', 'exceptions']")
    old = "expected = {'documents': 2, 'assets': 1, 'asset_depreciation_entries': 3, 'asset_writeoffs': 1, 'exceptions': 4, 'journal_entries': 5, 'journal_lines': 10}"
    source = replace_once(source, old,
        "expected = {'documents': 4, 'assets': 3, 'asset_depreciation_entries': 4, 'asset_writeoffs': 0, 'native_asset_disposals': 3, 'exceptions': 0, 'journal_entries': 10, 'journal_lines': 21}")
    source = replace_once(source, "            phase('finish', 20)",
                          "            phase('disposal-reconcile', 120)\n            phase('finish', 20)")
    source = replace_once(source,
        "report['status'] != 'passed':", "(report['status'] != 'passed' or args.original_asset):")
    source = replace_once(source, "        def bank_original():\n", "        def bank_original():\n            return  # Disposal fixture has no bank statement; originals remain verified.\n")
    source = replace_once(source, r"r'synthetic-(?:batch-purchase-[12]|party166-internal)\.txt'", r"r'synthetic-batch-purchase-[1-4]\.txt'")
    source = replace_once(source, "else 2 if (args.original_accrual or args.original_asset)", "else 4 if args.original_asset else 2 if args.original_accrual")
    start_print = source.index("            print(('PASS:")
    end_print = source.index('\n        except BaseException', start_print)
    source = source[:start_print] + "            print('Lifecycle completed; final disposal qualification pending: ' + str(args.evidence))" + source[end_print:]
    spec = importlib.util.spec_from_loader('disposal_lifecycle', loader=None)
    module = importlib.util.module_from_spec(spec)
    module.__file__ = str(runner)
    exec(compile(source, str(runner), 'exec'), module.__dict__)
    module.BROWSER = replace_once(module.BROWSER, 'm.createOriginalAssetProof(', 'm.createAssetDisposalProof(')
    module.BROWSER = replace_once(module.BROWSER, "else if(action==='finish')", "else if(action==='disposal-reconcile')value=await originalAssetProof.reconcile(ownerPage);else if(action==='finish')")
    # Reuse ordinary scanned original intake, but no unrelated statement rows or
    # their unresolved exceptions in this disposal-only fixture.
    module.BROWSER = replace_once(module.BROWSER, '[[1,125,25],[2,250,50]]', '[[1,30,0],[2,60,0],[3,35,0],[4,250,0]]')
    start = module.BROWSER.index('const csv=`dato;tekst;beløb;valuta;saldo')
    end = module.BROWSER.index('async function rules()', start)
    module.BROWSER = module.BROWSER[:start] + "await originalProof(ownerPage);return{today,originals};}\n" + module.BROWSER[end:]
    return module, runner


def main():
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    for name in ['server-binary', 'web-dir', 'cli-binary', 'mcp-binary', 'evidence']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    pins = source_hashes()
    module, runner = prepare_lifecycle()
    original_argv = sys.argv[:]
    sys.argv = [str(runner), *original_argv[1:], '--original-asset']
    try:
        module.main()
        report = json.loads(args.evidence.read_text())
        proof = report['results']['original-asset']
        checks = {**proof['checks'], 'restart': report['results']['reopen']['restartNormalMfa'],
                  'signedSourceFreeRestore': report['sourceFreeRestoreQualified'] and report['independentArchiveVerification'] and report['restoredResolutionInventoryVerified'] and report['results']['disposal-reconcile']['mappedOriginalKeyReplay']}
        if set(checks) != set(CHECKS) or any(checks[k] is not True for k in CHECKS):
            raise RuntimeError('Incomplete actual disposal checks; no acceptance claim')
        if pins != source_hashes():
            raise RuntimeError('Final qualifier raw tracked source hashes changed')
        report.update(status='passed', workflow='asset_disposal698', checks=checks,
                      sourceHashes=pins, binarySHA256=report['artifacts'],
                      realOperationCounts=proof['operationCounts'],
                      durableJournalObservations=proof['journals'])
        report.pop('originalAssetQualified', None)
        args.evidence.write_text(json.dumps(report, indent=2) + '\n')
        print('PASS: actual disposal698 journals, shared clients/UI, restart and explicitly reconciled signed restore: ' + str(args.evidence))
    except BaseException as error:
        if args.evidence.exists():
            report = json.loads(args.evidence.read_text())
            report.update(status='failed', qualifierFailure=str(error), sourceHashes=pins)
            args.evidence.write_text(json.dumps(report, indent=2) + '\n')
        raise
    finally:
        sys.argv = original_argv


if __name__ == '__main__':
    main()
