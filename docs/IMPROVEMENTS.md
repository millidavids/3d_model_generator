# Plan: Higher-Quality Reconstruction for the Lo-Fi Handoff

## Repo context (for an unbiased reviewer)
- **Project**: `/Users/davidyurek/code/blackhearth_games/3d_model_generator` — a Rust (edition 2024) CLI
  (`modelgen`) that turns photos → a textured glTF `.glb`, CPU-only/CUDA-free. Pipeline:
  preprocess (downscale + optional rembg background mask) → COLMAP Structure-from-Motion →
  OpenMVS dense/mesh/(refine)/texture. Heavy tools run as subprocesses inside a pinned Docker image
  (`modelgen:runtime`). Core lib at `crates/core`, CLI at `crates/cli`.
- **Key files**: `crates/core/src/reconstruct/{sfm.rs,dense.rs,gates.rs,ignore_mask.rs,mod.rs}`,
  `crates/core/src/preprocess/{downscale.rs,segment.rs}`, `crates/core/src/quality.rs`,
  `crates/core/src/external.rs` (subprocess runner + `doctor` tool list),
  `crates/cli/src/{main.rs,commands/{doctor.rs,batch.rs}}`, `docker/Dockerfile`.
- **Existing state (already shipped/committed or in working tree)**:
  - `--quality {draft,balanced,high}` preset (`quality.rs`): sets input downscale (`max_edge`
    1000/1600/2400), dense `--resolution-level` (3/2/1), and whether OpenMVS `RefineMesh` runs (High only).
  - Background masking composites the subject onto black; the dense step is handed a derived OpenMVS
    *ignore-mask* (`--mask-path`/`--ignore-mask-label 0`) so it skips the black background — this fixed a
    "webbing membrane between the legs" artifact.
  - COLMAP feature extraction is capped at `--FeatureExtraction.max_image_size 1600`
    (`SIFT_MAX_IMAGE_SIZE` in sfm.rs) because full-res SIFT on every core OOMs the ~8GB Docker VM
    (COLMAP exits -1). The undistorted images the dense step consumes keep full resolution.
  - `doctor` reports tools, marks `RefineMesh` optional, exits non-zero if a *required* tool is missing.
- **Downstream consumer** = the lo-fi Blender converter (`../lo_fi_converter_blender_addon`). VERIFIED it
  already does: `heal` (keep largest connected component, drop floaters — *and its code warns the
  floor/table can BE the largest*), `watertight` (fill holes / cap the open base), `normalize` (center +
  scale to target; **orientation deliberately left as-is** — "we can't know which way is up"), `prep`
  (apply transforms, merge doubled verts, drop zero-area faces). So those are NOT to be re-done here.

