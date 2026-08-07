"""Builds the Kosmic stage: 20ft container, 16x4ft deck, DJ table, 10 fixtures,
and the three plywood cutouts.

Run: blender -b -noaudio --python stage.py

Writes assets/Kosmic.blend and assets/Kosmic.glb, so edits belong here rather
than in the blend.

Frame: meters, Z up, origin at ground level in the middle of the container's
front face. +Y is upstage (into the container), -Y is toward the audience, +X is
to the right as seen from the audience.

Every fixture is an Empty at its contact point with the surface it sits on,
oriented so -Z is the beam at its home position (the GDTF root convention).
Proxy bodies hang off it, sized from the GDTF <Model> boxes.
"""

import math
import os
import sys

# Blender ships its own python, without the numpy the glTF exporter imports.
sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import bpy
from mathutils import Matrix, Vector
from mathutils.geometry import tessellate_polygon

import cicada
import seahorse

FT = 0.3048
IN = 0.0254

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")

# 20ft ISO container, exterior.
CONTAINER = (6.058, 2.438, 2.591)

# Four 4x4ft deck sections, 1.5ft tall, centered on the container.
DECK_W, DECK_D, DECK_H = 16 * FT, 4 * FT, 1.5 * FT
DECK_PANEL = 2 * IN

TABLE_W, TABLE_D, TABLE_H = 6 * FT, 2 * FT, 36 * IN
TABLE_TOP = 1.5 * IN

DECK_TOP = DECK_H
TABLE_TOP_Z = DECK_TOP + TABLE_H
# The table sits on the downstage edge; the DJ stands behind it.
TABLE_Y = -DECK_D + TABLE_D / 2

PLY = 0.75 * IN

# The cicada hangs flat on the container's front face.
CICADA_W = 6 * FT
CICADA_Z = 2.286
CICADA_Y = -0.015

# The seahorses hang at the container's ends, clear of the ground and overhanging
# the corners, each facing stage center.
SEAHORSE_H = 2.198
SEAHORSE_Z = 1.848
SEAHORSE_Y = CICADA_Y
SEAHORSE_X = CONTAINER[0] / 2 - 0.10
# Where the back pars point, in meters outboard and up from the piece's center.
SEAHORSE_AIM = (0.15, 0.10)


def shape(pts, scale, flip=1):
    """An outline in meters, mirrored about X when `flip` is -1."""
    pts = [(flip * scale * x, scale * y) for x, y in pts]
    return pts if flip > 0 else pts[::-1]


def clear():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.length_unit = "METERS"


def material(name, rgb, rough=0.6, metal=0.0):
    mat = bpy.data.materials.get(name)
    if mat:
        return mat
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    return mat


def link(obj, collection):
    collection.objects.link(obj)
    return obj


def mesh_obj(name, verts, faces, mat, collection):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.validate()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    obj.data.polygons.foreach_set("use_smooth", [False] * len(mesh.polygons))
    return link(obj, collection)


