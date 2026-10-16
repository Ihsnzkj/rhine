// The ∞ fin sculpture — Three.js port of the SDDM theme's Quick3D scene
// (FinScene.qml).  Same units, camera, lights and shaders; one instanced draw
// call per mesh, all motion in the vertex shader.
import * as THREE from "three";
import { HDRLoader } from "three/examples/jsm/loaders/HDRLoader.js";
import {
  BlendFunction, DepthOfFieldEffect, Effect, EffectAttribute, EffectComposer,
  EffectPass, RenderPass,
} from "postprocessing";
import { N8AOPostPass } from "n8ao";
import {
  cardCarveGlsl, cardUniformsGlsl, cardVertexGlsl, common, finCarveGlsl, finEmissiveGlsl,
  finSurfaceGlsl, finUniformsGlsl, finVertexGlsl, gradeFragment,
} from "./shaders";

export type Quality = "ultra" | "high" | "low";

// look-dev overrides: ?exp=1.3&key=3&env=0.8
const P = new URLSearchParams(location.search);
const num = (k: string, d: number) => (P.has(k) ? Number(P.get(k)) : d);
const LOOK = { exposure: num("exp", 1.5), key: num("key", 3.4), fill: num("fill", 0.55), env: num("env", 0.8), tm: num("tm", 0), sat: num("sat", 0.7) };

const FIN_COUNT = 400;
const CARD_COUNT = 30;
const DEG = Math.PI / 180;

export interface StageTargets {
  verify: number;      // 0..1
  denied: number;      // 0..1  (graphite / orange lockdown palette)
  standby: number;     // 0..1
  parallaxX: number;   // -1..1
  parallaxY: number;
  cardsReveal: number; // 0..1
  glitch: number;      // 0..1 (post)
}

class GradeEffect extends Effect {
  constructor() {
    super("RhineGrade", gradeFragment, {
      blendFunction: BlendFunction.SET,
      attributes: EffectAttribute.CONVOLUTION,
      uniforms: new Map<string, THREE.Uniform>([
        ["uTime", new THREE.Uniform(0)],
        ["uGlitch", new THREE.Uniform(0)],
        ["uAberration", new THREE.Uniform(0.0016)],
        ["uVignette", new THREE.Uniform(0.32)],
        ["uDenied", new THREE.Uniform(0)],
        ["uStandby", new THREE.Uniform(0)],
        ["uExposure", new THREE.Uniform(1.0)],
        ["uCentre", new THREE.Uniform(new THREE.Vector2(0.45, 0.47))],
        ["uPaperBase", new THREE.Uniform(new THREE.Vector3(0.9, 0.9, 0.9))],
        ["uTonemap", new THREE.Uniform(LOOK.tm)],
        ["uSat", new THREE.Uniform(LOOK.sat)],
      ]),
    });
  }
  set(name: string, v: number) { this.uniforms.get(name)!.value = v; }
}

// Khronos PBR Neutral, mirrored from the grade shader, and its inverse — used
// to pick a clear colour that lands exactly on the paper after tone mapping.
function neutral(c: number[]) {
  const sc = 0.76, desat = 0.15;
  const x = Math.min(...c), off = x < 0.08 ? x - 6.25 * x * x : 0.04;
  let v = c.map((k) => k - off);
  const peak = Math.max(...v);
  if (peak < sc) return v;
  const d = 1 - sc, np = 1 - (d * d) / (peak + d - sc);
  v = v.map((k) => (k * np) / peak);
  const g = 1 - 1 / (desat * (peak - np) + 1);
  return v.map((k) => k + (np - k) * g);
}
function aces(c: number[]) {
  const i = [[0.59719, 0.35458, 0.04823], [0.07600, 0.90834, 0.01566], [0.02840, 0.13383, 0.83777]];
  const o = [[1.60475, -0.53108, -0.07367], [-0.10208, 1.10813, -0.00605], [-0.00327, -0.07276, 1.07602]];
  const mul = (m: number[][], v: number[]) => m.map((r) => r[0] * v[0] + r[1] * v[1] + r[2] * v[2]);
  const v = mul(i, c).map((x) => (x * (x + 0.0245786) - 0.000090537) / (x * (0.983729 * x + 0.432951) + 0.238081));
  return mul(o, v).map((x) => Math.min(1, Math.max(0, x)));
}
// the grade's object path: exposure → tone curve → saturation (linear in, linear out)
function forward(c: number[]) {
  const e = c.map((x) => x * LOOK.exposure);
  const t = LOOK.tm > 0.5 ? aces(e) : neutral(e);
  const l = 0.2126 * t[0] + 0.7152 * t[1] + 0.0722 * t[2];
  return t.map((x) => l + (x - l) * LOOK.sat);
}
function inverseForward(target: number[]) {
  let x = target.slice();
  for (let i = 0; i < 40; ++i) {
    const y = forward(x);
    x = x.map((k, j) => Math.max(0, k + (target[j] - y[j]) / LOOK.exposure));
  }
  return x;
}
const srgbToLin = (c: number) => (c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4));
const PAPER_LIGHT = [0.925, 0.918, 0.902];   // the paper's centre colour (display space)
const PAPER_DARK = [0.043, 0.043, 0.047];

