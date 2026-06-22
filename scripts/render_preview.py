"""Headless Blender geometry preview: render a mesh from a 3/4 view (Workbench).

    blender --background --python render_preview.py -- <mesh> <out.png>

Used to eyeball reconstruction/decimation quality (geometry only, no texture).
"""

import math
import sys

import bpy
import mathutils

argv = sys.argv[sys.argv.index("--") + 1:]
inp, out_png = argv[0], argv[1]

bpy.ops.wm.read_factory_settings(use_empty=True)
low = inp.lower()
if low.endswith(".ply"):
    bpy.ops.wm.ply_import(filepath=inp)
elif low.endswith((".glb", ".gltf")):
    bpy.ops.import_scene.gltf(filepath=inp)
else:
    bpy.ops.wm.obj_import(filepath=inp)

obj = next(o for o in bpy.context.scene.objects if o.type == "MESH")
bb = [obj.matrix_world @ mathutils.Vector(c) for c in obj.bound_box]
center = sum(bb, mathutils.Vector()) / 8.0
size = max(max(v[i] for v in bb) - min(v[i] for v in bb) for i in range(3)) or 1.0

cam_data = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cam_data)
bpy.context.scene.collection.objects.link(cam)
bpy.context.scene.camera = cam
d = size * 2.0
cam.location = center + mathutils.Vector((d * 0.7, -d * 0.9, d * 0.55))
cam.rotation_euler = (center - cam.location).to_track_quat("-Z", "Y").to_euler()

scene = bpy.context.scene
scene.render.engine = "BLENDER_WORKBENCH"
scene.display.shading.light = "STUDIO"
scene.display.shading.show_cavity = True
scene.render.resolution_x = 900
scene.render.resolution_y = 900
scene.render.filepath = out_png
bpy.ops.render.render(write_still=True)
print(f"rendered {inp} -> {out_png}")
