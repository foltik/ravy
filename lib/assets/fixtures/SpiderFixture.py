"""Builds SpiderFixture.glb: a mini dual-bank spider mover.

Two 4-lens bars on independent tilt axes between trapezoid end pods.
Node layout the sim drives: Fixture > Head.{0,1} > {Emit.i, Emitter.i.j}, where
Head.i's origin is its tilt pivot (axis along X, faces up at rest), the Emit.i
material takes the runtime colour, and each Emitter.i.j empty sits on a lens
with -Z down its beam for the volumetric pass. Head.0 is the front bank (-Y in
Blender). Origin at floor center; meters.

Run: blender -b -noaudio --python SpiderFixture.py
"""

import math
import os
import sys

sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.dirname(os.path.abspath(__file__))

LEN, HEIGHT = 0.28, 0.12
TOP_W, BOT_W = 0.15, 0.10
SLAB_H = 0.045
HEAD_L, FACE_W, HEAD_T = 0.205, 0.075, 0.06
PIVOT_Z = 0.09
BANK_Y = 0.0375
LENS = 0.042
LENS_X = [-0.0769, -0.0256, 0.0256, 0.0769]
SPLAY = [-12.0, -4.0, 4.0, 12.0]


def width(z):
    return BOT_W + (TOP_W - BOT_W) * z / HEIGHT


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


def obj_from(name, verts, faces, mat, parent=None, loc=(0, 0, 0), smooth=()):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    obj.data.polygons.foreach_set("use_smooth", [i in smooth for i in range(len(mesh.polygons))])
    bpy.context.scene.collection.objects.link(obj)
    if parent:
        obj.parent = parent
    obj.location = loc
    return obj


def trap_box(x0, x1, z0, z1):
    """Verts/faces for a section that follows the side taper between z0..z1."""
    w0, w1 = width(z0) / 2, width(z1) / 2
    verts = [
        (x0, -w0, z0), (x1, -w0, z0), (x1, w0, z0), (x0, w0, z0),
        (x0, -w1, z1), (x1, -w1, z1), (x1, w1, z1), (x0, w1, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def box(w, d, h):
    hx, hy, hz = w / 2, d / 2, h / 2
    verts = [
        (-hx, -hy, -hz), (hx, -hy, -hz), (hx, hy, -hz), (-hx, hy, -hz),
        (-hx, -hy, hz), (hx, -hy, hz), (hx, hy, hz), (-hx, hy, hz),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def merged(pieces):
    """Merge (verts, faces, transform) pieces into one vert/face list."""
    verts, faces = [], []
    for vs, fs, xf in pieces:
        base = len(verts)
        verts += [tuple(xf @ Vector(v)) for v in vs]
        faces += [tuple(base + i for i in f) for f in fs]
    return verts, faces


def splayed(i):
    """Transform placing local +Z geometry at lens i, fanned along the bar."""
    rot = Matrix.Rotation(math.radians(SPLAY[i]), 4, "Y")
    return Matrix.Translation((LENS_X[i], 0.0, HEAD_T / 2 + 0.004)) @ rot


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.length_unit = "METERS"

    plastic = material("PlasticBlack", (0.02, 0.02, 0.02))

    pod = LEN / 2 - HEAD_L / 2
    base_verts, base_faces = merged([
        (*trap_box(-LEN / 2, LEN / 2, 0.0, SLAB_H), Matrix.Identity(4)),
        (*trap_box(-LEN / 2, -LEN / 2 + pod, SLAB_H, HEIGHT), Matrix.Identity(4)),
        (*trap_box(LEN / 2 - pod, LEN / 2, SLAB_H, HEIGHT), Matrix.Identity(4)),
    ])
    fixture = obj_from("Fixture", base_verts, base_faces, plastic)

    lens_quad = ([(-LENS / 2, -LENS / 2, 0), (LENS / 2, -LENS / 2, 0),
                  (LENS / 2, LENS / 2, 0), (-LENS / 2, LENS / 2, 0)], [(0, 1, 2, 3)])

    for i, side in enumerate((-1, 1)):
        head = obj_from(f"Head.{i}", *box(HEAD_L, FACE_W, HEAD_T), plastic,
                        parent=fixture, loc=(0, side * BANK_Y, PIVOT_Z))
        emit_verts, emit_faces = merged([(*lens_quad, splayed(j)) for j in range(4)])
        obj_from(f"Emit.{i}", emit_verts, emit_faces,
                 material(f"Emit.{i}", (0, 0, 0), rough=0.2, emissive=True), parent=head)
        for j in range(4):
            emitter = bpy.data.objects.new(f"Emitter.{i}.{j}", None)
            emitter.empty_display_size = 0.02
            bpy.context.scene.collection.objects.link(emitter)
            emitter.parent = head
            # -Z down the beam, matching the fan the lens quads splay to.
            emitter.matrix_basis = splayed(j) @ Matrix.Rotation(math.pi, 4, "X")

    bpy.context.view_layer.update()
    out = f"{OUT}/SpiderFixture.glb"
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", out)


main()
