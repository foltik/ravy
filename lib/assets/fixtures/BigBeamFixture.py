"""Builds BigBeamFixture.glb: a 90W RGBW mini beam moving head.

Node layout the sim drives: Fixture > Yoke > Head > {Emit, Emitter}.
Yoke's origin is the pan axis (Z up), Head's its tilt pivot (axis along X,
lens up at rest). The Emit material is the protruding dome lens, and the
Emitter empty sits inside it with -Z down the beam for the volumetric pass.
Unlike the angular 60W, this housing is smooth and barrel-round.
Origin at floor center; meters.

Run: blender -b -noaudio --python BigBeamFixture.py
"""

import math
import os
import sys

sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.dirname(os.path.abspath(__file__))

FOOT_H = 0.015
BASE_W, BASE_D, BASE_H = 0.1575, 0.145, 0.065
BASE_TOP = FOOT_H + BASE_H
# Thick arms, so the yoke sits nearly flush against the head's sides.
YOKE_W, YOKE_H, ARM_D, ARM_T = 0.215, 0.16, 0.08, 0.038
PLATE_R, PLATE_H = 0.06, 0.012
PIVOT = 0.115
# Barrel profile along the beam: round back cap, bulge, taper to the face.
HEAD_SECTIONS = [(-0.075, 0.096), (-0.055, 0.114), (-0.030, 0.12),
                 (0.020, 0.115), (0.075, 0.105)]
CHAMFER = 0.35
# The tilt boss: a small hidden cylinder, not a visible disc.
CAP_R = 0.02
# The dome lens: rings shrinking from the face out to the 2cm bulge's tip.
LENS_R = 0.0425
DOME = [(0.070, 0.0425), (0.083, 0.0385), (0.091, 0.0265), (0.095, 0.012)]


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


