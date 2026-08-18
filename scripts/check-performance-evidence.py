#!/usr/bin/env python3
"""Reject a release package when its performance evidence is stale or invalid."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

def fail(message):
    raise SystemExit(f"check-performance-evidence: {message}")

parser = argparse.ArgumentParser()
parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
parser.add_argument('--evidence', type=Path, required=True)
parser.add_argument('--version', required=True)
parser.add_argument('--rust-target', required=True)
parser.add_argument('--binary', type=Path)
parser.add_argument('--minimum-main-update-hz', type=float, default=100.0)
parser.add_argument('--minimum-capture-submit-hz', type=float, default=100.0)
args = parser.parse_args()
root, evidence_path = args.root.resolve(), args.evidence.resolve()
if not evidence_path.is_file(): fail(f"missing Release performance evidence: {evidence_path}")
try:
    evidence = json.loads(evidence_path.read_text(encoding='utf-8-sig'))
except (OSError, json.JSONDecodeError) as exc:
    fail(f"cannot read evidence {evidence_path}: {exc}")
if evidence.get('schema') != 'daedalus-performance-v1': fail('unsupported evidence schema')
if evidence.get('profile') != 'release': fail('evidence was not collected from a Release profile')
if evidence.get('rust_target') != args.rust_target:
    fail(f"evidence Rust target {evidence.get('rust_target')!r} does not match required {args.rust_target!r}")
expected_features = {'talos', 'distribution-release', 'learning-release'} if args.version.endswith('-learning') else {'talos', 'distribution-release', 'contest-release'}
if set(evidence.get('features') or []) != expected_features: fail(f'evidence features must be {sorted(expected_features)!r}')
if evidence.get('corner_labels_enabled') is not False: fail('baseline Release performance evidence must keep corner-label export disabled')
if evidence.get('version') != args.version: fail(f"evidence version {evidence.get('version')!r} does not match VERSION {args.version!r}")
if evidence.get('source_dirty') is not False: fail('formal Release evidence must be collected from a clean committed checkout')
binary_path = args.binary.resolve() if args.binary else Path(str(evidence.get('binary_path', '')))
if binary_path.parent.name.lower() != 'release': fail(f"evidence binary is not under a release directory: {binary_path}")
if not binary_path.is_file(): fail(f"evidence binary is missing: {binary_path}")
binary_sha256 = hashlib.sha256(binary_path.read_bytes()).hexdigest()
if evidence.get('binary_sha256') != binary_sha256: fail('evidence binary hash does not match the measured Release binary')
metrics = evidence.get('metrics') or {}
for key, minimum in (('main_update_hz', args.minimum_main_update_hz), ('capture_copy_submit_hz', args.minimum_capture_submit_hz)):
    try: value = float(metrics[key])
    except (KeyError, TypeError, ValueError): fail(f"evidence is missing numeric metric {key}")
    if value < minimum: fail(f"{key} {value:.3f} Hz is below required {minimum:.3f} Hz")
if metrics.get('talos_image_transport') != 'tcp': fail(f"evidence transport must be tcp, got {metrics.get('talos_image_transport')!r}")
source_commit = str(evidence.get('source_commit', ''))
if not source_commit: fail('evidence is missing source_commit')
if subprocess.run(['git', '-C', str(root), 'merge-base', '--is-ancestor', source_commit, 'HEAD']).returncode != 0: fail(f"evidence source commit is not an ancestor of HEAD: {source_commit}")
performance_paths = ['Cargo.toml', 'Cargo.lock', 'src', 'assets', 'config.performance.toml', 'release', 'sdk']
if subprocess.run(['git', '-C', str(root), 'diff', '--quiet', f'{source_commit}..HEAD', '--', *performance_paths]).returncode != 0: fail('performance-relevant files changed after the evidence source commit; rerun the Release benchmark')
print(f"performance evidence accepted: {evidence_path}")
