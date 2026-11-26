#!/usr/bin/env python3
"""REL-01: records the environment a check or a measurement ran in, and verifies the
artefacts an operator configuration pins.

Prints one JSON document: the commit and whether the tree is clean, the toolchain (with the
Rust host triple), the SHA-256 of the lockfiles, the machine (CPU, memory, GPU, operating
system, power plan and Smart App Control's state on Windows) and, when an operator directory
exists, whether every file that `tools/s0/*.toml`
pins (manifests, ONNX Runtime, DirectML) and every weights file those manifests name has
the digest it is pinned to. It reads files and runs `git`, `rustc`, `cargo` and (on
Windows) PowerShell's read-only `Get-CimInstance`. It uses no network, installs nothing,
writes nothing but its output, and never prints the content of a file.

    python tools/repro/environment.py --out environment.json
    python tools/repro/environment.py --operator-dir "%LOCALAPPDATA%\\Ommatix"
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

PIN = re.compile(r'^\s*([a-z0-9_]+?)_sha256\s*=\s*"([0-9a-fA-F]{64})"\s*(?:#.*)?$')
PATH_KEY = re.compile(r'^\s*([a-z0-9_]+?)\s*=\s*"([^"]+)"\s*(?:#.*)?$')


def run(args: list[str], cwd: Path = REPO) -> str | None:
    """Output of a read-only command, or None when it is not there or fails."""
    try:
        done = subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done.stdout.strip() if done.returncode == 0 else None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open('rb') as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b''):
            digest.update(chunk)
    return digest.hexdigest()


def pins(config_text: str) -> list[dict]:
    """The `<name>_sha256` pins of an operator configuration with the path they pin.

    `onnxruntime_sha256` pins `onnxruntime_path`; `detector_manifest_sha256` pins
    `detector_manifest`; `directml_sha256` pins `directml_path`.
    """
    paths: dict[str, str] = {}
    digests: dict[str, str] = {}
    for line in config_text.splitlines():
        if line.lstrip().startswith('#'):
            continue
        match = PIN.match(line)
        if match:
            digests[match.group(1)] = match.group(2).lower()
            continue
        match = PATH_KEY.match(line)
        if match:
            paths[match.group(1)] = match.group(2)
    found = []
    for name, digest in digests.items():
        path = paths.get(f'{name}_path') or paths.get(name)
        found.append({'key': f'{name}_sha256', 'path': path, 'sha256': digest})
    return found


def verify(config: Path, operator_dir: Path) -> list[dict]:
    """Each pin of `config` against the file it names under `operator_dir`."""
    results = []
    for pin in pins(config.read_text(encoding='utf-8')):
        target = operator_dir / pin['path'] if pin['path'] else None
        if target is None or not target.is_file():
            status, actual = 'absent', None
        else:
            actual = sha256_file(target)
            status = 'ok' if actual == pin['sha256'] else 'mismatch'
        results.append({**pin, 'actual': actual, 'status': status})
        if status == 'ok' and pin['key'] == 'detector_manifest_sha256':
            results.extend(verify_weights(target, pin['path']))
    return results


def verify_weights(manifest: Path, manifest_name: str) -> list[dict]:
    """The weights a manifest declares, against the digest it declares."""
    results = []
    document = json.loads(manifest.read_text(encoding='utf-8'))
    for artifact in document.get('artifacts', []):
        digest = artifact.get('sha256')
        if artifact.get('role') != 'detector' or not isinstance(digest, str):
            continue
        target = manifest.parent / artifact['path']
        actual = sha256_file(target) if target.is_file() else None
        results.append({
            'key': f'weights of {manifest_name}',
            'path': artifact['path'],
            'sha256': digest,
            'actual': actual,
            'status': 'absent' if actual is None else ('ok' if actual == digest else 'mismatch'),
            'license': artifact.get('license', '')[:60],
        })
    return results


#: Smart App Control's state, as Windows keeps it in ``VerifiedAndReputablePolicyState`` under
#: ``HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy``. When it is on, Windows may refuse to start
#: a binary built on the machine (``os error 4551``), file by file (S4_SESIONES_PENDIENTES.md §A.2).
SMART_APP_CONTROL = {0: 'off', 1: 'on', 2: 'evaluation'}


def smart_app_control(value) -> str:
    """The closed name of Smart App Control's registry state: ``off``, ``on``, ``evaluation``;
    ``not_reported`` when the value is absent and ``unknown`` for anything else, never ``off``."""
    if value is None:
        return 'not_reported'
    if isinstance(value, bool) or not isinstance(value, int):
        return 'unknown'
    return SMART_APP_CONTROL.get(value, 'unknown')


def rustc_host(verbose: str | None) -> str | None:
    """The host triple in ``rustc -vV`` (``x86_64-pc-windows-msvc`` or ``-gnu``), or None."""
    for line in (verbose or '').splitlines():
        if line.startswith('host: '):
            return line.removeprefix('host: ').strip() or None
    return None


def windows_machine() -> dict:
    """CPU, memory, GPU, operating system, power plan and Smart App Control through read-only
    queries."""
    script = (
        '$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1;'
        '$os = Get-CimInstance Win32_OperatingSystem;'
        '$gpu = Get-CimInstance Win32_VideoController | Select-Object Name, DriverVersion;'
        '$ubr = (Get-ItemProperty "HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion").UBR;'
        "$plan = (powercfg /getactivescheme) -replace '^.*\\((.*)\\)\\s*$', '$1';"
        '$sac = (Get-ItemProperty "HKLM:\\SYSTEM\\CurrentControlSet\\Control\\CI\\Policy"'
        ' -ErrorAction SilentlyContinue).VerifiedAndReputablePolicyState;'
        '[pscustomobject]@{'
        'cpu = $cpu.Name.Trim(); cores = $cpu.NumberOfCores; threads = $cpu.NumberOfLogicalProcessors;'
        'ram_gib = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1);'
        'os = $os.Caption + " build " + $os.BuildNumber + "." + $ubr;'
        'gpus = @($gpu | ForEach-Object { $_.Name + " (driver " + $_.DriverVersion + ")" });'
        'power_plan = $plan; smart_app_control_state = $sac } | ConvertTo-Json -Compress'
    )
    out = run(['powershell', '-NoProfile', '-NonInteractive', '-Command', script])
    if not out:
        return {}
    machine = json.loads(out)
    machine['smart_app_control'] = smart_app_control(machine.pop('smart_app_control_state', None))
    return machine


def environment(operator_dir: Path | None) -> dict:
    lockfiles = {
        str(p.relative_to(REPO)).replace('\\', '/'): sha256_file(p)
        for p in [REPO / 'Cargo.lock', *sorted((REPO / 'tools').glob('*/*/Cargo.lock'))]
        if p.is_file()
    }
    record = {
        'commit': run(['git', 'rev-parse', 'HEAD']),
        'tree_clean': run(['git', 'status', '--porcelain', '--untracked-files=no']) == '',
        'rustc': run(['rustc', '-V']),
        'rustc_host': rustc_host(run(['rustc', '-vV'])),
        'cargo': run(['cargo', '-V']),
        'toolchain_file': (REPO / 'rust-toolchain.toml').read_text(encoding='utf-8'),
        'lockfiles_sha256': lockfiles,
        'python': platform.python_version(),
        'platform': platform.platform(),
        'machine': windows_machine() if sys.platform == 'win32' else {
            'cpu': platform.processor(), 'logical_processors': os.cpu_count(),
        },
    }
    if operator_dir is not None:
        record['operator_directory'] = {
            'present': operator_dir.is_dir(),
            'pins': [
                {'config': config.name, **result}
                for config in sorted((REPO / 'tools' / 's0').glob('*.toml'))
                for result in (verify(config, operator_dir) if operator_dir.is_dir() else [])
            ],
        }
    return record


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--out', type=Path, help='write the JSON here instead of stdout')
    parser.add_argument(
        '--operator-dir', type=Path,
        help='the operator directory that holds the pinned files '
             '(default: %%LOCALAPPDATA%%\\Ommatix when it exists)')
    args = parser.parse_args(argv)
    operator_dir = args.operator_dir
    if operator_dir is None and os.environ.get('LOCALAPPDATA'):
        candidate = Path(os.environ['LOCALAPPDATA']) / 'Ommatix'
        operator_dir = candidate if candidate.is_dir() else None
    record = environment(operator_dir)
    text = json.dumps(record, ensure_ascii=False, indent=2) + '\n'
    if args.out:
        args.out.write_text(text, encoding='utf-8')
    else:
        sys.stdout.write(text)
    bad = [p for p in record.get('operator_directory', {}).get('pins', []) if p['status'] == 'mismatch']
    return 1 if bad else 0


if __name__ == '__main__':
    raise SystemExit(main())
