# syntax=docker/dockerfile:1
###############################################################################
# 3D model generator — CUDA-free photogrammetry reconstruction image (multi-stage).
#
#   builder  → compiles OpenMVS from source (CPU, no SSE = arm64-safe), deps via apt
#   appbuild → compiles the modelgen release binary (Rust)
#   runtime  → COLMAP (conda, CPU) + OpenMVS + rembg + modelgen on PATH (server base)
#   dev      → runtime + Rust toolchain (used by the devcontainer)
#
# Strategy: OpenMVS build deps come from APT (its Eigen 3 / Boost stay isolated
# from COLMAP's modern conda deps — co-installing them in one conda env is
# unsolvable). COLMAP comes from conda-forge pinned to its CPU build string.
#
# Blender note: no official arm64-Linux Blender exists. On Apple Silicon the
# bake runs HOST-NATIVE (official macOS Blender, Metal), so this image OMITS
# Blender on arm64. The amd64 image adds an official Linux Blender via
# INSTALL_BLENDER=1.
#
# Build natively per arch: arm64 on the Mac now; amd64 on the Windows box later.
#
# Pinned for reproducibility: the base image by manifest-list digest (below),
# OpenMVS by release tag + VCG by commit SHA (further down). Still floating:
# apt package versions, conda packages, and the rembg pip install — TODO: a
# conda-lock + a constraints file would close those.
###############################################################################

# Digest is the multi-arch manifest list; BuildKit selects the per-platform image.
ARG MINIFORGE=condaforge/miniforge3:24.11.3-2@sha256:1148ac0d3edaa4f25ca77d35e440766883a1be275fd5552f106c4756ac53f532

# --- builder: compile OpenMVS (apt deps) -------------------------------------
FROM ${MINIFORGE} AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential cmake ninja-build git ca-certificates pkg-config \
        libeigen3-dev libboost-all-dev libopencv-dev libcgal-dev libnanoflann-dev \
        libjxl-dev libpng-dev libjpeg-dev libtiff-dev libexiv2-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
# Pin OpenMVS to the v2.3.0 release. It ships the apps we use (InterfaceCOLMAP,
# DensifyPointCloud, ReconstructMesh, RefineMesh, TextureMesh) while avoiding two
# newer headaches: master's `libs/SFM` module (vcpkg-only TinyEXIF/TinyNPY/
# PoseLib/Ceres), and v2.4.0's JPEG-XL code path (cv::IMWRITE_JPEGXL_QUALITY needs
# OpenCV >= 4.8; Ubuntu 24.04 ships 4.6). We use COLMAP for SfM, so OpenMVS's own
# SfM is never needed.
# VCG has no releases, so pin it by commit SHA — a floating HEAD is a silent
# reproducibility hole that could break the OpenMVS build without warning.
# TODO(slim): prune the now-unused apt libs (jxl/exiv2) from the runtime stage.
RUN mkdir vcglib && cd vcglib && git init -q && \
    git fetch --depth 1 https://github.com/cdcseacave/VCG.git \
        658ba36d0a5666650da6e066b4794efc5a463407 && \
    git checkout -q FETCH_HEAD && cd .. && \
    git clone --depth 1 --branch v2.3.0 https://github.com/cdcseacave/openMVS.git openMVS

# Configure (split from the compile so missing-dep failures surface in seconds).
# CPU-only; SSE off = arm64-safe. Viewer/Python/BreakPad/SiftGPU/Tests off (we
# only need the reconstruction apps); IPO/LTO off to avoid slow, OOM-prone links.
# _FORTIFY_SOURCE=0: Ubuntu 24.04 defaults glibc hardening to level 3, which
# false-positives ("*** buffer overflow detected ***") in OpenMVS's release
# depth-map code — a known v2.3.0 issue on 24.04 (x86_64 AND arm64; not a debug
# build, not macOS). cdcseacave/openMVS#1107/#1136/#1176.
RUN cmake -S openMVS -B /build/openMVS -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_CXX_FLAGS="-U_FORTIFY_SOURCE -D_FORTIFY_SOURCE=0" \
        -DVCG_ROOT=/src/vcglib \
        -DOpenMVS_USE_CUDA=OFF \
        -DOpenMVS_USE_SSE=OFF \
        -DOpenMVS_USE_OPENMP=ON \
        -DOpenMVS_BUILD_VIEWER=OFF \
        -DOpenMVS_USE_PYTHON=OFF \
        -DOpenMVS_USE_BREAKPAD=OFF \
        -DOpenMVS_USE_SIFTGPU=OFF \
        -DOpenMVS_ENABLE_TESTS=OFF \
        -DOpenMVS_ENABLE_IPO=OFF \
        -DCMAKE_INSTALL_PREFIX=/opt/openmvs