function instanced(base: THREE.BufferGeometry, count: number) {
  const g = new THREE.InstancedBufferGeometry();
  g.index = base.index;
  g.setAttribute("position", base.getAttribute("position"));
  g.setAttribute("normal", base.getAttribute("normal"));
  g.setAttribute("uv", base.getAttribute("uv"));
  const idx = new Float32Array(count);
  for (let i = 0; i < count; ++i) idx[i] = i;
  g.setAttribute("aIndex", new THREE.InstancedBufferAttribute(idx, 1));
  g.instanceCount = count;
  return g;
}

export class Stage {
  readonly renderer: THREE.WebGLRenderer;
  readonly scene = new THREE.Scene();
  readonly camera: THREE.PerspectiveCamera;
  readonly sculpture = new THREE.Group();
  private composer: EffectComposer;
  private grade = new GradeEffect();
  private dof: DepthOfFieldEffect | null = null;
  private finU: Record<string, THREE.IUniform>;
  private cardU: Record<string, THREE.IUniform>;
  private cards: THREE.Mesh;
  private textures: THREE.Texture[] = [];
  private environmentTarget: THREE.WebGLRenderTarget | null = null;
  private clock = new THREE.Clock();
  private raf = 0;
  private disposed = false;
  private lastBase = -1;
  private lastRender = 0;
  /** frame-rate cap, 0 = every display frame (App lowers it in standby) */
  maxFps = 0;

  // animated state
  time = 0;
  private flow = 0;
  private spin = 0;
  private cardFlow = 0;
  private disorder = 0;
  private dolly = 0;
  private assemble = 1;
  private assembleAnim: { from: number; to: number; t0: number; dur: number } | null = null;
  private dissolve = 0;
  private dissolveAnim: { t0: number; dur: number } | null = null;
  private pulseT = new Float32Array(16).fill(-99);
  private pulseO = new Float32Array(16).fill(0.25);
  private pulseSlot = 0;
  sculptureOffset = -10;

  targets: StageTargets = { verify: 0, denied: 0, standby: 0, parallaxX: 0, parallaxY: 0, cardsReveal: 0, glitch: 0 };
  onFrame: ((dt: number) => void) | null = null;
  stats = { frames: 0, slow: 0, ms: 0 };

