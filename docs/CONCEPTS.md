# 3D Model Generator — Concepts & Methods (a primer)

This is a from-scratch explanation of the ideas behind this tool: what each term means,
why it matters, and how we actually used it. It's written to be read top-to-bottom, but
every section stands alone, and there's a [glossary](#glossary) at the end.

The one idea to anchor everything: **this tool solves an *inverse problem* — it works
backwards from flat photos to the 3D thing that produced them.** A camera takes a 3D
object and flattens it into a 2D picture, throwing depth away. **Photogrammetry** is the
art of running that backwards: take *many* 2D pictures, and use the tiny disagreements
between them to *recover* the depth that was thrown away.

The whole pipeline is three questions asked in order:

1. **Where were the cameras?** (and what does the world's rough skeleton look like?) — this
   is **Structure-from-Motion**, done by **COLMAP**.
2. **Where is every surface point?** (fill the skeleton in to a solid surface) — this is
   **Multi-View Stereo** + meshing, done by **OpenMVS**.
3. **What colour is every surface point?** (paint the photos back onto the surface) — this
   is **texturing**, also OpenMVS.

```
photos/ ─▶ [preprocess] ─▶ [COLMAP: SfM] ─▶ [OpenMVS: MVS] ─▶ scene_textured.glb
           downscale,       where were        dense points →    (mesh + UVs +
           optional mask    the cameras?      mesh → texture     embedded texture)
```

Almost everything below is "one stage of that pipeline," plus the plumbing (running
external tools, checking their output, packaging the result) that connects them.

A defining constraint runs through all of it: **CPU-only, no NVIDIA/CUDA GPU.** Most
photogrammetry tools gate the expensive middle step behind a CUDA GPU; this one
deliberately stays on the CPU so it runs anywhere Docker does (Apple Silicon, WSL2). That
single choice explains a lot of the design (which steps we skip, why we downscale, why it's
slow).

---

## 1. The core distinction: sparse vs dense, structure vs texture

Two distinctions organize the whole pipeline. Hold both in mind:

- **Sparse vs dense.** First we recover a *sparse* model — a few thousand confidently-matched
  points and the camera positions. It's a skeleton: enough to know the shape's *frame*, not
  its surface. Then we go *dense* — millions of points, one for (nearly) every pixel — and
  turn that into a continuous surface. **Sparse answers "where are things roughly"; dense
  answers "what is the actual surface."**

- **Geometry vs texture.** A finished 3D model is two separate things: its **shape** (the
  **mesh** — points in space joined into triangles) and its **surface look** (the **texture**
  — a 2D image of colour wrapped onto the shape). The first two questions above build the
  geometry; the third paints the texture onto it. (This same split is the *whole* subject of
  the downstream lo-fi converter — see [Downstream](#7-downstream-what-happens-next).)

The tool's output bundles both: a mesh **plus** a texture **plus** the **UV coordinates**
that say how the texture wraps onto the mesh, all in one `.glb` file.

---

## 2. Preprocess — getting the photos ready

Before any 3D work, we condition the input images. Two optional steps.

### 2.1 Downscale — keep CPU reconstruction tractable

A modern phone photo is ~12 megapixels. The dense step's cost scales with pixel count, and
on a CPU that's brutal — and the extra resolution buys nothing for a lo-fi target. So we
**cap the longest edge** (default **1600 px**): any image longer than that on its long side
is shrunk; smaller ones are left alone.

We shrink with a **Lanczos** filter — a high-quality resampling kernel that preserves edges
and fine detail far better than a naive average, which matters because the *next* stage hunts
for fine visual detail to match. (`crates/core/src/preprocess/downscale.rs`)

> **Why a longest-edge cap, not a fixed size?** Photos come in portrait and landscape. Capping
> the *longest* edge bounds the pixel budget while preserving each image's aspect ratio.

### 2.2 Background masking — make the mesh object-only

If your object sits on a table, the table has lots of texture too — and the reconstructor
will happily rebuild the *table* along with (or instead of) the object. **Masking** removes
the background so only the object survives.

We use **rembg**, a small open-source background remover. Under the hood it runs a neural
segmentation model (**u2net**) that outputs, per image, a grayscale **mask**: white where it
thinks the object is, black where it thinks the background is. (We deliberately use the
default u2net model and *avoid* rembg's `bria-rmbg` model, which is non-commercial.)

Two non-obvious decisions (`crates/core/src/preprocess/segment.rs`):

- **We mask the *images*, not the tools.** Both COLMAP and OpenMVS *can* take separate mask
  files — but they use *different* mask conventions, and OpenMVS works on COLMAP's
  *undistorted* images (§3.4), so a mask aligned to the original photo wouldn't line up
  anymore. Instead we bake the mask into the pixels: **composite the object onto solid black**
  (every pixel the mask scores below 128 becomes `(0,0,0)`). A flat black region carries *no*
  visual features, so COLMAP's SfM ignores it for free — no special configuration, no alignment
  problem.

- **The dense step needs an explicit "ignore" mask, though.** "Black carries no texture" turns
  out *not* to be enough for the dense stereo step: in a narrow, dark concavity — the gap
  *between* a standing person's legs, the underside where feet meet the ground — both sides are
  black, and the mesher bridges that black-on-black gap with a thin **webbing membrane**. The
  fix is to hand `DensifyPointCloud` a per-image *ignore-mask* (`--mask-path` +
  `--ignore-mask-label`) so it skips background pixels entirely instead of guessing depth for
  them. We derive that mask straight from the (already-black) undistorted image — so it aligns
  by construction — and erode the foreground a few pixels to widen the ignored band. See the
  webbing lesson in §8 and `crates/core/src/reconstruct/ignore_mask.rs`.

> **Mental model:** masking turns "object on a cluttered desk" into "object floating in a void."
> The void is invisible to a feature-matcher, so it can't be reconstructed — but you still have
> to *tell the dense stereo step to stop guessing* inside the void, or it fills the gaps.

---

## 3. COLMAP — Structure-from-Motion (the "where were the cameras?" stage)

**COLMAP** is the workhorse for **Structure-from-Motion (SfM)**: from a pile of overlapping
photos, recover (a) the **pose** of every camera — its position and orientation in 3D — and
(b) a **sparse point cloud** of the scene. It runs entirely on CPU here.
(`crates/core/src/reconstruct/sfm.rs`)

The magic that makes this possible is **parallax**: the same point on the object lands at a
*slightly different spot* in two photos taken from different angles. Measure that shift across
many points and many photos, and geometry pins down where every camera and every point must
have been. SfM is the algorithm that does that bookkeeping at scale.

It runs as a sequence of sub-commands:

### 3.1 Feature extraction — find the distinctive spots

`colmap feature_extractor`. In each image, find **features** (a.k.a. **keypoints**): small,
visually distinctive spots — corners, speckles, texture detail — that can be *recognized
again* in another photo. Each gets a **descriptor**: a little numeric fingerprint of its local
appearance, designed to stay stable when the spot is seen from a different angle, distance, or
lighting. The classic algorithm is **SIFT** (Scale-Invariant Feature Transform); we run the
**CPU** SIFT (`FeatureExtraction.use_gpu 0`).

This is *why* matte, textured objects reconstruct well and blank/shiny ones fail: **a feature
detector needs visual detail to latch onto.** A bumpy white Buddha head is full of features; a
smooth white egg has almost none, and SfM has nothing to match.

We also pass `--ImageReader.single_camera 1`: it tells COLMAP that **every photo came from one
physical camera with one set of lens parameters** (true for a single-phone capture). That lets
it pool evidence from all images to nail down the lens once, instead of re-solving it per photo
— a big robustness and speed win.

> **Intrinsics vs extrinsics.** A camera has **intrinsics** (fixed properties of the lens/sensor
> — focal length, optical centre, distortion) and **extrinsics** (where it *is* — the pose).
> `single_camera` says "intrinsics are shared; only extrinsics change shot to shot."

### 3.2 Matching — find which features are the same point

`colmap exhaustive_matcher`. For every pair of images, compare their feature descriptors and
record which keypoint in image A is the *same physical point* as which keypoint in image B.
**Exhaustive** = compare *all* image pairs; that's O(n²) but perfectly fine for the tens of
images a single-object capture has. (Bigger captures would use a cheaper matcher, but we don't
need it.) Also CPU (`FeatureMatching.use_gpu 0`).

### 3.3 Mapping — solve for poses and the sparse cloud

`colmap mapper`. This is the heart of SfM, called **incremental mapping**: start from a
well-matched pair of images, triangulate their shared points into 3D, then add one camera at a
time — each new photo is positioned by how its features line up with points already placed.

Running continuously underneath is **bundle adjustment**: a big optimization that jiggles *all*
the camera poses and *all* the 3D points together to minimize **reprojection error** — the gap
between where each 3D point *lands* when projected back into each photo and where its feature
was actually *observed*. Minimize that across the whole bundle and you get a globally
consistent solution.

The output is a **sparse reconstruction**: camera poses + a cloud of the confidently-triangulated
points (typically thousands). It's a skeleton, not a surface — but it's the scaffold everything
else hangs on.

> **The sub-model gotcha.** If the photos don't *all* connect through matches (say two clusters
> of angles that never share enough overlap), the mapper emits *several* disconnected sub-models:
> `sparse/0`, `sparse/1`, … We **keep the largest** one (most registered images) and discard the
> rest — a partial-but-coherent reconstruction beats a fragmented one. If COLMAP registers
> *nothing*, that's our earliest, clearest failure signal: "too few images or poor overlap."
> (`gates::pick_largest_submodel`)

### 3.4 Undistortion — remove the lens bend

`colmap image_undistorter`. Real lenses bend straight lines (barrel/pincushion **distortion**).
COLMAP *modeled* that distortion while solving; now it **re-renders every photo as if shot
through a perfect pinhole camera** — straight lines straight — and writes them out alongside the
solved geometry in a tidy folder (`images/` + `sparse/`). We ask for `--output_type COLMAP` so
the folder is in exactly the layout OpenMVS's importer expects.

Why bother? Because the **next** stage (dense stereo) assumes ideal pinhole geometry. Feeding it
undistorted images means its depth math is correct. This also explains §2.2's masking decision:
downstream tools see these *undistorted* images, not your originals.

---

## 4. OpenMVS — Multi-View Stereo (the "where is every surface point?" stage)

COLMAP handed us cameras + a sparse skeleton. **OpenMVS** fills that skeleton into a solid,
textured surface. It's a suite of separate binaries we run in order, each consuming the
previous one's output. All CPU. (`crates/core/src/reconstruct/dense.rs`)

A path-handling note that bit us once: each OpenMVS tool runs **with the work directory as its
cwd** and is given **absolute** file paths. Mixing a relative input *and* a working-folder flag
makes OpenMVS resolve the path twice and **double it** (`/work/work/scene.mvs`). Absolute paths
everywhere avoid that. (See [§8](#8-the-hard-lessons).)

### 4.1 InterfaceCOLMAP — the handoff

Translate COLMAP's scene (poses + undistorted images) into OpenMVS's own scene format
(`scene.mvs`). Pure format conversion; no new geometry. This is the bridge between the two
tools.

### 4.2 DensifyPointCloud — from thousands of points to millions

This is **Multi-View Stereo (MVS)** proper, and it's the step CUDA tools accelerate. For each
image, using the now-known camera poses, it computes a **depth map** — an estimate of *how far
away* the surface is at (nearly) every pixel — by **dense stereo matching**: slide a patch
around a neighbouring view along the line geometry permits (the *epipolar* line) and find where
it matches best; the match position gives the depth. Fuse all the per-view depth maps and you
get a **dense point cloud**: millions of points, essentially one per pixel, blanketing the real
surface.

We tune it for CPU: `--resolution-level 2` (work at quarter-resolution — halve each dimension
twice — which is dramatically faster and plenty for a lo-fi target) and `--max-resolution 1600`
(an absolute pixel ceiling). This is the slowest, most memory-hungry stage; the knobs trade
detail we don't need for speed we do.

### 4.3 ReconstructMesh — points into a surface

A point cloud is still just dots in space — there are no faces, no "solid." **Surface
reconstruction** connects the dots into a continuous **triangle mesh**: a watertight-ish skin
of triangles approximating the real surface the points sampled. OpenMVS does this by tetrahedralizing
the points and extracting the surface that best separates "inside" from "outside." Output:
`scene_mesh.ply` — geometry only, still un-coloured (grey clay).

> **We skip `RefineMesh`.** OpenMVS has an optional `RefineMesh` step that sharpens the mesh
> against the images — and it's the **CUDA-heavy** part. Skipping it is the central CPU-only
> compromise: we accept a slightly softer mesh in exchange for running with no GPU at all.

### 4.4 TextureMesh — paint the photos back on

Finally, colour. **TextureMesh** takes the grey mesh and the source images and **projects the
photographs back onto the surface**: for each triangle it picks the best image(s) that saw that
patch, samples their pixels, and writes the colour into a **texture atlas** — one packed image
holding the whole surface's colour. To address that image it generates **UV coordinates** (the
2D `(u,v)` map saying which spot on the texture belongs to which point on the mesh). The result
is the finished textured model.

> **A small irony of the data flow:** texturing reads from the *dense scene* (`-i scene_dense.mvs`,
> which carries the image list and poses) **and** the bare mesh (`-m scene_mesh.ply`) — it needs
> the cameras to know *where* each photo's pixels land on the geometry.

---

## 5. Output — packaging a self-contained `.glb`

We export **glTF** in its binary single-file form, **`.glb`**. glTF is the "JPEG of 3D": a
Khronos standard interchange format that bundles mesh + UVs + texture + material so any DCC tool
(Blender, etc.) or game engine imports it cleanly.

Two hard-won export decisions:

- **glb, not PLY.** OpenMVS's default output is a `.ply`, but it stores UVs **per-face**, and
  **Blender's PLY importer silently drops them** — you'd get the right shape with no texture and
  no error message. glTF stores UVs per-vertex in the way Blender expects. (We also can't use OBJ:
  OpenMVS v2.3.0 **segfaults** exporting OBJ.) So glb it is.

- **Embed the texture.** OpenMVS writes `scene_textured.glb` *plus a separate*
  `scene_textured_0.png`, with the glb merely *referencing* the PNG by an external `uri`. That
  means the `.glb` alone is **incomplete** — move or delete the sidecar PNG and the model renders
  untextured (magenta "missing texture"). So after export we **rewrite the glb to pull the PNG
  inside its own binary buffer** (`uri` → `bufferView`), making the single file truly
  self-contained — portable, and safe to keep when `--clean` deletes everything else.
  (`crates/core/src/reconstruct/embed.rs`)

> **A glb is a tiny container format.** It's `"glTF"` magic bytes, then a **JSON chunk**
> (the scene graph: meshes, materials, images, bufferViews), then a **BIN chunk** (the raw
> geometry/texture bytes). Embedding a texture means: append the PNG bytes to the BIN chunk
> (4-byte aligned), add a `bufferView` pointing at them, and repoint the image's JSON from
> `uri: "...png"` to that `bufferView`. Then re-stitch the chunks with corrected length
> headers. That's exactly what `embed_textures` does.

---

## 6. The plumbing — how the stages are run and guarded

The 3D math lives in those external C++/Python tools. This Rust crate is the **conductor**:
it preprocesses, invokes each tool in order, checks the output, and packages the result. A few
cross-cutting concerns:

### 6.1 Out-of-process, never linked

Every heavy tool (COLMAP, OpenMVS, rembg) runs as a **subprocess** — we shell out and check the
exit code, never link the libraries. Two reasons: it's the natural way to drive CLI tools, and it
keeps this crate's licensing **independent** of the tools' (notably OpenMVS's **AGPL**, which is
viral when you *link* it but not when you merely *invoke* a separate process).
(`crates/core/src/external.rs`)

### 6.2 Gates — fail fast and clearly

Reconstruction is slow, so a doomed run should die **early** with a human message, not deep
inside a tool ten minutes later. **Gates** are cheap between-stage checks:
`pick_largest_submodel` errors with "COLMAP produced no sparse reconstruction (too few images or
poor overlap?)" the instant SfM comes up empty; `require_nonempty` verifies each stage actually
wrote a non-empty file. (`crates/core/src/reconstruct/gates.rs`) Input is also validated *before*
anything runs — the photo directory must exist and contain at least one decodable image.
(`validate.rs`)

### 6.3 `doctor` — is the toolchain even here?

`modelgen doctor` checks each external tool resolves on `PATH` and runs `--version`. That version
probe **doubles as a smoke test**: a binary can *exist* yet crash on launch (e.g. onnxruntime
hitting an illegal instruction on some arm64 setups), and a crash yields no version string — so a
missing version is itself a red flag. (`crates/cli/src/commands/doctor.rs`)

### 6.4 `batch` — many objects, unattended

Because each object takes tens of minutes, `batch` is built to **run overnight**: it reconstructs
every photo-subfolder, is **resumable** (skips objects whose `.glb` already exists, unless
`--force`), **fault-tolerant** (one object's failure is logged and the batch continues), and
writes a `manifest.txt` after *every* object so progress survives an interruption.
(`crates/cli/src/commands/batch.rs`)

### 6.5 `--clean` — keep only the result

A full run leaves hundreds of MB of intermediates — downscaled/masked images, the dense cloud,
per-view depth maps, the COLMAP database, OpenMVS scene files. `--clean` deletes everything in the
work dir *except* the final `.glb` — but **only after success** (a failed run keeps its
intermediates for debugging). This is *why* §5's texture-embedding matters: once embedded, the
lone `.glb` is complete. (`pipeline::reconstruct` → `clean_intermediates`)

### 6.6 Containerized + CPU-pinned

Everything heavy is installed in a pinned, multi-arch Docker image via conda-forge. The subtle
part: COLMAP is pinned to its **`cpu*` build string** (`colmap=4.0.4=cpu*`) so the solver never
drags in CUDA — and we *explicitly* pin `libfaiss` (cpu) and `openimageio`, because conda-forge's
arm64 COLMAP **links** them but forgets to **declare** them as dependencies, so COLMAP aborts with
"`libfaiss.so: cannot open shared object file`" unless we add them by hand. (`docker/environment.yml`)

---

## 7. Downstream: what happens next

This tool deliberately **stops at a clean, photoreal textured mesh** — high triangle count,
photographic texture with real lighting baked in. Turning that into a **lo-fi / low-poly /
pixelated game asset** (decimate the mesh, shrink + palettize the texture, de-light it,
unlit/nearest material — the PS1 / *Abiotic Factor* look) is a **separate downstream project**:
the **lo-fi converter Blender add-on**, which consumes the `.glb` this tool produces (or any
mesh, e.g. from Apple Object Capture). That add-on has its own `CONCEPTS.md` covering the
mesh-vs-material split, decimation, baking, de-lighting, and palettizing. **This** tool's job is
just to *get you a faithful mesh in the first place.*

---

## 8. The hard lessons (bugs that taught us the most)

The most instructive moments were the bugs. Each is a real principle.

- **The path-doubling bug.** OpenMVS tools resolve paths against a `-w` working folder. Pass a
  *relative* input *and* run them in a cwd and the path gets resolved twice —
  `/work/work/scene.mvs`. Fix: run each tool with the work dir as cwd **and** hand it **absolute**
  paths (we `canonicalize` the work dir up front). **Lesson:** with tools that have an implicit
  working directory, pick one convention — absolute *or* relative — and never mix them.

- **The magenta model: an incomplete `.glb`.** OpenMVS's "glb" left the texture as an external
  sidecar PNG. The file *looked* exported and *opened* fine in OpenMVS's own viewer — but copy the
  `.glb` somewhere on its own and it rendered untextured magenta, and `--clean` would have deleted
  its texture. Fix: embed the PNG into the glb's buffer so "one file" really means one file.
  **Lesson:** "self-contained" is a property to *verify by moving the file*, not assume from the
  extension.

- **The silently-dropped UVs.** OpenMVS's default PLY stores UVs per-face; Blender's PLY importer
  drops them **without error** — right geometry, no texture, no warning. Invisible until you
  actually open it in the target tool. Fix: export glb (per-vertex UVs). **Lesson:** test the
  output in the *consumer* (Blender), not just the producer; "the file wrote successfully" proves
  nothing about whether it *imports* correctly. (And the obvious alternative, OBJ, **segfaulted**
  OpenMVS — a reminder that the fallback can be worse than the problem.)

- **CUDA hiding in a "CPU" build.** COLMAP's GPU SIFT flags were *renamed* between versions
  (`SiftExtraction.use_gpu` → `FeatureExtraction.use_gpu`), and conda-forge's COLMAP can pull CUDA
  unless pinned to the `cpu*` build — which in turn fails to declare `libfaiss`/`openimageio`,
  crashing at launch. **Lesson:** "CPU-only" isn't a single switch; it's a property you re-verify
  at every layer — build string, transitive deps, *and* per-command GPU flags.

- **Garbage in → no output, late.** Feed COLMAP too few or poorly-overlapping photos and it
  registers nothing — but only after feature-extracting and matching everything first. Surfacing
  that as an early, plain-English gate ("too few images or poor overlap?") instead of an empty
  `sparse/` folder turned a baffling silent failure into an actionable one. **Lesson:** for a slow
  pipeline, invest in *early* failure with a *human* message.

- **The webbing between the legs: "black is not empty" to a stereo matcher.** A masked capture of
  a person standing came out with a thin membrane stretched across the gap between the legs (and a
  ground-coloured one bridging the feet). The instinct — "it's the mesher bridging a concavity" —
  sent us through every `ReconstructMesh` knob (`free-space-support`, `remove-spurious`,
  `thickness-factor`, `min-point-distance`): **none helped, and `free-space-support` made it
  worse.** The membrane wasn't invented at the *mesh* stage; it was already in the dense
  *point cloud*. Root cause: we'd composited the background to solid black, and the narrow gap
  between the legs is black-on-black — so the dense stereo step happily "matched" that uniform
  black and planted false points right across the gap, which the mesher then faithfully surfaced.
  Fix: give `DensifyPointCloud` an explicit ignore-mask so it never estimates depth on background
  pixels at all (§2.2). **Lessons:** (1) *diagnose at the right stage* — a mesh artifact can be a
  point-cloud artifact wearing a disguise; cross-section the geometry to see where it's really
  born. (2) "no texture" and "no data" are different things to a matcher — a flat colour is still
  something it will try to match; you have to *exclude* it, not just make it boring.

---

## 9. The methods, named (for further reading)

If you want to go deeper, these are the actual named techniques the tools lean on. Search any of
them.

| What we call it | The real technique / source |
|---|---|
| The whole idea | **Photogrammetry** — recovering 3D geometry from 2D photographs |
| Where were the cameras? | **Structure-from-Motion (SfM)**; **incremental SfM** (COLMAP, Schönberger & Frahm 2016) |
| Finding distinctive spots | **SIFT** — Scale-Invariant Feature Transform (Lowe 2004); **feature detection + descriptors** |
| Pinning the solution down | **Bundle adjustment** — joint optimization minimizing **reprojection error** |
| Why parallax gives depth | **Epipolar geometry** / **triangulation** / **stereo** |
| Removing lens bend | **Camera distortion model** + **undistortion** (pinhole rectification) |
| Points → millions of points | **Multi-View Stereo (MVS)**; **dense stereo / depth-map fusion** (OpenMVS) |
| Points → surface | **Surface reconstruction** (point cloud → triangle mesh) |
| Painting photos on a mesh | **Mesh texturing / texture mapping**; **UV atlas** generation |
| Background removal | **Image segmentation** (rembg + the **u2net** model) |
| High-quality shrink | **Lanczos resampling** |
| File format | **glTF / GLB** (Khronos) — `bufferView`, `uri`, embedded vs external images |
| Licensing posture | **Process isolation** vs **linking** (keeps AGPL OpenMVS at arm's length) |

---

## Glossary

- **Bundle adjustment** — the optimization that nudges all camera poses and 3D points together to
  minimize reprojection error; the engine that makes SfM globally consistent.
- **COLMAP** — the open-source SfM/undistortion tool used for the "where were the cameras?" stage.
- **Dense point cloud** — millions of points (≈ one per pixel) blanketing the real surface; the MVS output.
- **Depth map** — a per-pixel estimate of how far the surface is from one camera.
- **Descriptor** — a numeric fingerprint of a feature's local appearance, stable across viewpoint.
- **Distortion / undistortion** — the lens's bending of straight lines / re-rendering photos as ideal pinhole images.
- **Epipolar geometry** — the constraint that a point in one image must lie along a known line in another; what makes stereo matching tractable.
- **Feature / keypoint** — a distinctive, re-recognizable spot in an image (corner, speckle).
- **glTF / GLB** — the standard 3D interchange format (GLB = its single-file binary form). Stores mesh + UVs + texture + material.
- **Intrinsics / extrinsics** — a camera's fixed lens properties / its variable pose (position + orientation).
- **Mask** — a per-pixel object-vs-background map; we composite the object onto black using it.
- **Mesh** — the shape: vertices/edges/triangles. *Not* the colour.
- **Multi-View Stereo (MVS)** — recovering dense per-pixel depth from many posed images; the dense stage.
- **OpenMVS** — the open-source MVS/meshing/texturing suite used for the dense + surface + texture stages.
- **Parallax** — the apparent shift of a point between two viewpoints; the raw signal depth is recovered from.
- **Photogrammetry** — recovering 3D structure from 2D photographs.
- **Pose** — a camera's position and orientation in 3D.
- **Reprojection error** — the gap between where a 3D point projects into a photo and where its feature was actually seen.
- **rembg / u2net** — the background-removal tool and the segmentation model it runs.
- **Sparse reconstruction** — the SfM output: camera poses + a few thousand confident 3D points (a skeleton).
- **Structure-from-Motion (SfM)** — recovering camera poses + a sparse cloud from overlapping photos.
- **Sub-model** — one disconnected cluster the mapper emits when photos don't all link up; we keep the largest.
- **Surface reconstruction** — turning a point cloud into a continuous triangle mesh.
- **Texture atlas** — one packed image holding the whole surface's colour.
- **TextureMesh** — the OpenMVS step that projects source photos onto the mesh into an atlas.
- **UV coordinates** — the 2D `(u,v)` map saying which spot on the texture wraps onto which point on the mesh.