# -j4 (not nproc): heavy CGAL/OpenMVS template units OOM the Docker VM at high
# parallelism — matches OpenMVS's own `make -j4`.
RUN cmake --build /build/openMVS --target install -j4

# --- appbuild: compile the modelgen release binary ---------------------------
# Same base as runtime, so the dynamically-linked binary's glibc matches.
FROM ${MINIFORGE} AS appbuild
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl ca-certificates build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain 1.93.1 --profile minimal
WORKDIR /app
# Only what the build needs (scripts/ holds the include_str!'d Blender rebake
# script). The release binary is copied OUT of the target cache mount so it
# survives into this stage's layer for the runtime COPY below.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY scripts ./scripts
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin modelgen && \
    cp target/release/modelgen /usr/local/bin/modelgen

# --- runtime: COLMAP + OpenMVS + rembg + modelgen on PATH --------------------
FROM ${MINIFORGE} AS runtime
ARG INSTALL_BLENDER=0

# Shared libraries OpenMVS links against at runtime (CGAL is header-only, so no
# runtime lib needed). TODO(slim): copy only the required .so instead of -dev.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libboost-all-dev libopencv-dev libgomp1 \
        libjxl-dev libpng-dev libjpeg-dev libtiff-dev libexiv2-dev \
    && rm -rf /var/lib/apt/lists/*

# COLMAP (CPU build) + the libfaiss/OpenImageIO it needs (see environment.yml).
# conda CDN downloads truncate intermittently here (IncompleteRead); a BuildKit
# cache mount persists fetched packages across build retries (cumulative
# progress) and more retries help a single pass recover. The cache mount keeps
# the package tarballs out of the image layer, so no `mamba clean` is needed.
COPY docker/environment.yml /tmp/environment.yml
RUN --mount=type=cache,target=/opt/conda/pkgs \
        conda config --set remote_max_retries 10 && \
        conda config --set remote_backoff_factor 2 && \
        mamba env update -n base -f /tmp/environment.yml

# Background removal. The [cpu] extra pulls the onnxruntime backend (without it
# rembg errors "No onnxruntime backend found"); [cli] adds the command-line entry
# point. Use the open u2net/isnet models only. Cache mount + retries for the same
# flaky-download reason as the conda step above.
RUN --mount=type=cache,target=/root/.cache/pip \
        pip install --retries 10 "rembg[cpu,cli]"

# Bake in the u2net model so rembg doesn't download it (~176 MB) on first use —
# otherwise every fresh `--rm` container re-fetches it before masking can start.
# U2NET_HOME makes the location explicit and independent of $HOME (rembg honours
# it). Pinned by sha256, like the other external assets, so the build is
# reproducible and a corrupted/truncated download fails loudly here, not at run.
ENV U2NET_HOME=/opt/u2net
RUN mkdir -p "$U2NET_HOME" && \
    wget -q -O "$U2NET_HOME/u2net.onnx" \
        https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx && \
    echo "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491  $U2NET_HOME/u2net.onnx" \
        | sha256sum -c -

# Official Linux x86-64 Blender (amd64 image only; arm64 bakes host-native).
RUN if [ "${INSTALL_BLENDER}" = "1" ]; then \
        mamba install -y -n base -c conda-forge blender && mamba clean -afy ; \
    fi

COPY --from=builder /opt/openmvs /opt/openmvs
# OpenMVS installs its binaries under <prefix>/bin/OpenMVS — expose both.
ENV PATH=/opt/openmvs/bin/OpenMVS:/opt/openmvs/bin:$PATH

# The modelgen CLI itself (release binary), so the image runs without cargo:
#   docker run --rm modelgen:runtime modelgen doctor
COPY --from=appbuild /usr/local/bin/modelgen /usr/local/bin/modelgen

# Shared working dir: bind-mounted at runtime; also the host<->container bake dir.
WORKDIR /work

# --- dev: runtime + pinned Rust toolchain (devcontainer target) --------------
FROM runtime AS dev
# Stable, shareable locations so the devcontainer can cache them on named volumes.
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends curl build-essential pkg-config && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain 1.93.1 --profile minimal -c rustfmt -c clippy
WORKDIR /workspace
