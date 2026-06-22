# 3D Model Generator

Turn **photos of a real object into a lo-fi, low-poly, pixelated 3D game asset**
— the retro / PS1-era aesthetic of games like *Abiotic Factor*. Runs entirely
**locally** on **open-source** tools (no cloud, no paid services, no NVIDIA/CUDA
required), cross-platform on **macOS (Apple Silicon)** and **Windows/WSL2** via a
single container.

```
photos/ ──> [reconstruct] ──> textured mesh ──> [lo-fi] ──> object.glb
            COLMAP + OpenMVS    (PLY + atlas)    decimate +    (low-poly,
            + optional rembg                     pixelate       pixelated,
            background masking                   + glTF         unlit + nearest)
```

## How it works

1. **Reconstruct** (CPU): [COLMAP](https://colmap.github.io) does
   Structure-from-Motion (camera poses) + undistortion; [OpenMVS](https://github.com/cdcseacave/openMVS)
   densifies, meshes, and textures. Optional [rembg](https://github.com/danielgatis/rembg)
   background removal isolates the object.
2. **Lo-fi back-half** (Rust): keep the largest connected component (drop
   floaters) → decimate to a triangle budget (`meshopt`) → center + unit-scale →
   pixelate the texture (downscale + `quantette` palette) → export a
   self-contained `.glb` with `KHR_materials_unlit` + a nearest-neighbour sampler.

The "PS1 feel" that *isn't* in the asset — vertex jitter, affine texture warp,
framebuffer dithering, low-res rendering — is deliberately left to the game
engine's shaders. This tool outputs clean low-poly geometry + a small palettized
texture + the right material flags.

## Requirements

- **Docker** (Desktop on macOS/Windows, or in WSL2). Everything heavy runs in a
  pinned, multi-arch image — nothing else to install.

## Quick start

Build the toolchain + dev image (native to your architecture):

```bash
docker build --target dev -t modelgen:dev -f docker/Dockerfile .
```

Then run the CLI inside it (the Rust workspace is built with `cargo`). The
simplest path is the dev container in VS Code (`.devcontainer/`), or a direct
`docker run` with the project and a work directory mounted:

```bash
docker run --rm -it \
  -v "$PWD":/workspace -w /workspace \
  -v modelgen-target:/workspace/target \
  -v modelgen-cargo-registry:/usr/local/cargo/registry \
  -v modelgen-rembg:/root/.u2net \
  modelgen:dev bash

# inside the container:
cargo run --release --bin modelgen -- doctor          # check the toolchain
cargo run --release --bin modelgen -- \
    process /workspace/photos /workspace/out/object.glb --mask
```

> Packaging note: the compiled `modelgen` binary is not yet baked into the slim
> `runtime` image (a Phase-0-polish TODO) — for now run it from the `dev` image
> via `cargo run`.

## Commands

| Command | What it does |
|---|---|
| `doctor` | Verify COLMAP / OpenMVS / rembg (and, on the host, Blender) are available. |
| `process <photos> <out.glb>` | **End-to-end**, one shot: reconstruct → lo-fi → `.glb`. |
| `reconstruct <photos> <work>` | Just the reconstruction half → a textured `.ply` in `<work>`. |
| `lofi <mesh.ply> <out.glb>` | Just the lo-fi back-half on an existing mesh. |
| `batch <in_dir> <out_dir>` | Run `process` over every photo subfolder; resumable, fault-tolerant, writes `manifest.txt`. |

Common options (on `process` / `batch`): `--mask` (remove background),
`--target-tris N` (default 1500), `--texture-size N` (default 128px),
`--palette-colors N` (default 256), `--max-edge N` (downscale inputs, default
1600). See `--help` on any command.

```bash
# one object
cargo run --release --bin modelgen -- process photos/ chair.glb --mask --target-tris 1000

# hundreds of objects (one subfolder of photos each) — leave running
cargo run --release --bin modelgen -- batch captures/ assets/ --mask
```

## Capturing good photos

- Many overlapping angles (≈ 60–80% overlap), including top and bottom.
- Even, diffuse lighting; avoid harsh shadows and reflections.
- Textured, matte surfaces reconstruct best. **Avoid** glass, mirrors, shiny or
  featureless objects (no generative fallback exists in the all-local pipeline).
- A plain background helps; `--mask` removes it automatically.
- Shoot **JPEG/PNG** (HEIC isn't decoded yet).

## Limitations

- **CPU reconstruction is slow** (~tens of minutes per object) and
  memory-hungry — `batch` is meant to run unattended. A future GPU Linux server
  removes this.
- **Orientation** isn't auto-corrected (photogrammetry frames are gauge-free);
  assets are centered and unit-scaled, but you may need to rotate them in-engine.
- Imports glTF (`.glb`) cleanly into Godot; Unity/Unreal may need the unlit
  material / nearest-filter set on import.

## Development

The Rust workspace (`crates/core` library + `crates/cli`) follows `CLAUDE.md`.
Inside the dev container:

```bash
cargo build              # or cargo run --bin modelgen -- <cmd>
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## License

Tool sources: MIT OR Apache-2.0. Note the bundled external tools' own licenses
(COLMAP BSD, OpenMVS AGPL — invoked as a separate process only, never linked).
