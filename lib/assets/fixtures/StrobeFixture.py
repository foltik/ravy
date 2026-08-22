"""Builds StrobeFixture.glb: a 31cm LED bar strobe in its U-bracket yoke.

Node layout: Fixture > {Emit, Emitter, Yoke}. A plain box with the LED window
recessed slightly into the front face (+Y) and a knob cylinder through
each side. The Yoke object's origin sits on the knob axis, so rotating it
about local X in Blender models angling the real bracket; it is not
driven at runtime. Origin at floor center; meters.

Run: blender -b -noaudio --python StrobeFixture.py
"""

import math
import os
import sys

sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.dirname(os.path.abspath(__file__))

LEN, H, D = 0.31, 0.07, 0.07
# LED window, inset behind the front face.
WIN_X, WIN_Z0, WIN_Z1 = 0.14, 0.012, 0.058
INSET = 0.005
# Knobs on the pivot axis, poking through the yoke arms.
KNOB_R, KNOB_OUT, PIVOT_Z = 0.015, 0.1725, 0.035
YOKE_HW, ARM_T, ARM_W = 0.16125, 0.004, 0.022
# Knob axis up to the underside of the crossbar.
ARM_TOP, BAR_T = 0.076, 0.004


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


def obj_from(name, verts, faces, mat, parent=None, loc=(0, 0, 0)):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    if parent:
        obj.parent = parent
    obj.location = loc
    return obj


def box(w, d, z0, z1):
    hx, hy = w / 2, d / 2
    verts = [
        (-hx, -hy, z0), (hx, -hy, z0), (hx, hy, z0), (-hx, hy, z0),
        (-hx, -hy, z1), (hx, -hy, z1), (hx, hy, z1), (-hx, hy, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def front_quad(x0, x1, z0, z1, y):
    """Quad on a y-plane facing +Y."""
    return [(x0, y, z0), (x0, y, z1), (x1, y, z1), (x1, y, z0)], [(0, 1, 2, 3)]


def ngon(n, r):
    return [(r * math.cos(2 * math.pi * i / n), r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def prism(r, z0, z1, n=8):
    ring = ngon(n, r)
    verts = [(x, y, z0) for x, y in ring] + [(x, y, z1) for x, y in ring]
    faces = [tuple(range(n - 1, -1, -1)), tuple(range(n, 2 * n))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return verts, faces


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

    hx, hy = LEN / 2, D / 2

    # Body without its front face; the window frame and pocket walls close it.
    body_verts, body_faces = box(LEN, D, 0.0, H)
    body_faces.remove((2, 3, 7, 6))

    frame = [
        front_quad(-hx, hx, 0.0, WIN_Z0, hy),
        front_quad(-hx, hx, WIN_Z1, H, hy),
        front_quad(-hx, -WIN_X, WIN_Z0, WIN_Z1, hy),
        front_quad(WIN_X, hx, WIN_Z0, WIN_Z1, hy),
    ]

    # Pocket walls: the window box turned inside out, minus front and back.
    pocket_verts, pocket_faces = box(2 * WIN_X, INSET, WIN_Z0, WIN_Z1)
    pocket_faces = [tuple(reversed(f)) for f in pocket_faces
                    if f not in ((2, 3, 7, 6), (0, 1, 5, 4))]

    fixture_verts, fixture_faces = merged(
        [(body_verts, body_faces, Matrix.Identity(4))]
        + [(vs, fs, Matrix.Identity(4)) for vs, fs in frame]
        + [(pocket_verts, pocket_faces, Matrix.Translation((0, hy - INSET / 2, 0)))]
        + [(*prism(KNOB_R, hx, KNOB_OUT),
            Matrix.Translation((0, 0, PIVOT_Z)) @ Matrix.Rotation(s * math.pi / 2, 4, "Y"))
           for s in (1, -1)]
    )
    fixture = obj_from("Fixture", fixture_verts, fixture_faces, plastic)

    obj_from("Emit", *front_quad(-WIN_X, WIN_X, WIN_Z0, WIN_Z1, hy - INSET),
             material("Emit", (0, 0, 0), rough=0.2, emissive=True), parent=fixture)

    # One wide-aperture emitter for the whole window, like the bar's, proud
    # of the front face so the aperture disc clears the housing.
    emitter = bpy.data.objects.new("Emitter", None)
    emitter.empty_display_size = 0.02
    bpy.context.scene.collection.objects.link(emitter)
    emitter.parent = fixture
    # -Z out the beam, straight out the front.
    emitter.matrix_basis = (Matrix.Translation((0, hy + 0.002, (WIN_Z0 + WIN_Z1) / 2))
                            @ Matrix.Rotation(math.pi / 2, 4, "X"))

    arm_x = YOKE_HW - ARM_T / 2
    yoke_verts, yoke_faces = merged(
        [(*box(ARM_T, ARM_W, -0.015, ARM_TOP), Matrix.Translation((s * arm_x, 0, 0)))
         for s in (1, -1)]
        + [(*box(2 * YOKE_HW, ARM_W, ARM_TOP, ARM_TOP + BAR_T), Matrix.Identity(4))]
    )
    yoke = obj_from("Yoke", yoke_verts, yoke_faces, plastic, parent=fixture,
                    loc=(0, 0, PIVOT_Z))

    bpy.context.view_layer.update()
    out = f"{OUT}/StrobeFixture.glb"
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", out)


main()
