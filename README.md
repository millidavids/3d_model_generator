# 3D Model Generator

Turn **photos of a real object into a textured 3D mesh** — local, open-source,
and **CUDA-free** (CPU only, no NVIDIA GPU). This is the part of photogrammetry
that off-the-shelf tools (Scene Scanner, Meshroom's dense step, RealityCapture)
gate behind an NVIDIA GPU; this one runs anywhere Docker does, cross-platform on
**macOS (Apple Silicon)** and **Windows/WSL2**.

```
photos/ ──▶ [preprocess] ──▶ [COLMAP] ──▶ [OpenMVS] ──▶ scene_textured.glb
            downscale,        Structure-    dense cloud →   (mesh + UVs +
            optional rembg    from-Motion   mesh → texture   embedded texture,
            background mask    (CPU SfM)     (CPU MVS)        Blender-ready)
```

The output is a standard textured **glTF `.glb`** — ready to drop into Blender or
any DCC tool. Turning it into a lo-fi / low-poly game asset is a separate,
downstream concern (see **Downstream** below).

## How it works

1. **Preprocess** — downscale inputs (keeps CPU reconstruction tractable) and,
   with `--mask`, remove the background ([rembg](https://github.com/danielgatis/rembg))
   so the mesh is object-only (essential when the object sits on a surface).
2. **COLMAP** ([colmap.github.io](https://colmap.github.io)) — Structure-from-
   Motion: camera poses + a sparse cloud, then undistortion. **CPU**, single-camera.
3. **OpenMVS** ([github.com/cdcseacave/openMVS](https://github.com/cdcseacave/openMVS))
   — dense point cloud → surface mesh → texture. **CPU** (`RefineMesh`, the
   CUDA-heavy step, is skipped).
4. **Output** — `scene_textured.glb`: a self-contained glTF (geometry + UVs +
   embedded texture) that imports cleanly into Blender. (OpenMVS's default PLY
   uses per-face UVs that Blender's importer silently drops, so we export glb.)

## Requirements

- **Docker** (Desktop on macOS/Windows, or in WSL2). Everything heavy runs in a
  pinned, multi-arch image — nothing else to install.

## Quick start

Build the slim runtime image (it bakes in the `modelgen` binary + all tools):

```bash
docker build --target runtime -t modelgen:runtime -f docker/Dockerfile .

# check the toolchain
docker run --rm modelgen:runtime modelgen doctor

# reconstruct one object: photos -> work/ (writes scene_textured.glb)
docker run --rm -v "$PWD/data":/work modelgen:runtime \
    modelgen reconstruct /work/photos /work/out --mask
```

The result is `data/out/scene_textured.glb`.

## Commands

| Command | What it does |
|---|---|
| `doctor` | Verify COLMAP / OpenMVS / rembg are present and run. |
| `reconstruct <photos> <work>` | Photos → `scene_textured.glb` in `<work>`. |
| `batch <in_dir> <out_dir>` | Reconstruct every photo subfolder; resumable, fault-tolerant, writes `manifest.txt`. |

Options: `--mask` (remove background), `--max-edge N` (downscale, default 1600),
`--no-downscale`, `--clean` (after a successful run, delete all intermediates and
leave only `scene_textured.glb`). See `--help` on any command.

## Downstream: making it lo-fi

This tool deliberately stops at a clean textured mesh. Converting that into a
**lo-fi, low-poly, pixelated game asset** (the *Abiotic Factor* / PS1 look —
decimate + pixelated texture + unlit/nearest material) lives in a separate
**Blender add-on** project, which consumes the `.glb` this tool produces (or any
mesh, e.g. from Apple Object Capture on the Mac).

## Capturing good photos

- Many overlapping angles (≈ 60–80% overlap), including top and bottom.
- **Textured, matte** surfaces reconstruct best. Uniform/shiny/featureless
  objects (a plain white statue, glass, mirrors) reconstruct poorly — SfM needs
  visual features. No generative fallback exists in an all-local CPU pipeline.
- Even, diffuse lighting; avoid harsh shadows and reflections.
- Object on a surface → use `--mask`, or the flat surface dominates the result.
- Shoot **JPEG/PNG** (HEIC isn't decoded yet).

## Limitations

- **CPU reconstruction is slow** (~tens of minutes per object) and memory-hungry
  — `batch` is meant to run unattended.
- No NVIDIA/CUDA path by design; a future GPU Linux server could accelerate the
  dense step.

## Development

Rust workspace: `crates/core` (library) + `crates/cli` (`modelgen`). Inside the
dev image (`--target dev`):

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## License

Tool sources: MIT OR Apache-2.0. The bundled external tools keep their own
licenses (COLMAP BSD, OpenMVS AGPL — invoked as separate processes only, never
linked).
