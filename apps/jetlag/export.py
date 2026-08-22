"""Re-exports assets/Jetlag.glb from the hand-placed assets/Jetlag.blend.

Run after moving things in the blend: blender -b -noaudio --python export.py
"""

import os
import sys

# Blender ships its own python, without the numpy the glTF exporter imports.
sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")

bpy.ops.wm.open_mainfile(filepath=f"{OUT}/Jetlag.blend")
bpy.ops.export_scene.gltf(filepath=f"{OUT}/Jetlag.glb", export_format="GLB",
                          export_apply=True, export_cameras=False, export_lights=False)
print("WROTE", f"{OUT}/Jetlag.glb")
