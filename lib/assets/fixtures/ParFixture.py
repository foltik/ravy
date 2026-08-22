"""Builds ParFixture.glb: a 12-lens mini par can in its U-bracket yoke.

Node layout: Fixture > {Emit, Emitter, Yoke}. A tapered can with a vented
rim ring, recessed lens plate carrying 9+3 lenses, and a knob cylinder
through each side. The beam fires out local -Z, matching the aim of the
blend's par empties (stage.py's to_track_quat("-Z", "Y")); the Yoke
object's origin sits on the knob axis, so rotating it about local X in
Blender models angling the real bracket. Origin at the back cap; meters.

Run: blender -b -noaudio --python ParFixture.py
"""

import math
import os
import sys

sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.dirname(os.path.abspath(__file__))

DEPTH, FACE_R, BACK_R = 0.09, 0.0575, 0.0475
# Lens plate recessed behind the rim ring, 9 outer + 3 inner 2cm lenses.
PLATE_R, PLATE_Z, LENS_R = 0.0525, 0.085, 0.01
OUTER_R, INNER_R = 0.0405, 0.0175
KNOB_R, KNOB_Z, KNOB_OUT = 0.015, 0.045, 0.070
# U-bracket: straight past the knobs, then a bend in to the crossbar.
YOKE_HW, THIN_HW, ARM_T, ARM_W = 0.06, 0.035, 0.003, 0.02
BEND_Z, RISE = 0.06, 0.04


def material(name, rgb, rough=0.55, emissive=False):
    mat = bpy.data.materials.get(name)
    if mat:
        return mat
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    if emissive:
        bsdf.inputs["Emission Color"].default_value = (1.0, 1.0, 1.0, 1.0)
        bsdf.inputs["Emission Strength"].default_value = 1.0
    return mat


def obj_from(name, verts, faces, mat, parent=None):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    if parent:
        obj.parent = parent
    return obj


def box(w, d, z0, z1):
    hx, hy = w / 2, d / 2
    verts = [
        (-hx, -hy, z0), (hx, -hy, z0), (hx, hy, z0), (-hx, hy, z0),
        (-hx, -hy, z1), (hx, -hy, z1), (hx, hy, z1), (-hx, hy, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def seg(x0, z0, x1, z1, t, w):
    """Flat bar segment skewed from (x0, z0) up to (x1, z1)."""
    verts = [(x0 - t / 2, -w / 2, z0), (x0 + t / 2, -w / 2, z0),
             (x0 + t / 2, w / 2, z0), (x0 - t / 2, w / 2, z0),
             (x1 - t / 2, -w / 2, z1), (x1 + t / 2, -w / 2, z1),
             (x1 + t / 2, w / 2, z1), (x1 - t / 2, w / 2, z1)]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def ngon(n, r):
    return [(r * math.cos(2 * math.pi * i / n), r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def cone(sections, n=12, cap0=True):
    """Round cross sections (z, r) skinned into a tube, back cap optional."""
    verts, faces = [], []
    for z, r in sections:
        verts += [(x, y, z) for x, y in ngon(n, r)]
    if cap0:
        faces.append(tuple(range(n - 1, -1, -1)))
    for s in range(len(sections) - 1):
        b = n * s
        faces += [(b + i, b + (i + 1) % n, b + n + (i + 1) % n, b + n + i) for i in range(n)]
    return verts, faces


def prism(r, z0, z1, n=8):
    ring = ngon(n, r)
    verts = [(x, y, z0) for x, y in ring] + [(x, y, z1) for x, y in ring]
    faces = [tuple(range(n - 1, -1, -1)), tuple(range(n, 2 * n))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return verts, faces


def washer(r0, r1, z, n=12):
    inner, outer = ngon(n, r0), ngon(n, r1)
    verts = [(x, y, z) for x, y in inner] + [(x, y, z) for x, y in outer]
    faces = [(n + i, n + (i + 1) % n, (i + 1) % n, i) for i in range(n)]
    return verts, faces


def disc(r, z, n=8):
    return [(x, y, z) for x, y in ngon(n, r)], [tuple(range(n))]


def merged(pieces):
    verts, faces = [], []
    for vs, fs, xf in pieces:
        base = len(verts)
        verts += [tuple(xf @ Vector(v)) for v in vs]
        faces += [tuple(base + i for i in f) for f in fs]
    return verts, faces


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.length_unit = "METERS"

    plastic = material("PlasticBlack", (0.02, 0.02, 0.02))

    # Built face up, flipped so the face fires out -Z like the aimed empties.
    flip = Matrix.Rotation(math.pi, 4, "X")

    # Rim tube's inner wall, normals facing the recess.
    tube_verts, tube_faces = prism(PLATE_R, PLATE_Z, DEPTH, n=12)
    tube_faces = [tuple(reversed(f)) for f in tube_faces[2:]]

    fixture_verts, fixture_faces = merged(
        [(*cone([(0.0, BACK_R), (0.06, FACE_R), (DEPTH, FACE_R)]), flip),
         (*washer(PLATE_R, FACE_R, DEPTH), flip),
         (tube_verts, tube_faces, flip),
         (*disc(PLATE_R, PLATE_Z, n=12), flip)]
        + [(*prism(KNOB_R, KNOB_Z, KNOB_OUT),
            flip @ Matrix.Translation((0, 0, KNOB_Z)) @ Matrix.Rotation(s * math.pi / 2, 4, "Y"))
           for s in (1, -1)]
    )
    fixture = obj_from("Fixture", fixture_verts, fixture_faces, plastic)

    lenses = [(OUTER_R, 2 * math.pi * i / 9) for i in range(9)]
    lenses += [(INNER_R, math.pi / 2 + 2 * math.pi * i / 3) for i in range(3)]
    emit_verts, emit_faces = merged(
        [(*disc(LENS_R, 0.0), flip @ Matrix.Translation(
            (r * math.cos(a), r * math.sin(a), PLATE_Z + 0.0015)))
         for r, a in lenses]
    )
    obj_from("Emit", emit_verts, emit_faces,
             material("Emit", (0, 0, 0), rough=0.2, emissive=True), parent=fixture)

    emitter = bpy.data.objects.new("Emitter", None)
    emitter.empty_display_size = 0.02
    bpy.context.scene.collection.objects.link(emitter)
    emitter.parent = fixture
    # -Z down the beam, which is the whole fixture's -Z aim axis.
    emitter.matrix_basis = Matrix.Translation((0, 0, -(PLATE_Z + 0.003)))

    yoke_verts, yoke_faces = merged(
        [(*seg(s * (YOKE_HW - ARM_T / 2), -0.02, s * (YOKE_HW - ARM_T / 2), BEND_Z, ARM_T, ARM_W),
          Matrix.Identity(4)) for s in (1, -1)]
        + [(*seg(s * (YOKE_HW - ARM_T / 2), BEND_Z, s * (THIN_HW - ARM_T / 2), BEND_Z + RISE,
                 ARM_T, ARM_W), Matrix.Identity(4)) for s in (1, -1)]
        + [(*box(2 * THIN_HW, ARM_W, BEND_Z + RISE, BEND_Z + RISE + ARM_T), Matrix.Identity(4))]
    )
    yoke = obj_from("Yoke", yoke_verts, yoke_faces, plastic, parent=fixture)
    # Origin on the knob axis, arms toward the back at rest.
    yoke.matrix_basis = Matrix.Translation((0, 0, -KNOB_Z))

    bpy.context.view_layer.update()
    out = f"{OUT}/ParFixture.glb"
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", out)


main()