  constructor(private canvas: HTMLCanvasElement, readonly quality: Quality) {
    this.renderer = new THREE.WebGLRenderer({
      canvas, antialias: false, alpha: false, stencil: false, depth: true,
      // keep the clear colour un-premultiplied: rgb = paper base, alpha = 0 marks background
      premultipliedAlpha: false,
      powerPreference: "high-performance",
    });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.toneMapping = THREE.NoToneMapping;
    // Linear output: the grade pass writes display-space values itself.  With
    // an sRGB output space three would also "encode" the clear colour (see
    // WebGLBackground.setClear), lifting the WARNING black to grey.
    this.renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    this.renderer.shadowMap.enabled = quality !== "low";
    this.renderer.shadowMap.type = THREE.PCFShadowMap;

    this.camera = new THREE.PerspectiveCamera(30, 16 / 9, 20, 6000);
    this.camera.rotation.order = "YXZ";

    this.lights();
    this.environment();

    // ── fins ──
    const finGeo = instanced(new THREE.BoxGeometry(1, 1, 1), FIN_COUNT);
    this.finU = {
      uTime: { value: 0 }, uFlow: { value: 0 }, uCount: { value: FIN_COUNT },
      uRadius: { value: 285 }, uAspect: { value: 0.95 }, uDepth: { value: 95 }, uTwist: { value: 1.5 },
      uSpin: { value: 0 }, uThickness: { value: 4.2 }, uLean: { value: 0.62 }, uCorner: { value: 0.14 },
      uWidth: { value: 84 }, uHeight: { value: 84 }, uAssemble: { value: 1 }, uVerify: { value: 0 },
      uDenied: { value: 0 }, uAlign: { value: 0 }, uDisorder: { value: 0 }, uDissolve: { value: 0 },
      uPulseT: { value: this.pulseT }, uPulseO: { value: this.pulseO },
    };
    const finMat = new THREE.MeshStandardMaterial({ metalness: 0, roughness: 0.5, side: THREE.DoubleSide });
    finMat.onBeforeCompile = (sh) => {
      Object.assign(sh.uniforms, this.finU);
      sh.vertexShader = sh.vertexShader
        .replace("#include <common>", `#include <common>\n${common}\n${finUniformsGlsl}\n${finVertexGlsl}`)
        .replace("#include <beginnormal_vertex>",
          `vec3 rlPos; vec3 rlNrm; rl_finTransform(position, normal, rlPos, rlNrm);\nvec3 objectNormal = rlNrm;`)
        .replace("#include <begin_vertex>", "vec3 transformed = rlPos;");
      sh.fragmentShader = sh.fragmentShader
        .replace("#include <common>", `#include <common>\n${common}\n${finUniformsGlsl}`)
        .replace("#include <clipping_planes_fragment>", `#include <clipping_planes_fragment>\n${finCarveGlsl}\n${finSurfaceGlsl}`)
        .replace("#include <color_fragment>", "diffuseColor.rgb = rlBody;")
        .replace("#include <roughnessmap_fragment>", "float roughnessFactor = mix(0.55, 0.36, bevel);")
        .replace("#include <emissivemap_fragment>", finEmissiveGlsl);
    };
    const finDepth = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
    finDepth.onBeforeCompile = (sh) => {
      Object.assign(sh.uniforms, this.finU);
      sh.vertexShader = sh.vertexShader
        .replace("#include <common>", `#include <common>\n${common}\n${finUniformsGlsl}\n${finVertexGlsl}`)
        .replace("#include <begin_vertex>",
          `vec3 rlPos; vec3 rlNrm; rl_finTransform(position, normal, rlPos, rlNrm);\nvec3 transformed = rlPos;`);
      sh.fragmentShader = sh.fragmentShader
        .replace("#include <common>", `#include <common>\n${finUniformsGlsl}`)
        .replace("#include <clipping_planes_fragment>", `#include <clipping_planes_fragment>\n${finCarveGlsl}`);
    };
    const fins = new THREE.Mesh(finGeo, finMat);
    fins.customDepthMaterial = finDepth;
    fins.frustumCulled = false;
    fins.castShadow = fins.receiveShadow = true;
    this.sculpture.add(fins);
    this.scene.add(this.sculpture);

    // ── frosted ∞ card stream (left column) ──
    const cardTex = new THREE.TextureLoader().load("./scene/card.png");
    this.textures.push(cardTex);
    cardTex.colorSpace = THREE.SRGBColorSpace;
    cardTex.anisotropy = 8;
    this.cardU = {
      uTime: { value: 0 }, uFlow: { value: 0 }, uCount: { value: CARD_COUNT }, uWidth: { value: 104 },
      uThickness: { value: 3.2 }, uReveal: { value: 0 }, uDenied: { value: 0 }, uCardTex: { value: cardTex },
    };
    const cardMat = new THREE.MeshStandardMaterial({ metalness: 0, roughness: 0.42, side: THREE.DoubleSide });
    cardMat.onBeforeCompile = (sh) => {
      Object.assign(sh.uniforms, this.cardU);
      sh.vertexShader = sh.vertexShader
        .replace("#include <common>", `#include <common>\n${common}\n${cardUniformsGlsl}\n${cardVertexGlsl}`)
        .replace("#include <beginnormal_vertex>",
          `vec3 rlPos; vec3 rlNrm; rl_cardTransform(position, normal, rlPos, rlNrm);\nvec3 objectNormal = rlNrm;`)
        .replace("#include <begin_vertex>", "vec3 transformed = rlPos;");
      sh.fragmentShader = sh.fragmentShader
        .replace("#include <common>", `#include <common>\n${common}\n${cardUniformsGlsl}`)
        .replace("#include <clipping_planes_fragment>", `#include <clipping_planes_fragment>\n${cardCarveGlsl}
          bool rlFace = abs(vLocal.z) > 0.49;`)
        .replace("#include <color_fragment>", `{
          vec3 c = texture2D(uCardTex, rlFace ? vCardUv : vec2(0.5)).rgb;
          vec3 graphite = rl_toLinear(vec3(0.07, 0.072, 0.078));
          diffuseColor.rgb = mix(c, graphite + (c - rl_toLinear(vec3(0.94))) * 0.25, uDenied);
        }`)
        .replace("#include <roughnessmap_fragment>", "float roughnessFactor = rlFace ? 0.42 : 0.25;")
        .replace("#include <emissivemap_fragment>",
          "totalEmissiveRadiance = (rlFace ? 0.0 : 1.0) * mix(vec3(0.06), rl_toLinear(vec3(1.0, 0.48, 0.0)) * 0.9, uDenied);");
    };
    const cardDepth = new THREE.MeshDepthMaterial({ depthPacking: THREE.RGBADepthPacking });
    cardDepth.onBeforeCompile = (sh) => {
      Object.assign(sh.uniforms, this.cardU);
      sh.vertexShader = sh.vertexShader
        .replace("#include <common>", `#include <common>\n${common}\n${cardUniformsGlsl}\n${cardVertexGlsl}`)
        .replace("#include <begin_vertex>",
          `vec3 rlPos; vec3 rlNrm; rl_cardTransform(position, normal, rlPos, rlNrm);\nvec3 transformed = rlPos;`);
      sh.fragmentShader = sh.fragmentShader
        .replace("#include <common>", `#include <common>\n${cardUniformsGlsl}`)
        .replace("#include <clipping_planes_fragment>", `#include <clipping_planes_fragment>\n${cardCarveGlsl}`);
    };
    this.cards = new THREE.Mesh(instanced(new THREE.BoxGeometry(1, 1, 1), CARD_COUNT), cardMat);
    this.cards.customDepthMaterial = cardDepth;
    this.cards.frustumCulled = false;
    this.cards.castShadow = this.cards.receiveShadow = true;
    this.scene.add(this.cards);

    // ── post ──
    const samples = quality === "ultra" ? 8 : quality === "high" ? 4 : 0;
    this.composer = new EffectComposer(this.renderer, { frameBufferType: THREE.HalfFloatType, multisampling: samples });
    this.composer.addPass(new RenderPass(this.scene, this.camera));
    if (quality !== "low") {
      const ao = new N8AOPostPass(this.scene, this.camera, 1920, 1080);
      ao.configuration.aoRadius = 26;
      ao.configuration.distanceFalloff = 1.2;
      ao.configuration.intensity = quality === "ultra" ? 2.6 : 2.2;
      ao.configuration.gammaCorrection = false;
      ao.configuration.screenSpaceRadius = false;
      ao.setQualityMode(quality === "ultra" ? "High" : "Medium");
      this.composer.addPass(ao);
      this.dof = new DepthOfFieldEffect(this.camera, {
        focusDistance: 1060, focusRange: 520, bokehScale: 1.2, resolutionScale: 0.5,
      });
      this.composer.addPass(new EffectPass(this.camera, this.dof));
    }
    this.composer.addPass(new EffectPass(this.camera, this.grade));

    this.resize();
    window.addEventListener("resize", this.resize);
  }

