#!/usr/bin/env python3
"""Bounded semantic tests of the actual GTK/WebKit/session-lock application.
Every process is a mock-auth build under tools/headless.sh. Artifacts contain no real credentials.
"""
import json, os, pathlib, select, signal, subprocess, sys, time, traceback
from PIL import Image, ImageStat
ROOT = pathlib.Path(__file__).resolve().parents[2]
BIN = ROOT / 'backend/target/debug/rhine-lock'
ART = ROOT / 'backend/test-results'
STUB = ROOT / 'backend/testdata/stub-frontend'
assert os.environ.get('RHINE_HEADLESS_TEST') == '1'
runtime = pathlib.Path(os.environ['XDG_RUNTIME_DIR']).resolve()
assert runtime.name.startswith('rl-headless.')
assert runtime.parent == pathlib.Path(os.environ.get('TMPDIR', '/tmp')).resolve()
assert runtime.stat().st_uid == os.getuid() and runtime.stat().st_mode & 0o077 == 0
assert not os.environ.get('NIRI_SOCKET')
ART.mkdir(exist_ok=True)
# Private test configuration, no user preferences or services.
CONFIG = ART / 'config/rhine-lock'
CONFIG.mkdir(parents=True, exist_ok=True)
(CONFIG/'config.toml').write_text('[lock]\nallow_power=false\n[frontend]\nfixture=true\n')


def wait_for(fn, description, timeout=12):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = fn()
        if last:
            return last
        time.sleep(.04) # condition polling, not a blind action delay
    raise AssertionError(f'Timed out: {description}; last={last!r}')


def shot(name):
    path = ART / f'{name}.png'
    def capture():
        subprocess.run(['grim', '-o', 'HEADLESS-1', str(path)], check=True, timeout=5)
        im = Image.open(path).convert('RGB')
        colors=im.getcolors(im.width*im.height) or []
        largest=max((count for count,_ in colors),default=0)
        return path if max(ImageStat.Stat(im).var) > 5 and largest/(im.width*im.height)<.99 else None
    return wait_for(capture, 'nonblank screenshot '+name, timeout=8)


class Locker:
    def __init__(self, name, *args, frontend=STUB, extra_env=None):
        self.name, self.seq = name, 0
        self.log = open(ART / f'{name}.log', 'w')
        self.trace = open(ART / f'{name}.jsonl', 'w')
        env = dict(os.environ, RHINE_TEST_CONTROL='1', XDG_CONFIG_HOME=str(CONFIG.parent),
                   LC_ALL='C.UTF-8', TZ='UTC', GDK_BACKEND='wayland')
        env.update(extra_env or {})
        # No bus access in mock builds; an independent address also isolates GTK accessibility.
        self.process = subprocess.Popen([str(BIN), '--mock-password', 'rhine-test', '--debug',
                          '--frontend', str(frontend), *args], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=self.log, env=env, bufsize=0)
        self.buffer = b''
        self.pid = None
        self.pidfd = None

    def send(self, op, **params):
        self.seq += 1
        self.trace.write(json.dumps({'request':{'protocol':1,'id':self.seq,'op':op,'params':params}})+'\n');self.trace.flush()
        self.process.stdin.write((json.dumps({'protocol':1,'id':self.seq,'op':op,'params':params})+'\n').encode())
        return self.seq

    def request(self, op, **params):
        wanted = self.send(op, **params)
        deadline = time.monotonic()+15
        while time.monotonic() < deadline:
            if b'\n' not in self.buffer:
                readable,_,_ = select.select([self.process.stdout],[],[],max(0,deadline-time.monotonic()))
                if not readable: break
                data = os.read(self.process.stdout.fileno(),65536)
                if not data: raise AssertionError(f'{self.name}: child exited, see log')
                self.buffer += data
                continue
            line,self.buffer = self.buffer.split(b'\n',1)
            try: reply = json.loads(line)
            except ValueError: continue
            if reply.get('id') == wanted:
                self.trace.write(json.dumps({'response':reply})+'\n');self.trace.flush()
                assert reply['ok'], reply
                return reply.get('data')
        raise AssertionError(f'{self.name}: request timeout {op}')

    def state(self):
        state = self.request('get_state')
        self.pid = state['pid']
        if self.pidfd is None:
            self.pidfd = os.pidfd_open(self.pid)
        return state

    def ready(self, count=1, fallback=False, locked=True):
        return wait_for(lambda: (lambda s:s if s['locked']==locked and len(s['windows'])==count and
                   all(w['fallback'] if fallback else w['ready'] for w in s['windows']) else None)(self.state()),
                   f'{self.name}: {count} ready windows', timeout=25)

    def js(self, script, output=0):
        return self.request('evaluate', script=script, output=output)

    def password(self, value, output=0):
        # Select the actual labelled HTML control and submit the actual form event handler.
        self.js("(()=>{const e=document.querySelector('input[type=password]');e.value="+json.dumps(value)+";e.dispatchEvent(new Event('input',{bubbles:true}));if(e.form)e.form.requestSubmit();else e.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));return true})()", output)

    def auth(self, expected):
        return wait_for(lambda: (lambda s: s if s['auth']['state']==expected else None)(self.state()), 'auth '+expected)

    def exited(self, daemon=False):
        if daemon:
            wait_for(lambda: not pathlib.Path(f'/proc/{self.pid}').exists() or
                     pathlib.Path(f'/proc/{self.pid}/stat').read_text().split()[2]=='Z', 'daemon exit')
        else:
            assert self.process.wait(timeout=8)==0
        self.log.flush()
        text=(ART/f'{self.name}.log').read_text()
        assert 'Session unlocked' in text
        assert 'Fatal backend panic' not in text
        assert 'frontend [error]' not in text, f'Unexpected JS error: {self.name}; see log'

    def cleanup(self):
        # Capture the state before tearing down an unexpectedly live process.
        live = self.process.poll() is None or (self.pidfd is not None and not select.select([self.pidfd], [], [], 0)[0])
        if live:
            try: subprocess.run(['grim', '-o', 'HEADLESS-1', str(ART/f'{self.name}-failure.png')], timeout=3, check=False, stderr=subprocess.DEVNULL)
            except Exception: pass
        if self.pidfd is not None:
            try: signal.pidfd_send_signal(self.pidfd, signal.SIGTERM)
            except ProcessLookupError: pass
            os.close(self.pidfd)
            self.pidfd = None
        self.pid = None
        if self.process.poll() is None:
            self.process.terminate()
        try: self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        self.log.close();self.trace.close()


