#!/usr/bin/env python3
"""Host-native Blender bake worker (Phase 5 — stub).

Runs OUTSIDE the container, on the host, using the official Blender's bundled
Python so it can use the host GPU (Metal on Apple Silicon):

    blender --background --python scripts/bake_worker.py -- --work ./work

It watches ``<work>/bake-jobs`` for job descriptors dropped by the containerized
pipeline, performs a Cycles "selected-to-active" diffuse bake (high-detail mesh
-> low-poly UVs), and publishes the baked texture back into ``<work>`` for the
container to pick up.

Handshake (see the plan — robust against Docker Desktop VirtioFS quirks):
  * both sides POLL a short interval (never rely on cross-boundary inotify);
  * payloads are published with an atomic rename (``*.tmp`` -> final);
  * completion is signalled by a sentinel file written AFTER the payload;
  * a timeout + ``*.err`` channel means a missing/dead worker fails the object
    gracefully instead of hanging the pipeline.

This is a stub; the protocol and bake are implemented in Phase 5. Until then the
default in-container CPU bake is used.
"""

raise SystemExit("bake_worker.py is a Phase 5 stub and is not implemented yet")
