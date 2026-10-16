#!/usr/bin/env python3
"""Verify --check-pam without authentication or distro-specific success assumptions."""
import os
import pathlib
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
binary = pathlib.Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT/'backend/target/release/rhine-lock'
artifacts = ROOT/'backend/test-results'
artifacts.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='pam-check-', dir=artifacts) as temp:
    config = pathlib.Path(temp)/'rhine-lock'
    config.mkdir()
    env = dict(os.environ, XDG_CONFIG_HOME=temp, LC_ALL='C', USER='rhine-nobody', LOGNAME='rhine-nobody')
    for key in ('WAYLAND_DISPLAY', 'DISPLAY', 'NIRI_SOCKET', 'WAYLAND_SOCKET', 'XDG_RUNTIME_DIR', 'LD_PRELOAD'):
        env.pop(key, None)
    fixture = pathlib.Path(temp)/'pam-preflight.so'
    subprocess.run(['cc', '-shared', '-fPIC', '-Wall', '-Wextra', '-Werror',
                    str(ROOT/'backend/testdata/pam-preflight.c'), '-o', str(fixture)], check=True, timeout=30)
    isolated = dict(env, LD_PRELOAD=str(fixture))
    for service, failed in [('fixture-good', None), ('fixture-start-denied', 'start'),
                            ('fixture-account-denied', 'account'), ('fixture-credentials-denied', 'credentials')]:
        (config/'config.toml').write_text(f'[lock]\npam_service="{service}"\n')
        result = subprocess.run([str(binary), '--check-pam'], env=isolated, capture_output=True, text=True, timeout=20)
        (artifacts/f'check-pam-{service}.log').write_text(result.stdout+result.stderr)
        assert result.returncode == (1 if failed else 0), result.stderr
        lines = result.stdout.splitlines()
        stages = ['start', 'account', 'credentials']
        expected = stages if failed is None else stages[:stages.index(failed)+1]
        assert len(lines) == len(expected), lines
        for line, stage in zip(lines, expected):
            assert line.startswith(stage + (': FAILED' if stage == failed else ': OK')), lines
        assert not any(line.startswith('authenticate:') for line in lines)
        print(f'PASS {service}: correct stage/order; no authentication, display or singleton', flush=True)

    (config/'config.toml').write_text('[lock]\npam_service="fixture-hang"\n')
    started = time.monotonic()
    result = subprocess.run([str(binary), '--check-pam'], env=isolated, capture_output=True, text=True, timeout=20)
    assert result.returncode == 1 and '15 seconds' in result.stderr
    assert time.monotonic() - started < 19
    (artifacts/'check-pam-timeout.log').write_text(result.stdout+result.stderr)
    print('PASS preflight deadline: stalled account stack killed/reaped; no authentication', flush=True)

    # Optional real-policy observations remain safe: this CLI never calls
    # pam_authenticate. Valid account/credential policies vary across distros.
    for service in ('login', 'swaylock', 'rhine-lock-nonexistent-r2-service'):
        if service != 'rhine-lock-nonexistent-r2-service' and not pathlib.Path('/etc/pam.d', service).is_file():
            print(f'SKIP real {service}: service not installed', flush=True)
            continue
        (config/'config.toml').write_text(f'[lock]\npam_service="{service}"\n')
        result = subprocess.run([str(binary), '--check-pam'], env=env, capture_output=True, text=True, timeout=20)
        (artifacts/f'check-pam-{service}.log').write_text(result.stdout+result.stderr)
        assert result.returncode in (0, 1), result.stderr
        assert not any(line.startswith('authenticate:') for line in result.stdout.splitlines())
        print(f'OBSERVED real {service}: exit {result.returncode}; account/credential preflight only', flush=True)

    result = subprocess.run([str(binary), '--mock-password', 'test-only'], env=env,
                            capture_output=True, text=True, timeout=5)
    assert result.returncode == 1 and 'Unknown option: --mock-password' in result.stderr
    print('PASS production CLI contains no mock-password option', flush=True)
