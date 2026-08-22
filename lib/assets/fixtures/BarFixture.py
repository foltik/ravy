"""Builds BarFixture.glb: a 53cm LED wash bar.

Node layout the sim drives: Fixture > {Emit, Emitter}. A plain box
with the LED window recessed slightly into the top face; the Emit material
is the window, and the Emitter empties sit on it with -Z out the beam,
which fires straight up. No moving parts. Origin at floor center; meters.

Run: blender -b -noaudio --python BarFixture.py
"""

import math
import os
import sys

sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.dirname(os.path.abspath(__file__))

LEN, H, D = 0.53, 0.065, 0.075
# LED window, inset behind the top face.
WIN_L, WIN_W = 0.46, 0.042
INSET = 0.006


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


def top_quad(x0, x1, y0, y1, z):
    """Quad on a z-plane facing +Z."""
    return [(x0, y0, z), (x1, y0, z), (x1, y1, z), (x0, y1, z)], [(0, 1, 2, 3)]


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

    hx, hy, wx, wy = LEN / 2, D / 2, WIN_L / 2, WIN_W / 2

    # Body without its top face; the window frame and pocket walls close it.
    body_verts, body_faces = box(LEN, D, 0.0, H)
    body_faces.remove((4, 5, 6, 7))

    frame = [
        top_quad(-hx, hx, -hy, -wy, H),
        top_quad(-hx, hx, wy, hy, H),
        top_quad(-hx, -wx, -wy, wy, H),
        top_quad(wx, hx, -wy, wy, H),
    ]

    # Pocket walls: the window box turned inside out, minus top and bottom.
    pocket_verts, pocket_faces = box(WIN_L, WIN_W, H - INSET, H)
    pocket_faces = [tuple(reversed(f)) for f in pocket_faces
                    if f not in ((4, 5, 6, 7), (0, 3, 2, 1))]

    fixture_verts, fixture_faces = merged(
        [(body_verts, body_faces, Matrix.Identity(4))]
        + [(vs, fs, Matrix.Identity(4)) for vs, fs in frame]
        + [(pocket_verts, pocket_faces, Matrix.Identity(4))]
    )
    fixture = obj_from("Fixture", fixture_verts, fixture_faces, plastic)

    obj_from("Emit", *top_quad(-wx, wx, -wy, wy, H - INSET),
             material("Emit", (0, 0, 0), rough=0.2, emissive=True), parent=fixture)

    # One wide-aperture emitter for the whole window: a single uniform column,
    # proud of the top face so the aperture disc clears the housing.
    emitter = bpy.data.objects.new("Emitter", None)
    emitter.empty_display_size = 0.02
    bpy.context.scene.collection.objects.link(emitter)
    emitter.parent = fixture
    # -Z down the beam, which fires up.
    emitter.matrix_basis = (Matrix.Translation((0, 0, H + 0.002))
                            @ Matrix.Rotation(math.pi, 4, "X"))

    bpy.context.view_layer.update()
    out = f"{OUT}/BarFixture.glb"
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", out)


main()