  private lights() {
    // key: Quick3D eulerRotation (-52, -38, 0) → direction (0.379, -0.788, -0.485)
    const key = new THREE.DirectionalLight(0xfff6ea, LOOK.key);
    key.position.set(-0.379, 0.788, 0.485).multiplyScalar(1200);
    key.castShadow = true;
    key.shadow.mapSize.set(4096, 4096);
    const cam = key.shadow.camera as THREE.OrthographicCamera;
    cam.left = -520; cam.right = 520; cam.top = 420; cam.bottom = -420; cam.near = 400; cam.far = 2000;
    key.shadow.bias = -0.0004;
    key.shadow.normalBias = 0.6;
    key.shadow.radius = 5;
    this.scene.add(key, key.target);
    // cool fill from camera-right/back: eulerRotation (-10, 120, 0)
    const fill = new THREE.DirectionalLight(0xdfe8ff, LOOK.fill);
    fill.position.set(0.853, 0.174, -0.4925).multiplyScalar(1200);
    this.scene.add(fill, fill.target);
  }

  private environment() {
    const pmrem = new THREE.PMREMGenerator(this.renderer);
    new HDRLoader().load("./scene/studio.hdr", (hdr) => {
      if (this.disposed) { hdr.dispose(); pmrem.dispose(); return; }
      hdr.mapping = THREE.EquirectangularReflectionMapping;
      this.environmentTarget = pmrem.fromEquirectangular(hdr);
      const env = this.environmentTarget.texture;
      this.scene.environment = env;
      this.scene.environmentIntensity = LOOK.env;
      this.scene.environmentRotation.set(0, -25 * DEG, 0);
      hdr.dispose();
      pmrem.dispose();
    }, undefined, () => pmrem.dispose());
  }