## Guiding principles
1. **Don't duplicate the lo-fi converter.** Invest only in what it can't fix: reconstruction quality
   (poses, density, completeness), input integrity, and object-only output (so its largest-component
   `heal` can't keep the floor and delete the subject).
2. **Backward compatible.** `--quality balanced` (the default) and a no-extra-flags run stay on today's
   behavior. NOTE (review S2): COLMAP (RANSAC mapper) and OpenMVS (OpenMP) are **not** byte-deterministic,
   so "identical output bytes" is not a usable gate. The real, deterministic guarantee is **command
   equality**: a unit test asserts `sift_args(Balanced)` / `dense_args(Balanced)` equal today's exact
   argument vectors, i.e. `balanced` enters no new code path. Output is checked with **metric tolerances**
   (registration count exact; dense-point count within ±10%; leg-separation metric within tolerance).
3. **Every phase ships verified**: pure-logic unit tests + `cargo clippy -- -D warnings` + `cargo fmt
   --check` + `cargo test` + Docker image rebuild + an end-to-end benchmark on the real `~/Downloads/gc`
   capture (registration count / dense points / leg-separation metric / clay+textured render) + commit
   only on explicit approval.

## Goals
- Raise the reconstruction quality floor, especially on low-texture / smooth / thin surfaces
  (observed: the `gc` subject's phone-holding hand reconstructed as a blob).
- Catch bad inputs / bad runs early (blurry frames, sparse registration, broken toolchain).
- Keep output strictly object-only so the downstream `heal` is safe.

## Non-goals
- Mesh heal / largest-component / hole-fill / watertight / center+scale / decimation / de-lighting
  (all done by lo-fi).
- Metric/world scale.
- A GPU/CUDA path.

---

## Phase 1 — Robust SfM (gated to `--quality high`)
**Why**: better poses + a denser, more complete sparse cloud on hard surfaces; nothing downstream
recovers from weak poses.

**Design**
- Thread `Quality` into `sfm::run` (today `run(images_dir, work_dir)`).
- At `High` only, add COLMAP `feature_extractor` flags:
  - `--ImageReader.camera_model OPENCV` (richer lens model than the default `SIMPLE_RADIAL`),
  - `--SiftExtraction.max_num_features 16384` (up from 8192),
  - `--SiftExtraction.estimate_affine_shape 1` + `--SiftExtraction.domain_size_pooling 1`
    (COLMAP's affine-invariant "robust" SIFT mode for difficult datasets).
- Keep `draft`/`balanced` on current fast defaults.

**Risks / mitigations**
- **OOM regression (review S3)** — the three knobs are *multiplicative* (`max_num_features` 2×,
  `estimate_affine_shape`, and `domain_size_pooling`'s `dsp_num_scales`≈10), well beyond a casual "2–4×",
  on top of `high`'s 2400px inputs. This is the exact class the `SIFT_MAX_IMAGE_SIZE` cap exists for.
  → **Required ablation** before declaring P1 done: pin `--FeatureExtraction.num_threads` to a fixed
  value, then enable flags ONE at a time (camera_model → max_num_features → affine_shape → DSP),
  measuring peak RSS at each step. **Pre-decided fallback order if it OOMs**: drop DSP first (priciest),
  then halve `max_num_features`; keep `affine_shape` (cheap, high-value on smooth surfaces) and
  `camera_model` as long as possible.
- **OPENCV needs enough views** to fit extra distortion params; with `single_camera=1` + ~34 images it
  should be fine. → Assert registration count does not regress vs current `high`.
- **Slower extraction** — acceptable for `high`.

**Tests**: unit-test a pure `sift_args(quality)` builder (mirror the existing `dense_args`); `high`
includes the robust flags, `balanced`/`draft` equal today's exact vector (this is the S2 command-equality
guarantee). End-to-end: registration count + sparse reprojection-error/track-length, and a clay render of
the hand/smooth regions vs current `high`.

**Success (review S6)**: registration ≥ current AND lower-or-equal mean reprojection error (from the
sparse model `pick_largest_submodel` already reads) AND visibly better hard-surface detail, no OOM.
Dense-point count is **informational only** — it's dominated by `--resolution-level` (unchanged here) and
stochastic stereo, so it is not a pass/fail gate.

**RESULT (implemented; benchmarked on `gc`)** — shipped, gated to `high`. Controlled same-image ablation
(non-robust vs robust SfM):
- **No OOM** — masking limits each image to ~3–6k features (far under the 16384 cap), so feature-
  extraction peak RSS did *not* rise (4.0 vs 4.8 GB); the run's 6.7 GB peak is the OpenMVS dense stage.
- **No measurable quality gain on `gc`**: registration 34/34 both (already maxed); points 4545→4750
  (+4.5%); mean reproj error 0.986→1.002 px and track length 3.92→3.66 — all within COLMAP RANSAC run-to-
  run noise. Cost ~+1 min (+50%).
- **Why**: `gc` is a cooperative, densely-photographed, well-textured subject that already reconstructs
  fully — no headroom. Robust SfM targets the README's stated weakness (low-texture / smooth / few-image
  captures), which we have no fixture to demonstrate.
- **Decision**: keep in `high` as insurance for hard captures (high already means "slower, best quality";
  no downside observed). Not validated on a hard dataset — revisit if one appears.

---

## Phase 2 — Input quality gating (applies to all presets)
**Why**: the dominant quality lever; a few blurry frames or a half-registered solve silently wreck the
mesh and nothing downstream can fix it.

**2a. Blur filtering**
- New `preprocess/sharpness.rs`: variance-of-Laplacian per image. **Implementation (review S1):** use the
  `image` crate ONLY for decode + `to_luma8`. Do **not** use `imageops::filter3x3` — it clamps responses
  to `[0,255]` (and drops a 1-px border), destroying the Laplacian's signed variance. Hand-roll the 3×3
  convolution accumulating into `f32`, then compute variance over the f32 responses.
- **Relative** threshold (content-independent): flag frames below `fraction × median_sharpness`
  (default ~0.4–0.5, tunable). No absolute cutoff.
- **Pipeline position (review S4b):** the drop happens in `pipeline/reconstruct.rs` **immediately after
  downscale, before masking and SfM**, so the dropped set propagates to every later stage (rembg masks,
  undistort, ignore-mask generation all see the same set). Integration assertion:
  `#masked == #downscaled-after-drop`.
- Default = **warn** (log soft frames + scores). `--drop-blurry` to exclude them, with guards
  **(review S5)**: never leave fewer than `N = max(20, 80% of input count)` images, and never drop more
  than 15% in one run. Default stays warn-only.

**2b. Registration / coverage report**
- After mapping, report `registered R / N images` from the chosen sparse sub-model and **warn** when
  `R/N` is low (e.g. < 0.6). Reuse the data `gates::pick_largest_submodel` already reads; do NOT scrape
  `colmap model_analyzer` (version-fragile).
- Angular-gap coverage (cluster camera centers on a sphere, flag big holes) = **explicit stretch**,
  deferred to a later iteration.

**Risks / mitigations**: thresholds content-dependent → relative + tunable + warn-not-fail by default
(only the most egregious cases could hard-fail, and even then behind a flag). Keep the SfM gate's
existing "registered nothing" hard error.

**Tests**: unit-test the blur metric on a **real fixture vs a Gaussian-blurred copy of it** (strictly
lower), not just synthetic gradients (review S1); unit-test the relative selection including the
all-uniform-sharpness edge cases (all soft ⇒ nothing flagged; all sharp ⇒ no false positives) (S5); the
drop floor/cap guards; the registration-ratio classification.

**Success**: blurry/under-registered runs emit a clear early warning (and optionally drop blur); clean
runs are unaffected and produce identical meshes.

**RESULT (implemented; verified on `gc` + a blur-injected set)** — shipped, all presets.
- `preprocess/sharpness.rs`: hand-rolled f64 variance-of-Laplacian (NOT `filter3x3`, per S1); relative
  threshold (`0.4×median`); drop guards `min(20, 80%)` kept / max 15% dropped (S5). Drop runs right after
  downscale, before mask/SfM, so the set propagates (S4b). 6 unit tests (real Gaussian-blur fixture,
  uniform-set edge cases, guard caps, images.bin header read).
- `--drop-blurry` CLI flag (reconstruct + batch); default is warn-only.
- Registration report in `sfm.rs` (reads the leading u64 of `images.bin`); warns below 60%.
- **End-to-end (blur-injected 34-image set)**: the deliberately-blurred frame scored **3.1** vs sharp
  frames in the hundreds–thousands; `--drop-blurry` dropped 2 (incl. one genuinely-soft real frame),
  `kept=32`, and the count propagated (`masked=32`, `SfM registered=32 / total=32`). Full run succeeded.

---

## Phase 3 — Configurable masking (protect the handoff)
**Why**: a cleaner object-only silhouette; prevent a floor remnant from becoming the largest component
downstream (lo-fi `heal` would then keep the floor and delete the subject). Observed: a thin feet↔floor
membrane sliver survived our current mask.

**Design** (rembg flags verified: `-m/--model`, `-a/--alpha-matting`; models resolve under
`$U2NET_HOME/<name>.onnx`)
- `--mask-model {u2net, u2net_human_seg}` (default `u2net` = current behavior), passed to rembg `-m`.
  **(review S4)** `isnet-general-use` is intentionally **excluded** for now: `gc` is a person, so we have
  no non-person fixture to validate a general model on — don't ship/bake an unmeasured model. Add it later
  only once it beats `u2net` on a real non-person object.
- `--alpha-matting` is a **separate, independently-validated sub-step (review S8)**, not bundled into the
  model change: alpha matting is much slower and can erode thin features (the very feet-floor sliver this
  phase targets). Validate it with its own before/after on the leg-separation metric.
- Bake only the **exposed** models into the Docker image (pinned by sha256, like u2net); each adds ~170MB.

**Risks / mitigations**: image size per baked model → only `u2net_human_seg` added now. `u2net_human_seg`
only helps people → default stays `u2net`.

**Tests**: mask one `gc` image with `u2net` vs `u2net_human_seg`; compare silhouette + feet region.
End-to-end: does `u2net_human_seg` remove the feet-floor sliver and keep the leg gap open (re-run the
leg-separation metric)? Alpha-matting measured separately.

**Success**: a cleaner mask option for people; the default path is unchanged; nothing baked that wasn't
measured.

**RESULT (implemented; measured on `gc`)** — shipped `--mask-model {u2net, u2net-human-seg}` (default
`u2net`, unchanged). `u2net_human_seg` baked into the image (pinned sha256
`01eb6a29…c73c`), verified to mask **offline** (0 downloads). CLI `MaskModelArg` maps kebab→underscore
with a unit test (`u2net-human-seg`→`u2net_human_seg`).
- **Measured u2net vs human_seg on `gc`**: masks agree **IoU 0.96**; human_seg keeps marginally *more*
  foreground (incl. ~+0.8pp in the feet band — ambiguous, could even add contact-ground). So like robust
  SfM, on `gc`'s clean wall-background the alternative doesn't demonstrably beat `u2net`; its benefit is
  for people on **busy** backgrounds, which `gc` isn't. Shipped as an opt-in for that case.
- **`isnet-general-use`**: still excluded (no non-person fixture, S4).
- **`--alpha-matting`: DROPPED** (not just deferred). With our `--only-mask` + hard-threshold composite,
  `-a` is a **no-op** (measured IoU **1.0000**, 0 px changed) — it only refines the *cutout's* alpha,
  which `--only-mask` never emits. Shipping it would be a misleading dead flag.

---

## Phase 4 — `doctor` smoke test + capture guidance
**4a. `doctor --full`** — beyond `--version`, run a bundled fixture through a minimal reconstruct and
assert it yields a real mesh via the existing `gates::require_ply_faces` (low floor). Optional flag
(slow); default `doctor` stays fast. **(review S7)** the fixture must be a **real, textured micro-capture**
(6–8 down-sized frames of a textured object) that *reliably registers on CPU in seconds* — low-texture or
synthetic frames may not register at all and would fail the smoke on a HEALTHY toolchain (false negative).
Acceptance: validate the fixture registers reliably *before* committing it as the gate; cap its runtime.

**4b. Capture guidance** — **(review note)** README already covers overlap (≈60–80%), top/bottom, diffuse
lighting, and matte-vs-shiny; scope this to *filling gaps only*: underside/turntable technique and an
optional scale reference. Docs only.

**Risks / mitigations**: bundling fixture images grows the repo/image → keep them tiny / few. A tiny
smoke still takes seconds → keep it opt-in.

**Tests**: `doctor --full` exits 0 on a healthy toolchain (fixture registers + meshes), non-zero when a
tool is broken/missing.

**Success**: a broken toolchain fails in `doctor`, not 40 minutes into a `batch`.

---

## Phase 5 — Canonical orientation (time-boxed spike; likely low ROI)
**Why**: ship the model upright — the one cleanup lo-fi explicitly punts on.

**Reality check**: hard. Raw phone EXIF has no reliable 3-D gravity vector; we mask the floor out so
there's no ground plane to RANSAC; PCA/longest-axis heuristics are subject-specific. The lo-fi addon
punts for these reasons.

**Plan**: a time-boxed research spike evaluating (a) EXIF orientation hints, (b) a "standing subject"
heuristic (longest principal axis = up) behind an opt-in flag, (c) leave-as-is. Decide go/no-go after
the spike. Default to **skip** unless the spike finds a low-risk win.

---

## Cross-cutting concerns
- **OOM**: re-validate on `gc` after Phase 1 (more features) and Phase 3 (more models), via the S3 ablation.
- **Backward-compat (S2)**: the deterministic gate is **command equality** — a unit test that
  `sift_args(Balanced)`/`dense_args(Balanced)` equal today's exact vectors (no new code path for the
  default). Output is checked with **tolerances** (registration exact; dense ±10%; leg-separation within
  tolerance), NOT byte-identity (COLMAP RANSAC + OpenMVS OpenMP are non-deterministic).
- **Per-phase gate**: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, image rebuild,
  `gc` benchmark + visual verify, then commit on approval.
- **Docs**: update README + `docs/CONCEPTS.md` for each user-visible change.
- **Effort estimates are optimistic** (review): each phase carries a full Docker rebuild (OpenMVS-from-
  source is the slow stage) + a real CPU `gc` run (34×2400px `high`+refine is not fast). P1 in particular
  excludes the S3 ablation sweep. Treat the day estimates as lower bounds.

## Sequencing & rationale
1. **Phase 1 (robust SfM)** — small, de-risks the `quality → sfm` threading and the OOM interaction;
   immediate quality gain on hard surfaces.
2. **Phase 2 (input QC)** — highest value (prevents doomed runs); larger, builds on a validated base.
3. **Phase 3 (masking)** — contained; needs an image rebuild.
4. **Phase 4 (doctor smoke + docs)** — low-risk hardening.
5. **Phase 5 (orientation spike)** — time-boxed; go/no-go; likely skip.

Rough effort: P1 ~0.5d, P2 ~1d, P3 ~0.5d (+ rebuild), P4 ~0.5d, P5 spike ~0.5d.