def box(w, d, z0, z1):
    hx, hy = w / 2, d / 2
    verts = [
        (-hx, -hy, z0), (hx, -hy, z0), (hx, hy, z0), (-hx, hy, z0),
        (-hx, -hy, z1), (hx, -hy, z1), (hx, hy, z1), (-hx, hy, z1),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return verts, faces


def ngon(n, r):
    return [(r * math.cos(2 * math.pi * i / n), r * math.sin(2 * math.pi * i / n)) for i in range(n)]


def rsq(w, c=CHAMFER):
    """Chamfered square: a rounded cross section with flat +-X sides."""
    h, k = w / 2, w / 2 * (1 - c)
    return [(h, -k), (h, k), (k, h), (-k, h), (-h, k), (-h, -k), (-k, -h), (k, -h)]


def loft(rings):
    """Skin (z, pts) cross sections into a closed solid; caps first."""
    n = len(rings[0][1])
    verts = [(x, y, z) for z, pts in rings for x, y in pts]
    top = n * (len(rings) - 1)
    faces = [tuple(range(n - 1, -1, -1)), tuple(range(top, top + n))]
    for s in range(len(rings) - 1):
        b = n * s
        faces += [(b + i, b + (i + 1) % n, b + n + (i + 1) % n, b + n + i) for i in range(n)]
    return verts, faces


def prism(r, z0, z1, n=8):
    return loft([(z0, ngon(n, r)), (z1, ngon(n, r))])


def slab(pts, x0, x1):
    """Outline in the Y-Z plane extruded along X; caps first."""
    n = len(pts)
    verts = [(x0, y, z) for y, z in pts] + [(x1, y, z) for y, z in pts]
    faces = [tuple(range(n - 1, -1, -1)), tuple(range(n, 2 * n))]
    faces += [(i, (i + 1) % n, n + (i + 1) % n, n + i) for i in range(n)]
    return verts, faces


def merged(pieces):
    """Pieces are (verts, faces, transform, rounded?); rounded pieces get all
    but their two leading cap faces smooth shaded."""
    verts, faces, smooth = [], [], []
    for piece in pieces:
        vs, fs, xf = piece[:3]
        vbase, fbase = len(verts), len(faces)
        verts += [tuple(xf @ Vector(v)) for v in vs]
        faces += [tuple(vbase + i for i in f) for f in fs]
        if len(piece) > 3 and piece[3]:
            smooth += range(fbase + 2, fbase + len(fs))
    return verts, faces, smooth


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.length_unit = "METERS"

    plastic = material("PlasticBlack", (0.02, 0.02, 0.02))

    foot = 0.03
    fx, fy = BASE_W / 2 - foot / 2, BASE_D / 2 - foot / 2
    base_verts, base_faces, _ = merged(
        [(*box(BASE_W, BASE_D, FOOT_H, BASE_TOP), Matrix.Identity(4))]
        + [(*box(foot, foot, 0.0, FOOT_H), Matrix.Translation((sx * fx, sy * fy, 0)))
           for sx in (-1, 1) for sy in (-1, 1)]
    )
    fixture = obj_from("Fixture", base_verts, base_faces, plastic)

    # One U-shaped yoke: big rounded arm plates joined by a square-cut band.
    # Only the arms' outer bottom corners round over; the inner span stays
    # square so it reads as one bent piece, not a plank under two plates.
    z0, d = PLATE_H, ARM_D / 2
    bottom = [(-d, z0 + 0.004), (-d + 0.004, z0), (d - 0.004, z0), (d, z0 + 0.004)]
    arm_pts = [
        (d, 0.118), (0.034, 0.148), (0.018, YOKE_H),
        (-0.018, YOKE_H), (-0.034, 0.148), (-d, 0.118),
    ] + bottom
    bridge_top = 0.025
    bridge_pts = [(d, bridge_top), (-d, bridge_top), (-d, z0), (d, z0)]
    yoke_verts, yoke_faces, yoke_smooth = merged([
        (*prism(PLATE_R, 0.0, PLATE_H), Matrix.Identity(4), True),
        (*slab(arm_pts, -YOKE_W / 2, -YOKE_W / 2 + ARM_T), Matrix.Identity(4), True),
        (*slab(arm_pts, YOKE_W / 2 - ARM_T, YOKE_W / 2), Matrix.Identity(4), True),
        (*slab(bridge_pts, -YOKE_W / 2 + ARM_T, YOKE_W / 2 - ARM_T), Matrix.Identity(4), True),
    ])
    yoke = obj_from("Yoke", yoke_verts, yoke_faces, plastic,
                    parent=fixture, loc=(0, 0, BASE_TOP), smooth=yoke_smooth)

    # Side hubs bridge from the barrel's flat sides out to the arms.
    arm_in = YOKE_W / 2 - ARM_T
    cap_x = 0.045
    head_verts, head_faces, head_smooth = merged([
        (*loft([(z, rsq(w)) for z, w in HEAD_SECTIONS]), Matrix.Identity(4), True),
        (*prism(CAP_R, 0.0, arm_in - cap_x), Matrix.Translation((cap_x, 0, 0))
         @ Matrix.Rotation(math.pi / 2, 4, "Y"), True),
        (*prism(CAP_R, 0.0, arm_in - cap_x), Matrix.Translation((-cap_x, 0, 0))
         @ Matrix.Rotation(-math.pi / 2, 4, "Y"), True),
    ])
    head = obj_from("Head", head_verts, head_faces, plastic,
                    parent=yoke, loc=(0, 0, PIVOT), smooth=head_smooth)

    dome_verts, dome_faces, dome_smooth = merged([
        (*loft([(z, ngon(8, r)) for z, r in DOME]), Matrix.Identity(4), True),
    ])
    obj_from("Emit", dome_verts, dome_faces,
             material("Emit", (0, 0, 0), rough=0.2, emissive=True),
             parent=head, smooth=dome_smooth)

    emitter = bpy.data.objects.new("Emitter", None)
    emitter.empty_display_size = 0.02
    bpy.context.scene.collection.objects.link(emitter)
    emitter.parent = head
    # -Z down the beam, which fires up at rest.
    emitter.matrix_basis = (Matrix.Translation((0, 0, 0.088))
                            @ Matrix.Rotation(math.pi, 4, "X"))

    bpy.context.view_layer.update()
    out = f"{OUT}/BigBeamFixture.glb"
    bpy.ops.export_scene.gltf(filepath=out, export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", out)


main()
