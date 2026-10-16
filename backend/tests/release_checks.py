"""Release regressions, using the same mock backend and semantic production paths."""
import io
import json
import os
import shlex
import signal
import subprocess
from PIL import Image


def unicode_boundary(h, count):
    password = 'é😀鍵' * 454 + 'x' * 10
    assert len(password.encode('utf-8')) == 4096
    a = h.start('release-unicode-boundary', '--mock-password', password)
    try:
        a.ready(count=count)
        a.password(password)
        a.exited()
        h.passed('4096-byte UTF-8 password with non-BMP characters survives the complete bridge/worker path')
    finally:
        a.cleanup()
        h.lockers.remove(a)


def run_release_checks(h):
    count = len([o for o in json.loads(subprocess.check_output(['swaymsg', '-t', 'get_outputs'])) if o['active']])
    unicode_boundary(h, count)
    helpers = h.ART / 'startup-helper'
    helpers.mkdir(exist_ok=True)
    sentinel = helpers / 'started'
    sentinel.unlink(missing_ok=True)
    grim = helpers / 'grim'
    grim.write_text('#!/bin/sh\nprintf started > ' + shlex.quote(str(sentinel)) + '\nexec /bin/sleep 5\n')
    grim.chmod(0o700)
    a = h.start('release-startup', extra_env={'PATH': str(helpers)})
    try:
        h.wait_for(sentinel.exists, 'capture helper started before lock')
        other = subprocess.run([str(h.BIN), '--mock-password', 'rhine-test', '--daemonize'],
                               capture_output=True, timeout=10)
        assert other.returncode == 0, other.stderr
        assert 'Session lock acquired' in (h.ART / 'release-startup.log').read_text()
        assert a.state()['locked']
        assert a.state()['dumpable'] == 0
        a.ready(count=count)
        inherited = subprocess.run([str(h.BIN), '--mock-password', 'rhine-test', '--daemonize'],
                                   env=dict(os.environ, WAYLAND_SOCKET='999'), capture_output=True, timeout=5)
        (h.ART/'release-inherited-connection.log').write_bytes(inherited.stderr)
        assert inherited.returncode == 1 and b'WAYLAND_SOCKET is unsupported' in inherited.stderr
        assert a.state()['locked']
        h.passed('inherited Wayland connection is rejected before singleton readiness can confirm the wrong display')
        a.password('rhine-test')
        a.exited()
        h.passed('concurrent daemon waits for compositor lock confirmation')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    a = h.start('release-panic-twice')
    b = None
    try:
        a.ready(count=count)
        a.send('panic')
        h.wait_for(lambda: 'Native fallback' in (h.ART/'release-panic-twice.log').read_text(), 'first panic native recovery')
        a.ready(count=count, fallback=True)
        assert a.state()['dumpable'] == 0
        a.send('panic')
        assert a.process.wait(timeout=5) == -signal.SIGABRT
        log = (h.ART/'release-panic-twice.log').read_text()
        assert log.count('Fatal backend panic') == 2 and 'restart already used' in log
        b = h.start('release-after-panic-limit')
        b.ready(count=count)
        b.password('rhine-test')
        b.exited()
        h.passed('second panic stops the restart loop; compositor remains recoverable')
    finally:
        a.cleanup()
        h.lockers.remove(a)
        if b:
            b.cleanup()
            h.lockers.remove(b)

    for fault in ('hang', 'exit', 'empty', 'account'):
        a = h.start('release-auth-' + fault, frontend=h.ART/'missing-frontend',
                    extra_env={'RHINE_TEST_AUTH_FAULT_ONCE': fault})
        try:
            a.ready(count=count, fallback=True)
            a.request('set_value', control='unlock-password', value='rhine-test')
            import time
            started = time.monotonic()
            a.request('invoke_action', control='unlock-submit')
            result = a.auth('failure')
            assert time.monotonic() - started >= 1.19
            assert result['locked'] and result['auth']['attempts'] == 1
            assert result['auth']['stage'] == ('account' if fault == 'account' else 'authenticate')
            if fault == 'account':
                assert 'Password accepted' in result['windows'][0]['status']
            if fault == 'hang':
                assert 'timed out' in result['auth']['message']
            if fault == 'empty':
                assert 'Missing credential result' in result['auth']['message']
            a.request('set_value', control='unlock-password', value='rhine-test')
            a.request('invoke_action', control='unlock-submit')
            a.exited()
            h.passed('credential worker ' + fault + ' fails closed; next attempt can unlock')
        finally:
            a.cleanup()
            h.lockers.remove(a)

    a = h.start('release-password-boundary')
    try:
        a.ready(count=count)
        for value in ('\0', 'x' * 4097, '密' * 1366):
            a.password(value)
            result = a.auth('failure')
            assert result['locked'] and result['auth']['stage'] == 'authenticate'
            assert '4096' in result['auth']['message']
        a.password('rhine-test')
        a.exited()
        h.passed('NUL and overlong ASCII/UTF-8 inputs reject without PAM; retry works')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    a = h.start('release-bridge-validation')
    try:
        a.ready(count=count)
        a.js("for(const m of [null,0,[],{}, {type:1}, {type:'authenticate',password:5}, {type:'unlock-ready'}, {type:'authenticate',password:'x'.repeat(70000)}]) webkit.messageHandlers.rhine.postMessage(m);true")
        assert a.state()['locked'] and a.state()['auth']['state'] == 'idle'
        a.send('evaluate', script="webkit.messageHandlers.rhine.postMessage({type:'hello',protocol:999});true")
        h.wait_for(lambda: a.state()['windows'][0]['fallback'], 'unsupported protocol native fallback')
        a.request('set_value', control='unlock-password', value='rhine-test')
        a.request('invoke_action', control='unlock-submit')
        a.exited()
        h.passed('malformed/oversized/out-of-order bridge messages cannot authorize unlocking')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    import shutil
    assets = h.ART/'resource-fixture'
    shutil.copytree(h.STUB, assets, dirs_exist_ok=True)
    sentinel = h.ART/'outside-resource'
    sentinel.write_text('synthetic outside-root sentinel')
    (assets/'escape').unlink(missing_ok=True)
    (assets/'escape').symlink_to(sentinel.resolve())
    (assets/'fifo').unlink(missing_ok=True)
    os.mkfifo(assets/'fifo', 0o600)
    with open(assets/'oversize', 'wb') as file:
        file.truncate(64 * 1024 * 1024 + 1)
    a = h.start('release-scheme-bounds', frontend=assets)
    try:
        a.ready(count=count)
        a.js("window.resourceChecks={};for(const name of ['escape','fifo','oversize'])fetch('rhine://app/'+name).then(r=>resourceChecks[name]=r.status);true")
        h.wait_for(lambda: a.js("Object.keys(resourceChecks).length===3"), 'bounded scheme replies')
        assert a.js('resourceChecks') == {'escape':404, 'fifo':404, 'oversize':404}
        a.password('rhine-test')
        a.exited()
        h.passed('outside symlink, FIFO and oversized scheme file rejected; GTK remains usable')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    names = [o['name'] for o in json.loads(subprocess.check_output(['swaymsg','-t','get_outputs'])) if o['active']]
    a = h.start('release-no-outputs')
    try:
        a.ready(count=count)
        a.password('wrong')
        assert a.state()['auth']['state'] == 'verifying'
        for name in names:
            subprocess.run(['swaymsg','output',name,'disable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        h.wait_for(lambda: not a.state()['windows'], 'all outputs removed')
        assert a.state()['locked']
        for name in names:
            subprocess.run(['swaymsg','output',name,'enable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        a.ready(count=count)
        assert a.auth('failure')['locked']
        a.password('rhine-test')
        a.exited()
        h.passed('all outputs can disappear during authentication and return without losing the lock')
    finally:
        for name in names:
            subprocess.run(['swaymsg','output',name,'enable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        a.cleanup()
        h.lockers.remove(a)

    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP, signal.SIGKILL):
        a = h.start('release-signal-' + sig.name)
        b = None
        try:
            a.ready(count=count)
            os.kill(a.pid, sig)
            assert a.process.wait(timeout=5) != 0
            # An abandoned lock is red in private Sway. The underlying desktop
            # must not become visible before the replacement authenticates.
            def abandoned():
                frame = Image.open(io.BytesIO(subprocess.check_output(['grim', '-o', 'HEADLESS-1', '-'], timeout=5))).convert('RGB')
                return frame.getextrema() == ((255, 255), (0, 0), (0, 0))
            h.wait_for(abandoned, 'Sway shows abandoned lock after ' + sig.name)
            b = h.start('release-recover-' + sig.name)
            b.ready(count=count)
            b.password('rhine-test')
            b.exited()
            h.passed(sig.name + ' stays locked; replacement authenticates and unlocks')
        finally:
            a.cleanup()
            h.lockers.remove(a)
            if b:
                b.cleanup()
                h.lockers.remove(b)

    def finish_native(a, output=0):
        a.request('set_value', control='unlock-password', value='rhine-test', output=output)
        a.request('invoke_action', control='unlock-submit', output=output)
        a.exited()

    nohello = h.ART / 'no-hello'
    nohello.mkdir(exist_ok=True)
    (nohello / 'index.html').write_text('<!doctype html><title>Missing handshake fixture</title>')
    a = h.start('release-nohello', frontend=nohello)
    try:
        a.ready(count=count, fallback=True)
        finish_native(a)
        h.passed('no handshake leads to usable native input')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    a = h.start('release-hung-web')
    try:
        a.ready(count=count)
        # Do not await a reply to intentionally nonterminating JS. GTK and the
        # independent semantic pipe must stay responsive while the view hangs.
        a.send('evaluate', script='while (true) {}')
        h.wait_for(lambda: any(w['fallback'] for w in a.state()['windows']),
                   'hung web process replaced', timeout=15)
        idx = next(i for i, w in enumerate(a.state()['windows']) if w['fallback'])
        finish_native(a, idx)
        h.passed('post-hello JS hang falls back and remains unlockable')
    finally:
        a.cleanup()
        h.lockers.remove(a)

    a = h.start('release-native-shortcut')
    keyboard = None
    try:
        a.ready(count=count)
        keyboard = subprocess.Popen(['wtype', '-', '-M', 'ctrl', '-M', 'shift', '-k', 'Escape',
                                     '-m', 'shift', '-m', 'ctrl', '-s', '30000'], stdin=subprocess.PIPE)
        h.wait_for(lambda: any('virtual' in x['identifier'].lower() for x in json.loads(
                   subprocess.check_output(['swaymsg', '-t', 'get_inputs']))), 'virtual keyboard registered')
        h.wait_for(lambda: any(a.js('document.hasFocus()', output=i) for i in range(count)), 'shortcut view focused')
        # Prove capture-phase recovery also works before the eight-second
        # watchdog when the page can no longer process keyboard events.
        focused = next(i for i in range(count) if a.js('document.hasFocus()', output=i))
        a.send('evaluate', script='while (true) {}', output=focused)
        keyboard.stdin.close()
        h.wait_for(lambda: any(w['fallback'] for w in a.state()['windows']), 'native shortcut on hung page', timeout=4)
        idx = next(i for i, w in enumerate(a.state()['windows']) if w['fallback'])
        keyboard.terminate()
        keyboard.wait(timeout=5)
        keyboard = None
        h.wait_for(lambda: not any('virtual' in x['identifier'].lower() for x in json.loads(
                   subprocess.check_output(['swaymsg', '-t', 'get_inputs']))), 'old virtual keyboard removed')
        a.request('set_value', control='unlock-password', value='temporary', output=idx)
        keyboard = subprocess.Popen(['wtype', '-', '-k', 'Escape', '-s', '30000'], stdin=subprocess.PIPE)
        h.wait_for(lambda: any('virtual' in x['identifier'].lower() for x in json.loads(
                   subprocess.check_output(['swaymsg', '-t', 'get_inputs']))), 'clear-key keyboard registered')
        keyboard.stdin.close()
        h.wait_for(lambda: a.state()['windows'][idx]['passwordBytes'] == 0, 'native Escape clears password')
        assert a.state()['auth']['state'] == 'idle' and a.state()['locked']
        h.shot('release-native-shortcut')
        finish_native(a, idx)
        h.passed('real Ctrl+Shift+Escape recovers a hung page before the watchdog; native Escape clears without authenticating')
    finally:
        if keyboard:
            keyboard.terminate()
            keyboard.wait(timeout=5)
        a.cleanup()
        h.lockers.remove(a)

    a = h.start('release-panic')
    try:
        a.ready(count=count)
        a.send('panic')
        # Wait on the new process image before writing another request: the
        # old stdin reader may otherwise consume it just before exec discards
        # that process's channel/buffer.
        h.wait_for(lambda: 'Native fallback' in (h.ART / 'release-panic.log').read_text(),
                   're-exec native fallback')
        a.ready(count=count, fallback=True)
        a.request('set_value', control='unlock-password', value='rhine-test')
        a.request('invoke_action', control='unlock-submit')
        assert a.process.wait(timeout=8) == 0
        log = (h.ART / 'release-panic.log').read_text()
        assert log.count('Fatal backend panic') == 1 and 'Session unlocked' in log
        h.passed('one panic re-execs into usable native fallback')
    finally:
        a.cleanup()
        h.lockers.remove(a)