  resize = () => {
    const w = this.canvas.clientWidth || window.innerWidth;
    const h = this.canvas.clientHeight || window.innerHeight;
    this.renderer.setSize(w, h, false);
    this.composer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  };

  // ── control ──
  pulse(origin = 0.25) {
    const i = this.pulseSlot;
    this.pulseT[i] = this.time;
    this.pulseO[i] = origin;
    this.pulseSlot = (i + 1) % 16;
  }
  playAssemble(duration = 2300, from = this.assemble) {
    this.assemble = from;
    this.assembleAnim = { from, to: 1, t0: this.time, dur: duration / 1000 };
  }
  setAssembled() { this.assemble = 1; this.assembleAnim = null; }
  hideBand() { this.assemble = 0; this.assembleAnim = null; }
  playDissolve(duration = 1100) { this.dissolveAnim = { t0: this.time, dur: duration / 1000 }; }
  get intro() { const a = Math.min(1, Math.max(0, this.assemble)); return 1 - Math.pow(1 - a, 3); }

  /** Project a point on the band (curve parameter 0..1) to CSS pixels. */
  project(u: number, out = new THREE.Vector3()) {
    const t = u * Math.PI * 2;
    const r = this.finU.uRadius.value as number, a = this.finU.uAspect.value as number, d = this.finU.uDepth.value as number;
    out.set(r * Math.cos(t), r * a * Math.sin(t) * Math.cos(t), d * Math.sin(t));
    this.sculpture.localToWorld(out);
    out.project(this.camera);
    const w = this.canvas.clientWidth, h = this.canvas.clientHeight;
    return { x: (out.x * 0.5 + 0.5) * w, y: (-out.y * 0.5 + 0.5) * h };
  }

  start() {
    this.clock.start();
    const loop = (now: number) => {
      if (this.disposed) return;
      this.raf = requestAnimationFrame(loop);
      if (this.maxFps > 0 && now - this.lastRender < 1000 / this.maxFps - 2) return;
      this.lastRender = now;
      this.frame();
    };
    this.raf = requestAnimationFrame(loop);
  }

