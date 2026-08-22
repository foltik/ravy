"""Swaps the proxy bodies under hand-placed fixture empties in
assets/Jetlag.blend for the real models in lib/assets/fixtures, keeping every
empty's placement. Patches in place, never regenerates the blend.

Run: blender -b -noaudio --python patch.py   (then export.py)
"""

import os
import sys

# Blender ships its own python, without the numpy the glTF importer imports.
sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import math

import bpy
from mathutils import Matrix, Vector

DIR = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.normpath(os.path.join(DIR, "..", "..", "lib", "assets", "fixtures"))

# Empty name prefix -> model whose glb replaces the empty's proxy parts.
MODELS = {"Spider": "SpiderFixture.glb", "Beam": "BeamFixture.glb",
          "BigBeam": "BigBeamFixture.glb", "Bar": "BarFixture.glb",
          "Strobe": "StrobeFixture.glb", "Par": "ParFixture.glb"}

# Empties aimed down their -Z on purpose; never counter-flipped upright.
AIMED = {"Par"}


def descendants(obj):
    for child in obj.children:
        yield from descendants(child)
        yield child


def main():
    bpy.ops.wm.open_mainfile(filepath=f"{DIR}/assets/Jetlag.blend")

    def target(obj):
        prefix, _, index = obj.name.partition(".")
        # Singletons like Strobe have no index.
        return obj.type == "EMPTY" and prefix in MODELS and (index.isdigit() or not index)

    # By name: deleting one empty's proxies invalidates a snapshot of objects.
    patched = 0
    for name in [o.name for o in bpy.data.objects if target(o)]:
        empty = bpy.data.objects[name]
        prefix = name.partition(".")[0]

        # Hand-angled yokes and roots survive a re-patch: carry their local
        # pose onto the fresh import.
        saved = {old.name.partition(".")[0]: old.matrix_basis.copy()
                 for old in descendants(empty) if old.name.startswith(("Yoke", "Fixture"))}
        for old in list(descendants(empty)):
            bpy.data.objects.remove(old, do_unlink=True)

        # stage.py hung the mover empties upside down. Unflip about Y: upright
        # with the facing that puts DMX-zero tilt (toward fixture local +Y)
        # out at the crowd. Position is untouched.
        if prefix not in AIMED and (empty.matrix_world.to_3x3() @ Vector((0, 0, 1))).z < 0:
            empty.matrix_basis = empty.matrix_basis @ Matrix.Rotation(math.pi, 4, "Y")

        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=f"{FIXTURES}/{MODELS[prefix]}")
        imported = [o for o in bpy.data.objects if o not in before]
        collection = empty.users_collection[0]
        for obj in imported:
            for c in obj.users_collection:
                c.objects.unlink(obj)
            collection.objects.link(obj)
        for root in (o for o in imported if o.parent is None):
            root.parent = empty
            root.matrix_parent_inverse = Matrix.Identity(4)
            root.matrix_basis = Matrix.Identity(4)
        for obj in imported:
            base = obj.name.partition(".")[0]
            if base in saved:
                obj.matrix_basis = saved[base]
        patched += 1

    bpy.data.orphans_purge(do_recursive=True)
    bpy.ops.wm.save_mainfile()
    print("PATCHED", patched, "fixtures")


main()