lockers=[]
def start(name,*args,**kwargs):
    obj=Locker(name,*args,**kwargs);lockers.append(obj);return obj

def passed(text): print('PASS '+text, flush=True)

def autoplay(locker, output=0):
    report=wait_for(lambda:locker.js("window.fixtureAudioReports.find(r=>r.state==='running') || null",output),
                    'AudioContext running before input')
    assert report['keyEvents']==0, report
    assert locker.js('window.fixtureKeyEvents',output)==0

def run_checks():
    try:
        a=start('main')
        a.ready()
        autoplay(a)
        passed('AudioContext running before any key event')
        shot('locked-stub')
        assert a.js("document.activeElement.id")=='password'
        wait_for(lambda:a.js('window.fixtureModuleLoaded===true'),'ES module loaded')
        assert a.js("document.styleSheets[0].cssRules.length")>0
        assert a.js("getComputedStyle(document.querySelector('button')).backgroundColor")== 'rgb(255, 123, 0)'
        init=a.js("JSON.parse(document.querySelector('#messages').textContent.split('\\n').find(x=>x.includes('\"type\":\"init\"')))")
        assert init['protocol']==1 and init['config']['fixture'] is True and init['mock'] is True
        assert init['output']['primary'] is True
        passed('lock acquired; rendered screenshot nonblank; hello/init/config; password focus')

        # A window close, unsolicited unlock-ready and unknown commands cannot unlock.
        a.js("window.webkit.messageHandlers.rhine.postMessage({type:'unlock-ready'});window.webkit.messageHandlers.rhine.postMessage({type:'unknown'});true")
        a.request('close_window')
        assert a.state()['locked']
        passed('cancel/close and unauthenticated unlock-ready leave session locked')
        other_runtime=ART/'other-runtime';other_runtime.mkdir(exist_ok=True);other_runtime.chmod(0o700)
        refusal_env=dict(os.environ,XDG_RUNTIME_DIR=str(other_runtime),WAYLAND_DISPLAY=str(pathlib.Path(os.environ['XDG_RUNTIME_DIR'])/os.environ['WAYLAND_DISPLAY']))
        refused=subprocess.run([str(BIN),'--mock-password','rhine-test','--frontend',str(STUB)],env=refusal_env,capture_output=True,timeout=8)
        (ART/'compositor-refusal.log').write_bytes(refused.stderr)
        assert refused.returncode==1 and b'Compositor refused' in refused.stderr
        passed('compositor refusal exits 1 without disturbing the existing lock')

        a.password('')
        empty=a.auth('failure');assert empty['auth']['attempts']==1
        assert empty['auth']['stage']=='authenticate'
        started=time.monotonic();a.password('wrong')
        assert a.state()['auth']['state']=='verifying'
        assert a.js("document.querySelector('#unlock').disabled")
        a.js("window.webkit.messageHandlers.rhine.postMessage({type:'authenticate',password:'rhine-test'});true")
        failed=a.auth('failure')
        assert time.monotonic()-started >= 1.19
        assert failed['locked'] and failed['auth']['attempts']==2
        assert failed['auth']['stage']=='authenticate' and failed['auth']['message']=='Authentication failed'
        assert 'Authentication failed at authenticate:' in (ART/'main.log').read_text()
        wait_for(lambda:a.js("JSON.parse(document.querySelector('#messages').textContent.trim().split('\\n').filter(x=>JSON.parse(x).type==='auth').at(-1)).stage")=='authenticate','failure stage delivered to frontend')
        passed('empty/wrong password rejected with authenticate stage and diagnostic; >=1.2 s floor; duplicate ignored; still locked')

        # Read-only network/navigation security checks with expected failure capture.
        a.js("window.securityResults={};fetch('https://example.invalid/forbidden').then(()=>securityResults.network='BAD',()=>securityResults.network='blocked');true")
        wait_for(lambda:a.js("window.securityResults.network")=='blocked','CSP blocks network')
        a.js("location.href='file:///etc/passwd';true")
        assert a.js("location.protocol")=='rhine:'
        a.js("window.open('https://example.invalid');true")
        assert len(a.state()['windows'])==1
        passed('CSP prevents network requests; file navigation and popup blocked')

        # Reload restores current failed state; keyboard focus and Tab reach actual controls.
        a.request('reload');a.ready()
        autoplay(a)
        wait_for(lambda:a.js("document.querySelector('#status').textContent")=='failure','reload auth snapshot')
        a.js("document.querySelector('#password').focus();true")
        # Gate wtype on observable focus before releasing its stdin barrier. Keep its
        # virtual keyboard alive until assertions finish (the trailing sleep is a
        # bounded device lifetime, never used as a synchronization condition).
        keyboard=subprocess.Popen(['wtype','-','-k','Tab','-s','30000'],stdin=subprocess.PIPE)
        try:
            wait_for(lambda:any('virtual' in x['identifier'].lower() for x in json.loads(subprocess.check_output(['swaymsg','-t','get_inputs']))),'virtual keyboard registered')
            wait_for(lambda:a.js('document.hasFocus()'),'compositor keyboard focus')
            keyboard.stdin.close()
            wait_for(lambda:a.js("document.activeElement.id")=='unlock','Tab focus')
            assert a.js("getComputedStyle(document.activeElement).outlineStyle") != 'none'
        finally:
            keyboard.terminate();keyboard.wait(timeout=5)
        passed('reload restores auth; keyboard Tab and visible focus work')

        subprocess.run(['swaymsg','create_output'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        a.ready(count=2)
        autoplay(a,output=1)
        assert {w['name'] for w in a.state()['windows']}=={'HEADLESS-1','HEADLESS-2'}
        state=a.state();assert all(w['width']>0 and w['height']>0 for w in state['windows'])
        assert a.js("JSON.parse(document.querySelector('#messages').textContent.split('\\n').find(x=>x.includes('\"type\":\"init\"'))).screenshot",output=1) is None
        passed('hot-plug output receives its own ready lock window with screenshot null')

        # WebKit API terminates the actual web process and emits the real termination signal.
        a.request('crash_web');a.ready(count=2)
        autoplay(a)
        assert a.state()['locked']
        passed('killed WebKit process reloads and lock remains held')
        subprocess.run(['swaymsg','output','HEADLESS-2','disable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        a.ready(count=1)
        subprocess.run(['swaymsg','output','HEADLESS-2','enable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        a.ready(count=2)
        passed('output removal drops its window; re-enabled output gets a new lock window')
        a.password('rhine-test',output=1)
        a.exited()
        passed('password on secondary output succeeds; unlock-ready releases lock; exit 0')

        # Starting a fresh lock proves compositor release independently of our process state.
        b=start('daemon','--daemonize')
        assert b.process.wait(timeout=25)==0
        assert b.state()['locked']
        b.ready(count=2)
        second=subprocess.run([str(BIN),'--mock-password','rhine-test','--daemonize'],timeout=5,capture_output=True)
        assert second.returncode==0
        passed('daemon parent returns only after lock; second instance exits 0')
        assert b.js("JSON.parse(document.querySelector('#messages').textContent.split('\\n').find(x=>x.includes('\"type\":\"init\"'))).screenshot",output=1)=='rhine://screenshot/HEADLESS-2'
        subprocess.run(['swaymsg','output','HEADLESS-2','disable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        b.ready(count=1)
        subprocess.run(['swaymsg','output','HEADLESS-2','enable'],check=True,stdout=subprocess.DEVNULL,timeout=5)
        b.ready(count=2)
        assert b.js("JSON.parse(document.querySelector('#messages').textContent.split('\\n').find(x=>x.includes('\"type\":\"init\"'))).screenshot",output=1) is None
        passed('reconnected output discards its old pre-lock screenshot')
        b.js('window.NO_UNLOCK_READY=true;true',output=0);b.js('window.NO_UNLOCK_READY=true;true',output=1)
        started=time.monotonic();b.password('rhine-test');b.auth('success');b.exited(daemon=True)
        assert time.monotonic()-started >= 1.59
        passed('1600 ms timeout unlocks when frontend never sends unlock-ready')

        c=start('fallback',frontend=ART/'does-not-exist')
        c.ready(count=2,fallback=True)
        shot('native-fallback')
        c.request('set_value',control='unlock-password',value='wrong')
        c.request('invoke_action',control='unlock-submit')
        assert c.auth('failure')['locked']
        c.request('set_value',control='unlock-password',value='rhine-test')
        c.request('invoke_action',control='unlock-submit')
        c.exited()
        passed('missing frontend native fallback renders; failure stays locked; success unlocks')

        d=start('crash-fallback');d.ready(count=2)
        for i in range(3):
            d.request('crash_web')
            if i<2: d.ready(count=2)
        wait_for(lambda:any(w['fallback'] for w in d.state()['windows']),'three crashes switch to fallback')
        state=d.state();idx=next(i for i,w in enumerate(state['windows']) if w['fallback'])
        d.request('set_value',control='unlock-password',value='rhine-test',output=idx)
        d.request('invoke_action',control='unlock-submit',output=idx)
        d.exited()
        passed('three web-process crashes in 30 s activate native fallback; still unlocks')

        p=start('preview','--preview');p.ready(locked=False)
        assert p.js("JSON.parse(document.querySelector('#messages').textContent.split('\\n').find(x=>x.includes('\"type\":\"init\"'))).mode")=='preview'
        p.request('close_window');assert p.process.wait(timeout=5)==0
        passed('preview is a normal closable window with no session lock')

        real=ROOT/'frontend/dist'
        if (real/'index.html').exists():
            r=start('real-frontend',frontend=real)
            settled=wait_for(lambda:(lambda s:s if len(s['windows'])==2 and all(w['ready'] or w['fallback'] for w in s['windows']) else None)(r.state()),'real frontend handshake or safe fallback',timeout=25)
            if all(w['ready'] for w in settled['windows']):
                assert r.js("!!document.querySelector('input[type=password]')")
                wait_for(lambda:r.js("(()=>{const b=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='REQUEST');if(!b)return false;for(let e=b;e;e=e.parentElement)if(+getComputedStyle(e).opacity<.99)return false;return b.getBoundingClientRect().width>0})()"),'visible real frontend controls',timeout=20)
                assert r.js("document.styleSheets.length>0 && [...document.styleSheets].every(s=>s.cssRules.length>0)")
                shot('real-frontend')
                r.password('rhine-test');r.exited()
                passed('real frontend smoke: hello, rendered screenshot, password input, success/unlock')
            else:
                shot('real-frontend')
                idx=next(i for i,w in enumerate(settled['windows']) if w['fallback'])
                r.request('set_value',control='unlock-password',value='rhine-test',output=idx)
                r.request('invoke_action',control='unlock-submit',output=idx);r.exited()
                raise AssertionError('Real frontend did not handshake; native fallback alone is not a frontend smoke pass')
        elif '--backend-only' in sys.argv:
            print('SKIP real frontend smoke: explicit --backend-only, dist absent',flush=True)
        else:
            raise AssertionError('Build frontend/dist first: (cd frontend && npm ci && npm run build), or use explicit --backend-only')
        passed('ALL END-TO-END CHECKS')
    except Exception:
        try: shot('failure')
        except Exception: pass
        traceback.print_exc()
        raise
    finally:
        for locker in lockers: locker.cleanup()


if __name__ == "__main__":
    from transitions import run_recordings
    from release_checks import run_release_checks
    failures=[]
    def phase(label, action):
        try:action()
        except Exception:
            traceback.print_exc()
            failures.append(label)
    if '--release-only' in sys.argv:
        phase('release regressions', lambda:run_release_checks(sys.modules[__name__]))
    elif '--regression-only' not in sys.argv:
        phase('single-output recording',lambda:run_recordings(sys.modules[__name__]))
    if '--transitions-only' not in sys.argv and '--release-only' not in sys.argv:
        phase('regression suite',run_checks)
        phase('release regressions',lambda:run_release_checks(sys.modules[__name__]))
        if '--regression-only' not in sys.argv:
            phase('multi-output recording',lambda:run_recordings(sys.modules[__name__],multi=True))
    if failures:
        print('FAIL '+', '.join(failures),flush=True)
        sys.exit(1)