  private frame() {
    const raw = this.clock.getDelta();
    const dt = Math.min(raw, 0.05);
    this.stats.frames++;
    this.stats.ms = raw * 1000;
    if (raw > 0.022) this.stats.slow++;
    this.time += dt;
    this.onFrame?.(dt);              // the app updates targets first, so no frame renders stale state
    const T = this.targets;

    // conveyor kept below one card spacing per frame (no wagon-wheel strobing)
    this.flow = (this.flow + dt * (1.6 + T.verify * 4.0)) % FIN_COUNT;
    this.spin += dt * (0.05 + T.verify * 1.4);
    this.cardFlow = (this.cardFlow + dt * (0.55 + T.verify * 0.9)) % CARD_COUNT;
    const k = 1 - Math.exp(-dt * 3.2);
    this.disorder += (T.denied * 0.12 - this.disorder) * k;
    this.dolly += (T.verify * 70 - T.standby * 40 - this.dolly) * (1 - Math.exp(-dt * 2.0));

    if (this.assembleAnim) {
      const a = this.assembleAnim;
      const x = Math.min(1, (this.time - a.t0) / a.dur);
      this.assemble = a.from + (a.to - a.from) * (0.5 - 0.5 * Math.cos(Math.PI * x));
      if (x >= 1) this.assembleAnim = null;
    }
    if (this.dissolveAnim) {
      const x = Math.min(1, (this.time - this.dissolveAnim.t0) / this.dissolveAnim.dur);
      this.dissolve = x * x;
    }

    const intro = this.intro;
    const u = this.finU;
    u.uTime.value = this.time;
    u.uFlow.value = this.flow;
    u.uSpin.value = this.spin;
    u.uAssemble.value = this.assemble;
    u.uVerify.value = T.verify;
    u.uDenied.value = T.denied;
    u.uAlign.value = T.verify * 0.5;
    u.uDisorder.value = this.disorder;
    u.uDissolve.value = this.dissolve;
    const c = this.cardU;
    c.uTime.value = this.time;
    c.uFlow.value = this.cardFlow;
    c.uReveal.value = T.cardsReveal * (1 - this.dissolve);
    c.uDenied.value = T.denied;

    // camera: dolly-in during assembly, push-in while verifying, damped parallax
    this.camera.position.set(T.parallaxX * 40, 60 + T.parallaxY * 24, 1080 + (1 - intro) * 300 - this.dolly);
    this.camera.rotation.set((-3 - T.parallaxY * 1.2) * DEG, T.parallaxX * 1.6 * DEG, 0);
    this.sculpture.position.x = this.sculptureOffset + T.standby * 150;
    this.sculpture.rotation.order = "YXZ";
    this.sculpture.rotation.set(
      (16 + Math.sin(this.time * 0.11) * 3) * DEG,
      (Math.sin(this.time * 0.07) * 9 - (1 - intro) * 32) * DEG,
      -2 * DEG);

    this.scene.environmentIntensity = LOOK.env * (1 - T.denied * 0.72);
    this.grade.set("uExposure", LOOK.exposure);
    const base = PAPER_LIGHT.map((l, i) => l + (PAPER_DARK[i] - l) * T.denied);
    if (Math.abs(base[0] - this.lastBase) > 1e-4) {
      this.lastBase = base[0];
      const clear = inverseForward(base.map(srgbToLin));
      this.renderer.setClearColor(new THREE.Color().setRGB(clear[0], clear[1], clear[2], THREE.LinearSRGBColorSpace), 0);
      (this.grade.uniforms.get("uPaperBase")!.value as THREE.Vector3).set(base[0], base[1], base[2]);
    }
    this.grade.set("uTime", this.time);
    this.grade.set("uGlitch", T.glitch);
    this.grade.set("uDenied", T.denied);
    this.grade.set("uStandby", T.standby);
    this.grade.set("uVignette", 0.32 + 0.25 * T.denied);

    this.composer.render(dt);
  }

  dispose() {
    if (this.disposed) return;
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    window.removeEventListener("resize", this.resize);
    const materials = new Set<THREE.Material>();
    this.scene.traverse((o) => {
      const m = o as THREE.Mesh;
      if (!m.isMesh) return;
      m.geometry.dispose();
      for (const mat of [m.material, m.customDepthMaterial].flat()) if (mat) materials.add(mat);
    });
    for (const mat of materials) mat.dispose();
    for (const t of this.textures) t.dispose();
    this.environmentTarget?.dispose();
    this.environmentTarget = null;
    this.scene.environment = null;
    this.composer.dispose();          // passes, effects and their render targets
    this.renderer.dispose();
  }
}
