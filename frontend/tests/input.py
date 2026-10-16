#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Input-path regressions of the real frontend (frontend/dist) in the real
GTK/WebKit lock: mock-auth backend, private headless compositor, real keys
through wtype.  Run with frontend/tests/input.sh."""
import os, pathlib, subprocess, sys, time

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "backend/tests"))
import e2e as h  # noqa: E402  (the backend's harness: Locker, wait_for, shot, CONFIG)

# optional: another mock-auth binary / artifact directory (e.g. beside a running e2e)
if os.environ.get("RHINE_TEST_BIN"):
    h.BIN = pathlib.Path(os.environ["RHINE_TEST_BIN"])
if os.environ.get("RHINE_TEST_ART"):
    h.ART = pathlib.Path(os.environ["RHINE_TEST_ART"])
    h.CONFIG = h.ART / "config/rhine-lock"
    h.CONFIG.mkdir(parents=True, exist_ok=True)
DIST = ROOT / "frontend/dist"
results = []


def check(name, ok, detail=""):
    results.append((name, bool(ok)))
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail != "" else ""), flush=True)


def wtype(*args):
    # the first key of every wtype call is lost: a sacrificial F13 in the same call
    subprocess.run(["wtype", "-k", "F13", *args], check=True, timeout=10)


def config(text):
    (h.CONFIG / "config.toml").write_text(text)


def settled(a):
    h.wait_for(lambda: a.js('!!window.__lock && __lock.phase==="ready" && __lock.uiReveal===1'),
               "ready and entry finished", timeout=15)


def field(a):
    return a.js("({phase:__lock.phase,len:__lock.passwordLength,"
                "v:document.querySelector('input[type=password]').value.length})")


# power messages are recorded and never forwarded; authenticate is recorded by length only.
# The handler wrapper must stay referenced: WebKit may collect and recreate it,
# and a recreated wrapper has lost the patched postMessage.
INTERCEPT = """{window.sent=[];const mh=window.__handler=webkit.messageHandlers.rhine,orig=mh.postMessage.bind(mh);
mh.postMessage=m=>{if(m.type==="power"){sent.push(m);return;}
if(m.type==="authenticate")sent.push({type:"authenticate",len:m.password.length});orig(m)};}true"""
STANDBY = '{__lock.setPhase("standby");}true'
CLEAR = '{__lock.clearPassword(false);document.querySelector("input[type=password]").value="";}true'


def main():
    config('[lock]\nallow_power=false\n[frontend]\nquality="low"\nmusic_volume=5\nsfx_volume=-3\nstandby_seconds=0\n')
    a = h.start("input-main", frontend=DIST)
    try:
        a.ready()
        settled(a)
        check("[frontend] quality reaches the renderer", a.js("__stage && __stage.quality") == "low")
        check("the entry finishes every reveal", a.js("__lock.uiReveal") == 1)
        a.js(INTERCEPT)

        a.js('{dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",isComposing:true,bubbles:true}));}true')
        time.sleep(0.3)
        check("Enter while an IME composes does not submit", a.js("sent.length") == 0)
        wtype("-k", "Return"); time.sleep(0.4)
        check("an empty key is never sent", a.js("sent.length") == 0 and a.state()["auth"]["state"] == "idle")

        a.js(STANDBY)
        a.js('{const e=document.querySelector("input[type=password]");e.value="ime-commit";'
             'e.dispatchEvent(new InputEvent("input",{inputType:"insertCompositionText",data:"ime-commit",isComposing:true,bubbles:true}));}true')
        time.sleep(0.1)
        st = field(a)
        check("input in standby only wakes", st == {"phase": "ready", "len": 0, "v": 0}, st)
        a.js(STANDBY)
        a.js('{const e=document.querySelector("input[type=password]");'
             'e.dispatchEvent(new InputEvent("beforeinput",{inputType:"insertText",data:"wake",cancelable:false,bubbles:true}));'
             'e.value="wake";e.dispatchEvent(new InputEvent("input",{inputType:"insertText",data:"wake",bubbles:true}));}true')
        st = field(a)
        check("non-cancelable beforeinput only wakes", st == {"phase": "ready", "len": 0, "v": 0}, st)
        wtype("z")
        check("the next intentional edit survives a non-cancelable wake", a.js("__lock.passwordLength") == 1)
        a.js(CLEAR)
        a.js(STANDBY)
        a.js('{const e=document.querySelector("input[type=password]");'
             'e.dispatchEvent(new CompositionEvent("compositionstart",{bubbles:true}));e.value="ni";'
             'e.dispatchEvent(new InputEvent("input",{inputType:"insertCompositionText",data:"ni",isComposing:true,bubbles:true}));'
             'e.value="你";e.dispatchEvent(new CompositionEvent("compositionend",{data:"你",bubbles:true}));'
             'e.dispatchEvent(new InputEvent("input",{inputType:"insertFromComposition",data:"你",bubbles:true}));}true')
        time.sleep(0.2)
        st = field(a)
        check("a composition begun in standby is dropped", st == {"phase": "ready", "len": 0, "v": 0}, st)

        a.js(STANDBY)
        wtype("-P", "a", "-s", "1400", "-p", "a"); time.sleep(0.2)
        st = field(a)
        check("a held waking key types nothing, not even repeats", st == {"phase": "ready", "len": 0, "v": 0}, st)
        wtype("bc"); time.sleep(0.2)
        check("typing after the wake reaches the field", a.js("__lock.passwordLength") == 2)

        a.js('{const e=document.querySelector("input[type=password]");e.value="x".repeat(5000);'
             'e.dispatchEvent(new InputEvent("input",{inputType:"insertFromPaste",bubbles:true}));}true')
        time.sleep(0.1)
        st = a.js("({len:__lock.passwordLength,msg:__lock.message})")
        check("a key over 4096 bytes is refused whole, with a message", st["len"] == 2 and "TOO LONG" in st["msg"], st)
        a.js('{const e=document.querySelector("input[type=password]");e.value="y".repeat(3000);'
             'e.dispatchEvent(new InputEvent("input",{inputType:"insertFromPaste",bubbles:true}));}true')
        time.sleep(0.3)
        st = a.js("({len:__lock.passwordLength,cells:document.querySelectorAll('.cell').length})")
        check("a 3000-character key is accepted, only visible cells drawn", st["len"] == 3000 and st["cells"] < 60, st)
        a.js('{const e=document.querySelector("input[type=password]");e.value="\U0001F511a";'
             'e.dispatchEvent(new InputEvent("input",{bubbles:true}));}true')
        check("a non-BMP character counts once", a.js("__lock.passwordLength") == 2)
        a.js(CLEAR)

        found = None
        for _ in range(12):
            wtype("-k", "Tab"); time.sleep(0.15)
            tag = a.js("(()=>{const e=document.activeElement;return e.tagName+':'+(e.getAttribute('aria-label')||e.textContent.trim())})()")
            if tag.startswith("BUTTON:toggle audio"):
                found = tag
                break
        check("Tab reaches a control and focus stays there", found is not None, found or "")
        muted = a.js("__lock.muted")
        wtype("-k", "space"); time.sleep(0.25)
        check("Space presses the focused control", a.js("__lock.muted") != muted and a.js("__lock.passwordLength") == 0)
        check("a focused button has a visible outline", a.js('getComputedStyle(document.activeElement).outlineStyle') != 'none')
        wtype("q"); time.sleep(0.2)
        check("typing on a control goes to the key field",
              a.js("document.activeElement.type") == "password" and a.js("__lock.passwordLength") == 1)

        wtype("escape-check", "-k", "Escape")
        check("plain Escape clears the key without native fallback", field(a)["len"] == 0 and not a.state()["windows"][0]["fallback"])
        wtype("shift-check", "-M", "shift", "-k", "Escape", "-m", "shift")
        check("Shift+Escape clears the key without native fallback", field(a)["len"] == 0 and not a.state()["windows"][0]["fallback"])
        a.js('{__lock.init.capabilities.powerOff=true;__lock.arm("powerOff");'
             'document.querySelector("button[aria-label=\\"toggle audio\\"]").focus();}true')
        wtype("-k", "Escape")
        check("Escape on a focused control cancels power and returns to the key",
              a.js('__lock.armed==="" && document.activeElement.type==="password"'))

        a.js("{__lock.init.capabilities.powerOff=true;}true")
        wtype("-P", "F9", "-s", "1500", "-p", "F9"); time.sleep(0.2)
        st = a.js("({armed:__lock.armed,power:sent.filter(m=>m.type==='power').length})")
        check("a held F9 arms but never confirms", st == {"armed": "powerOff", "power": 0}, st)
        wtype("-k", "F9"); time.sleep(0.2)
        st = a.js("({armed:__lock.armed,power:sent.filter(m=>m.type==='power').length})")
        check("a second, distinct press confirms (intercepted)", st == {"armed": "", "power": 1}, st)

        a.js(CLEAR)
        a.js("{sent.length=0;}true")
        wtype("wrong"); time.sleep(0.2)
        wtype("-P", "Return", "-s", "2500", "-p", "Return"); time.sleep(0.3)
        n = a.js("sent.filter(m=>m.type==='authenticate').length")
        check("a held Enter submits once", n == 1, n)
        a.auth("failure")

        a.js(STANDBY); time.sleep(3.5)
        f0 = a.js("__stage.stats.frames"); time.sleep(2.0); f1 = a.js("__stage.stats.frames")
        check("a settled standby renders at most ~30 fps", (f1 - f0) / 2.0 <= 33, f"{(f1 - f0) / 2.0:.1f} fps")
        a.js("{__lock.activity();}true"); time.sleep(1.0)
        f0 = a.js("__stage.stats.frames"); time.sleep(1.0); f1 = a.js("__stage.stats.frames")
        check("full frame rate after waking", f1 - f0 > 40, f"{f1 - f0} fps")

        h.shot("input-ready-r2")

        a.password("rhine-test")
        a.exited()
        check("the right key unlocks", True)
    finally:
        a.cleanup()

    config('[lock]\nallow_power=false\n[frontend]\nquality="potato"\n')
    b = h.start("input-badquality", frontend=DIST)
    try:
        b.ready()
        settled(b)
        check("an invalid quality falls back to ultra", b.js("__stage && __stage.quality") == "ultra")
        h.wait_for(lambda: b.js('!!__stage.environmentTarget'), "environment target loaded")
        b.js('{window.environmentDisposals=0;__stage.environmentTarget.addEventListener("dispose",()=>environmentDisposals++);'
             '__stage.dispose();__stage.dispose();}true')
        check("scene disposal releases the environment target exactly once", b.js('environmentDisposals===1 && __stage.environmentTarget===null'))
        b.password("rhine-test")
        b.exited()
    finally:
        b.cleanup()

    config('[lock]\nallow_power=false\n[frontend]\nquality="low"\nstandby_seconds=0\n')
    c = h.start("input-media-wait", frontend=DIST, extra_env={"RHINE_TEST_MEDIA_DELAY_MS": "10000"})
    try:
        c.ready()
        settled(c)
        c.js('''{window.musicRequests=0;const fetchOriginal=window.fetch.bind(window);
            window.fetch=(...args)=>{if(String(args[0]).includes("bgm-loop"))musicRequests++;return fetchOriginal(...args)};
            __lock.activity();}true''')
        time.sleep(1.7)  # observation beyond the removed 1.5 s music grace
        check("music waits for the first real media snapshot", c.js('!__lock.mediaKnown && musicRequests===0'))
        c.js('''{__rhine.receive({type:"media",player:{name:"Fixture",title:"Playing",artist:"",album:"",playing:true,
            artUrl:null,canNext:false,canPrevious:false,canPlayPause:false}});__lock.activity();}true''')
        check("a playing snapshot keeps the score silent", c.js('musicRequests===0 && __lock.musicPending===0'))
        c.js('''{__rhine.receive({...__lock.init,output:{...__lock.init.output,primary:false}});
            __rhine.receive({type:"media",player:null});__lock.toggleMute();__lock.toggleMute();__lock.activity();}true''')
        check("F4 cannot start music on a secondary output", c.js('musicRequests===0'))
        c.js('''{__rhine.receive({...__lock.init,output:{...__lock.init.output,primary:true}});__lock.activity();}true''')
        check("promotion to primary permits music on activity", c.js('musicRequests===1'))

        c.js('''{window.gains=[];const d=Object.getOwnPropertyDescriptor(AudioParam.prototype,"value");
            Object.defineProperty(AudioParam.prototype,"value",{...d,set(v){gains.push(v);d.set.call(this,v)}});
            const ramp=AudioParam.prototype.linearRampToValueAtTime;
            AudioParam.prototype.linearRampToValueAtTime=function(v,t){gains.push(v);return ramp.call(this,v,t)};
            for(const v of [-3,0,0.5,5,NaN,Infinity,"invalid"])
                __rhine.receive({...__lock.init,config:{...__lock.init.config,sfx_volume:v,music_volume:v}});}true''')
        check("initial and repeated volume updates keep every gain finite and within range",
              c.js('gains.length>10 && gains.every(v=>Number.isFinite(v)&&v>=0&&v<=1)'))
        c.password("rhine-test")
        c.exited()
    finally:
        c.cleanup()

    bad = [n for n, ok in results if not ok]
    print(f"\n{len(results) - len(bad)}/{len(results)} passed" + (": FAILED " + "; ".join(bad) if bad else ""), flush=True)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