def box(name, size, mat, collection):
    """Box centered on its own origin."""
    hx, hy, hz = (s / 2 for s in size)
    verts = [
        (-hx, -hy, -hz), (hx, -hy, -hz), (hx, hy, -hz), (-hx, hy, -hz),
        (-hx, -hy, hz), (hx, -hy, hz), (hx, hy, hz), (-hx, hy, hz),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return mesh_obj(name, verts, faces, mat, collection)


def cyl(name, radius, depth, mat, collection, segments=24):
    """Cylinder centered on its own origin, axis along +Z."""
    hz = depth / 2
    verts, faces = [], []
    for i in range(segments):
        a = 2 * math.pi * i / segments
        verts.append((radius * math.cos(a), radius * math.sin(a), -hz))
        verts.append((radius * math.cos(a), radius * math.sin(a), hz))
    for i in range(segments):
        j = (i + 1) % segments
        faces.append((2 * i, 2 * j, 2 * j + 1, 2 * i + 1))
    faces.append(tuple(range(0, 2 * segments, 2))[::-1])
    faces.append(tuple(range(1, 2 * segments, 2)))
    obj = mesh_obj(name, verts, faces, mat, collection)
    for poly in obj.data.polygons[:segments]:
        poly.use_smooth = True
    return obj


def cutout(name, shapes, mat, collection):
    """Plywood jigsawed to `shapes`, standing in XZ and facing the audience."""
    t = PLY / 2
    verts, faces = [], []
    for pts in shapes:
        n, base = len(pts), len(verts)
        verts += [(x, -t, z) for x, z in pts] + [(x, t, z) for x, z in pts]
        tris = [tuple(base + i for i in f)
                for f in tessellate_polygon([[Vector((x, z, 0.0)) for x, z in pts]])]
        faces += tris
        faces += [tuple(i + n for i in reversed(f)) for f in tris]
        faces += [(base + i, base + (i + 1) % n, base + (i + 1) % n + n, base + i + n)
                  for i in range(n)]
    return mesh_obj(name, verts, faces, mat, collection)


def place(obj, loc, rot=None):
    obj.matrix_world = Matrix.Translation(Vector(loc)) @ (rot or Matrix.Identity(3)).to_4x4()
    return obj


def attach(obj, parent, world):
    """Parent without disturbing the child's world transform."""
    obj.parent = parent
    obj.matrix_basis = parent.matrix_world.inverted() @ world
    return obj


def aim_rot(direction):
    """Rotation whose -Z axis lies along `direction`."""
    return Vector(direction).normalized().to_track_quat("-Z", "Y").to_matrix()


# ---------------------------------------------------------------- venue

def build_venue(col):
    steel = material("ContainerSteel", (0.42, 0.30, 0.16), rough=0.55, metal=0.7)
    deck_mat = material("DeckSteel", (0.06, 0.06, 0.07), rough=0.5, metal=0.8)
    black = material("Black", (0.02, 0.02, 0.02), rough=0.45)
    ply = material("Plywood", (0.85, 0.85, 0.83), rough=0.9)
    ground_mat = material("Ground", (0.05, 0.05, 0.05), rough=0.95)

    place(box("Ground", (40, 40, 0.02), ground_mat, col), (0, 0, -0.01))

    cw, cd, ch = CONTAINER
    place(box("Container", (cw, cd, ch), steel, col), (0, cd / 2, ch / 2))

    place(cutout("Target.Cicada", [shape(cicada.OUTLINE, CICADA_W)], ply, col),
          (0, CICADA_Y, CICADA_Z))
    for name, side in (("Target.Seahorse.L", -1), ("Target.Seahorse.R", 1)):
        obj = cutout(name, [shape(seahorse.OUTLINE, SEAHORSE_H, flip=-side)], ply, col)
        place(obj, (side * SEAHORSE_X, SEAHORSE_Y, SEAHORSE_Z))

    # Four 4x4ft sections, left to right.
    sec = 4 * FT
    for i in range(4):
        cx = -DECK_W / 2 + sec * (i + 0.5)
        cy = -DECK_D / 2
        place(box(f"Deck.{i}", (sec, sec, DECK_PANEL), deck_mat, col),
              (cx, cy, DECK_TOP - DECK_PANEL / 2))
        for sx in (-1, 1):
            for sy in (-1, 1):
                leg_h = DECK_TOP - DECK_PANEL
                place(box(f"Deck.{i}/Leg", (0.06, 0.06, leg_h), deck_mat, col),
                      (cx + sx * (sec / 2 - 0.08), cy + sy * (sec / 2 - 0.08), leg_h / 2))

    place(box("DjTable", (TABLE_W, TABLE_D, TABLE_TOP), black, col),
          (0, TABLE_Y, TABLE_TOP_Z - TABLE_TOP / 2))
    leg_h = TABLE_H - TABLE_TOP
    for sx in (-1, 1):
        for sy in (-1, 1):
            place(box("DjTable/Leg", (0.06, 0.06, leg_h), black, col),
                  (sx * (TABLE_W / 2 - 0.09),
                   TABLE_Y + sy * (TABLE_D / 2 - 0.07),
                   DECK_TOP + leg_h / 2))


# ---------------------------------------------------------------- fixtures

def empty(name, loc, rot, col):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "ARROWS"
    obj.empty_display_size = 0.35
    link(obj, col)
    return place(obj, loc, rot)


def mover(name, loc, facing, base, yoke, head, lens_r, col):
    """Moving head standing on its base: home beam points straight up."""
    body = material("FixtureBody", (0.03, 0.03, 0.035), rough=0.4, metal=0.6)
    glass = material("Lens", (0.7, 0.75, 0.8), rough=0.1)

    yaw = math.atan2(facing[1], facing[0]) - math.pi / 2
    up = Matrix.Rotation(yaw, 3, "Z") @ Matrix.Rotation(math.pi, 3, "X")
    ent = empty(name, loc, up, col)

    flat = Matrix.Rotation(yaw, 3, "Z")
    p = Vector(loc)

    bl, bw, bh = base
    attach(box(f"{name}/Base", (bl, bw, bh), body, col), ent,
           Matrix.Translation(p + Vector((0, 0, bh / 2))) @ flat.to_4x4())

    yl, yw, yh = yoke
    pivot = bh + yh * 0.62
    for sx in (-1, 1):
        arm = box(f"{name}/Yoke", (yw, yw * 0.9, yh), body, col)
        offset = flat @ Vector((sx * (yl / 2 - yw / 2), 0, 0))
        attach(arm, ent, Matrix.Translation(p + offset + Vector((0, 0, bh + yh / 2))) @ flat.to_4x4())

    hl, hw, hh = head
    hd = cyl(f"{name}/Head", hw / 2, hh, body, col)
    attach(hd, ent, Matrix.Translation(p + Vector((0, 0, pivot + hh / 2 - 0.02))) @ flat.to_4x4())

    ls = cyl(f"{name}/Lens", lens_r, 0.012, glass, col)
    attach(ls, ent, Matrix.Translation(p + Vector((0, 0, pivot + hh - 0.02))) @ flat.to_4x4())
    return ent


def par(name, loc, target, col):
    """Static par on a floor yoke, aimed at `target`."""
    body = material("FixtureBody", (0.03, 0.03, 0.035), rough=0.4, metal=0.6)
    glass = material("Lens", (0.7, 0.75, 0.8), rough=0.1)

    p = Vector(loc)
    d = (Vector(target) - p).normalized()
    ent = empty(name, loc, aim_rot(d), col)

    ground = Vector((d.x, d.y, 0))
    yaw = math.atan2(ground.y, ground.x) - math.pi / 2 if ground.length > 1e-4 else 0.0
    flat = Matrix.Rotation(yaw, 3, "Z")

    plate = 0.012
    attach(box(f"{name}/Yoke", (0.241, 0.164, plate), body, col), ent,
           Matrix.Translation(p + Vector((0, 0, plate / 2))) @ flat.to_4x4())

    pivot = 0.145
    for sx in (-1, 1):
        arm = box(f"{name}/Yoke", (0.018, 0.13, pivot + 0.03), body, col)
        offset = flat @ Vector((sx * 0.1115, 0, 0))
        attach(arm, ent, Matrix.Translation(p + offset + Vector((0, 0, (pivot + 0.03) / 2)))
               @ flat.to_4x4())

    seat = p + Vector((0, 0, pivot))
    aim = aim_rot(d).to_4x4()
    attach(cyl(f"{name}/Body", 0.0925, 0.20, body, col), ent, Matrix.Translation(seat) @ aim)
    attach(cyl(f"{name}/Lens", 0.086, 0.012, glass, col), ent,
           Matrix.Translation(seat + d * 0.104) @ aim)
    return ent


def build_fixtures(col):
    # GDTF <Model> boxes: (length, width, height) in meters.
    HYDRO = dict(base=(0.359, 0.239, 0.109), yoke=(0.300, 0.087, 0.270),
                 head=(0.224, 0.198, 0.292), lens_r=0.05)
    OUTCAST = dict(base=(0.303, 0.185, 0.076), yoke=(0.259, 0.082, 0.264),
                   head=(0.207, 0.207, 0.216), lens_r=0.095)

    down = (0, -1)  # facing downstage, toward the audience

    # Movers at the front corners of the deck, inset far enough for the base.
    edge = DECK_W / 2 - 0.20
    for i, x in enumerate((-edge, edge)):
        mover(f"HydroSpot.{i}", (x, -1.05, DECK_TOP), down, col=col, **HYDRO)

    # Movers on the downstage corners of the DJ table.
    for i, sx in enumerate((-1, 1)):
        loc = (sx * (TABLE_W / 2 - 0.17), TABLE_Y - TABLE_D / 2 + 0.12, TABLE_TOP_Z)
        mover(f"OutcastBeamwash.{i}", loc, down, col=col, **OUTCAST)

    # 0,1: outboard, upstage of the movers, washing the seahorses. The pars sit
    # below and inboard of the pieces, so aiming a little past center trades a
    # hot near edge for reach into the far top corner.
    out, up = SEAHORSE_AIM
    for i, sx in enumerate((-1, 1)):
        par(f"ColoradoSolo.{i}", (sx * edge, -1.05 + 1.5 * FT, DECK_TOP),
            (sx * (SEAHORSE_X + out), SEAHORSE_Y, SEAHORSE_Z + up), col)

    # 2,3: outboard, near the movers. 4,5: under the table and slightly upstage.
    # All four sit on the downstage edge and throw 50 degrees up, fanned out
    # toward the sides of the room rather than straight ahead.
    for i, (x, y, yaw) in enumerate(((-1.72, -1.05, 15), (1.72, -1.05, 15),
                                     (-0.50, -0.95, 7.5), (0.50, -0.95, 7.5))):
        loc = (x, y, DECK_TOP)
        elev, out = math.radians(50), math.radians(yaw) * (1 if x > 0 else -1)
        d = (math.sin(out) * math.cos(elev), -abs(math.cos(out)) * math.cos(elev),
             math.sin(elev))
        par(f"ColoradoSolo.{i + 2}", loc, [a + b * 6.0 for a, b in zip(loc, d)], col)


def main():
    clear()
    scene = bpy.context.scene
    venue = bpy.data.collections.new("Stage")
    fixtures = bpy.data.collections.new("Fixtures")
    scene.collection.children.link(venue)
    scene.collection.children.link(fixtures)

    build_venue(venue)
    build_fixtures(fixtures)

    bpy.context.view_layer.update()
    bpy.ops.wm.save_as_mainfile(filepath=f"{OUT}/Kosmic.blend")
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/Kosmic.glb", export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", OUT)


main()
