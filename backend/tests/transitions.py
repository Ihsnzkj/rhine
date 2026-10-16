"""Poll actual compositor frames throughout lock acquisition and WebKit startup."""
import hashlib
import io
import json
import os
import subprocess
import threading
import time
from PIL import Image, ImageDraw, ImageChops


def run_recordings(h, multi=False):
    folder = h.ART / 'transitions' / str(time.time_ns())
    folder.mkdir(parents=True)
    outputs=json.loads(subprocess.check_output(['swaymsg','-t','get_outputs']))
    outputs=sorted([o for o in outputs if o['active']],key=lambda o:o['name'])
    assert len(outputs)==(2 if multi else 1),outputs
    if multi:
        subprocess.run(['swaymsg','output','HEADLESS-2','scale','2'],check=True,stdout=subprocess.DEVNULL,timeout=5)
    width, height = 1920, 1080
    pattern = Image.new('RGB', (width, height), '#345678')
    draw = ImageDraw.Draw(pattern)
    for y in range(0, height, 90):
        for x in range(0, width, 120):
            draw.rectangle((x, y, x+59, y+44), fill=('#d18e32' if (x//120+y//90)%2 else '#7fb4c1'))
    draw.line((17, 29, 1701, 997), fill='#112233', width=1)
    pattern.save(folder/'desktop-fixture.png')
    stalled = folder/'stalled-path'
    stalled.mkdir(exist_ok=True)
    (stalled/'grim').write_text('#!/bin/sh\nexec /usr/bin/sleep 30\n')
    (stalled/'grim').chmod(0o700)
    partial=folder/'partial-path'
    partial.mkdir()
    (partial/'grim').write_text('#!/bin/sh\nif [ "$2" = HEADLESS-2 ]; then exec /usr/bin/sleep 30; fi\nexec /usr/bin/grim "$@"\n')
    (partial/'grim').chmod(0o700)
    foreign_cases=[]
    patterns=[pattern]
    if multi:
        second=Image.new('RGB',(outputs[1]['current_mode']['width'],outputs[1]['current_mode']['height']),'#713a58')
        ImageDraw.Draw(second).rectangle((37,49,711,331),fill='#cabd42')
        second.save(folder/'desktop-second.png')
        patterns.append(second)
    images=[str(folder/'desktop-fixture.png')]+([str(folder/'desktop-second.png')] if multi else [])
    desktop = subprocess.Popen(['python3', str(h.ROOT/'backend/tests/desktop_fixture.py'), *images], stdout=subprocess.DEVNULL, stderr=open(folder/'desktop.log','w'))

    def frame(output="HEADLESS-1"):
        png = subprocess.check_output(['/usr/bin/grim', '-o', output, '-t', 'ppm', '-'], timeout=3)
        return Image.open(io.BytesIO(png)).convert('RGB')

    def equal(a, b):
        return a.size == b.size and ImageChops.difference(a, b).getbbox() is None

    page = Image.new('RGB', (width, height), '#e7e5e1')
    ImageDraw.Draw(page).rectangle((0,0,width//4-1,height-1),fill='#b9e432')
    paper = Image.new('RGB', (width,height), '#e7e5e1')
    try:
        for i,o in enumerate(outputs):
            h.wait_for(lambda:equal(frame(o['name']),patterns[i]),'synthetic desktop pixel match '+o['name'])
        for name, args, env, screenshot in [
            ('entry', [], {}, True),
            ('entry-daemon', ['--daemonize'], {}, True),
            ('entry-no-grim', [], {'PATH':str(folder/'empty-path')}, False),
            ('entry-timeout', [], {'PATH':str(stalled)}, False),
        ]+([('entry-partial', [], {'PATH':str(partial)}, True)] if multi else []):
            only=next((v.split('=',1)[1] for v in h.sys.argv if v.startswith('--case=')),None)
            if only and name!=only:continue
            if os.environ.get('RHINE_TRACE_WAYLAND'):env=dict(env,WAYLAND_DEBUG='client')
            if multi:name+='-multi'
            h.wait_for(lambda:equal(frame(),pattern),'desktop restored before '+name)
            frames, errors = [], []
            stop = threading.Event()
            def record():
                while not stop.is_set():
                    try:
                        before = time.monotonic()
                        im = frame()
                        frames.append((before, time.monotonic(), im))
                    except Exception as e:
                        errors.append(str(e));return
            thread = threading.Thread(target=record)
            thread.start()
            locker = None
            unlocked = False
            try:
                h.wait_for(lambda:len(frames)>=2,'recorder ready')
                started = time.monotonic()
                locker = h.start(name,*args,frontend=h.ROOT/'backend/testdata/transition-frontend',extra_env=env)
                locker.ready(count=len(outputs))
                h.wait_for(lambda:locker.js('window.fixtureError || window.pagePainted') is True,'fixture screenshot decode/page paint')
                if multi:
                    h.wait_for(lambda:locker.js('window.fixtureError || window.pagePainted',output=1) is True,'second output page paint')
                    second_init=locker.js('window.fixtureInit',output=1)
                    assert second_init['output']['scale']==2,second_init
                    if screenshot and not name.startswith('entry-partial'):
                        assert second_init['screenshot']=='rhine://screenshot/HEADLESS-2',second_init
                        assert locker.js('window.screenshotHash',output=1)==hashlib.sha256(patterns[1].convert('RGBA').tobytes()).hexdigest()
                        assert equal(frame('HEADLESS-2'),patterns[1])
                    else:assert second_init['screenshot'] is None,second_init
                painted = time.monotonic()
                init = locker.js('window.fixtureInit')
                assert locker.js('window.missingStatus')==404
                if screenshot:
                    assert init['screenshot']=='rhine://screenshot/HEADLESS-1', init
                    assert locker.js('window.pngSignature')==[137,80,78,71,13,10,26,10]
                    assert locker.js('window.pngMime')=='image/png'
                    assert locker.js('window.screenshotHash')==hashlib.sha256(pattern.convert('RGBA').tobytes()).hexdigest()
                else:
                    assert init['screenshot'] is None,init
                # Keep the transparent/image first page in place until several real
                # compositor captures observe it, then paint a known opaque page.
                n = len(frames)
                h.wait_for(lambda:len(frames)>=n+3,'first page samples')
                locker.js('window.showPage()')
                h.wait_for(lambda:len(frames)>n+3 and equal(frames[-1][2],page),'opaque page captured')
                finished = time.monotonic()
                stop.set();thread.join(timeout=4)
                assert not thread.is_alive() and not errors,errors
                allowed = [pattern,page] if screenshot else [pattern,paper,page]
                foreign=[]
                for i,(before,after,im) in enumerate(frames):
                    im.save(folder/f'{name}-{i:04}.png')
                    if not any(equal(im,target) for target in allowed):foreign.append(i)
                # Sway (the headless test compositor) paints its own black lock
                # background the instant it receives the lock request, ~10 ms
                # before our first buffer (sway/lock.c).  niri — the target
                # compositor — instead keeps rendering the session in
                # LockState::WaitingForSurfaces until every lock surface has
                # committed (niri.rs), so this frame cannot occur there.
                # Tolerate exactly one such frame, uniformly black, sitting
                # directly on the desktop→lock boundary; anything else fails.
                sway_precommit=[]
                if len(foreign)==1:
                    i=foreign[0]; im=frames[i][2]
                    black=im.convert('L').getextrema()==(0,0)
                    after_desktop=all(equal(frames[j][2],pattern) for j in range(i))
                    # with a screenshot our first lock frame *is* the desktop, so the
                    # boundary is located by time: after the lock request, before the
                    # page painted, with only allowed frames on either side
                    in_window=started<=frames[i][0]<=painted
                    then_allowed=i+1<len(frames) and any(equal(frames[i+1][2],t) for t in allowed)
                    if black and after_desktop and in_window and then_allowed:
                        sway_precommit=foreign; foreign=[]
                        print(f'XFAIL {name}: sway pre-commit black frame {sway_precommit} (compositor-owned, not reproducible on niri)',flush=True)
                metadata={'frames':len(frames),'started':started,'painted':painted,'finished':finished,
                          'samples':[[b,a] for b,a,_ in frames],'foreign':foreign,'swayPrecommit':sway_precommit,
                          'maxSampleMs':max((a-b)*1000 for b,a,_ in frames)}
                (folder/f'{name}.json').write_text(json.dumps(metadata,indent=2))
                if foreign:
                    foreign_cases.append(name)
                    print(f'FAIL {name}: foreign frames {foreign}; see {folder}',flush=True)
                assert any(equal(im,pattern if screenshot else paper) and b>started for b,_,im in frames)
                if not screenshot and not foreign:
                    assert equal(next(im for idx,(b,_,im) in enumerate(frames)
                                      if b>started and idx not in sway_precommit and not equal(im,pattern)),paper)
                if args:assert locker.process.wait(timeout=5)==0 and locker.state()['locked']
                locker.js("webkit.messageHandlers.rhine.postMessage({type:'authenticate',password:'rhine-test'});true")
                locker.exited(daemon=bool(args))
                unlocked = True
                log=(h.ART/f'{name}.log').read_text()
                capture=next(line for line in log.splitlines() if line.startswith('Pre-lock capture:'))
                capture_ms=int(capture.split()[-2])
                assert capture_ms<=800,capture
                if name.startswith(('entry-timeout','entry-partial')):assert capture_ms>=750,capture
                assert not any(s in log for s in ['Fatal backend panic','Bridge delivery failed','Page load failed','frontend [error]'])
                if not foreign:
                    h.passed(f'{name}: {len(frames)} recorded frames, zero foreign frames; PNG/null verified; {capture}')
                else:
                    h.passed(f'{name}: PNG/null, safe unlock, capture budget verified; {capture}; RECORDING FAILED')
            finally:
                stop.set();thread.join(timeout=4)
                if errors or not (folder/f'{name}.json').exists():
                    for i,(_,_,im) in enumerate(frames):im.save(folder/f'{name}-diagnostic-{i:04}.png')
                if locker:
                    if not unlocked:
                        try:
                            locker.js("webkit.messageHandlers.rhine.postMessage({type:'authenticate',password:'rhine-test'});true")
                            locker.exited(daemon=bool(args))
                        except Exception:pass
                    locker.cleanup();h.lockers.remove(locker)
                    for ext in ['log','jsonl']:
                        (folder/f'{name}.{ext}').write_bytes((h.ART/f'{name}.{ext}').read_bytes())
    finally:
        desktop.terminate();desktop.wait(timeout=5)
        if multi:
            subprocess.run(['swaymsg','output','HEADLESS-2','scale','1'],check=True,stdout=subprocess.DEVNULL,timeout=5)
    assert not foreign_cases,f'Foreign compositor frames: {foreign_cases}; see {folder}'
