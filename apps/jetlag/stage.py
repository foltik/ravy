"""Builds the Jetlag stage: the witch hut model, the LED wall, the classic
rig on the grass, and the speakers.

Run: blender -b -noaudio --python stage.py

Writes assets/Jetlag.blend and assets/Jetlag.glb ONCE: placement is done by
hand in the blend, so an existing blend is never overwritten (pass `-- --force`
to start over). After moving things, re-export the glb with export.py.

Everything is movable by its root: the hut under `Stage Mockup`, each fixture
by its empty, each speaker by its empty.

Frame: meters, Z up, origin at ground level under the middle of the hut. The
hut model faces -Y, so -Y is toward the audience and +X is to the right as
seen from the audience.

Every fixture is an Empty at its contact point with the ground, oriented so -Z
is the beam at its home position. Proxy bodies hang off it for the blend; the
sim replaces them with the real fixture models at runtime.
"""

import math
import os
import sys

# Blender ships its own python, without the numpy the glTF exporter imports.
sys.path.append(f"/usr/lib/python3.{sys.version_info.minor}/site-packages")

import bpy
from mathutils import Matrix, Vector

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")

HUT_GLB = os.path.join(OUT, "Baba Yaga House.glb")

# The LED wall: 3x2 modules of 0.5m. The hut's window opening spans x -0.77 to
# 0.76 with its sill at z 2.16, so the wall hangs exactly below the opening,
# top edge at the sill.
SCREEN_W, SCREEN_H = 1.5, 1.0
SCREEN_SILL = 2.16
# Front wall face is y -0.54; hang the wall a little proud of it.
SCREEN_Y = -0.60


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


def wedge(name, w, d, h_front, h_back, mat, collection):
    """A line array cabinet: rectangular from the front, trapezoid from the
    side, base at its own origin. The front face (-Y) is the tall one, so the
    top slopes down toward the back."""
    hx, hy = w / 2, d / 2
    verts = [
        (-hx, -hy, 0), (hx, -hy, 0), (hx, hy, 0), (-hx, hy, 0),
        (-hx, -hy, h_front), (hx, -hy, h_front), (hx, hy, h_back), (-hx, hy, h_back),
    ]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    return mesh_obj(name, verts, faces, mat, collection)


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


def empty(name, loc, rot, col):
    obj = bpy.data.objects.new(name, None)
    obj.empty_display_type = "ARROWS"
    obj.empty_display_size = 0.35
    link(obj, col)
    return place(obj, loc, rot)


# ---------------------------------------------------------------- hut

def build_hut(col):
    """Import the converted STEP model and dress it like the photo."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=HUT_GLB)
    imported = [o for o in bpy.data.objects if o not in before]

    # The STEP assembly carries a one-solid mockup of the whole house on top of
    # the detailed parts; it would z-fight everything it duplicates.
    for obj in list(imported):
        if obj.name.startswith("=>[0:1:1:27]"):
            imported.remove(obj)
            bpy.data.objects.remove(obj, do_unlink=True)

    walls = material("HutWalls", (0.52, 0.33, 0.29), rough=0.85)
    roof = material("HutRoof", (0.05, 0.05, 0.055), rough=0.7)
    platform = material("HutPlatform", (0.21, 0.13, 0.09), rough=0.9)
    legs = material("HutLegs", (0.13, 0.09, 0.06), rough=0.9)

    def dress(obj):
        # Colour by the top-level part the mesh belongs to.
        top = obj
        while top.parent and top.parent.name != "Stage Mockup":
            top = top.parent
        name = top.name
        if name.startswith(("roof",)):
            return roof
        if name.startswith(("front wall", "back wall", "sidewall")):
            return walls
        if name.startswith(("base new",)):
            return platform
        return legs  # foot, toe, leg main

    for obj in imported:
        for c in list(obj.users_collection):
            c.objects.unlink(obj)
        link(obj, col)
        if obj.type == "MESH":
            obj.data.materials.clear()
            obj.data.materials.append(dress(obj))


# ---------------------------------------------------------------- screen

def build_screen(col):
    """The LED wall, exactly 1.5x1.0m, hanging centered below the hut opening,
    facing the audience. The sim swaps its material for the render target."""
    hw, h = SCREEN_W / 2, SCREEN_H
    verts = [
        (-hw, SCREEN_Y, SCREEN_SILL - h), (hw, SCREEN_Y, SCREEN_SILL - h),
        (hw, SCREEN_Y, SCREEN_SILL), (-hw, SCREEN_Y, SCREEN_SILL),
    ]
    obj = mesh_obj("Screen", verts, [(0, 1, 2, 3)], material("ScreenOff", (0.01, 0.01, 0.012), rough=0.4), col)
    # Natural UVs, u right and v up as seen from the audience; the exporter's
    # V-flip puts the render target right side up in engine.
    uvs = [(0, 0), (1, 0), (1, 1), (0, 1)]
    layer = obj.data.uv_layers.new()
    for loop in obj.data.loops:
        layer.data[loop.index].uv = uvs[loop.vertex_index]
    return obj


# ---------------------------------------------------------------- speakers

def speaker(name, loc, facing, col):
    """A 15in tops box on a tripod stand, from primitives. Sized from the
    photos: the cab towers over head height, top around 2.1m."""
    black = material("SpeakerBlack", (0.03, 0.03, 0.03), rough=0.9)
    grille = material("SpeakerGrille", (0.06, 0.06, 0.06), rough=0.6, metal=0.4)

    yaw = math.atan2(facing[1], facing[0]) + math.pi / 2
    flat = Matrix.Rotation(yaw, 3, "Z")
    p = Vector(loc)
    root = empty(name, loc, flat, col)

    pole_h = 1.4
    attach(cyl(f"{name}/Pole", 0.018, pole_h, black, col), root,
           Matrix.Translation(p + Vector((0, 0, pole_h / 2))))
    for i in range(3):
        a = 2 * math.pi * i / 3 + math.pi / 6
        foot = Vector((0.55 * math.cos(a), 0.55 * math.sin(a), 0))
        leg = box(f"{name}/Leg", (0.03, 0.95, 0.03), black, col)
        mid = p + foot / 2 + Vector((0, 0, 0.38))
        aim = (p + Vector((0, 0, 0.76)) - (p + foot)).normalized()
        rot = aim.to_track_quat("Y", "Z").to_matrix()
        attach(leg, root, Matrix.Translation(mid) @ rot.to_4x4())

    cab_w, cab_d, cab_h = 0.45, 0.42, 0.72
    cab_z = pole_h + cab_h / 2 - 0.06
    attach(box(f"{name}/Cab", (cab_w, cab_d, cab_h), black, col), root,
           Matrix.Translation(p + Vector((0, 0, cab_z))) @ flat.to_4x4())
    front = flat @ Vector((0, -cab_d / 2 - 0.005, 0))
    attach(box(f"{name}/Grille", (cab_w - 0.04, 0.01, cab_h - 0.04), grille, col), root,
           Matrix.Translation(p + front + Vector((0, 0, cab_z))) @ flat.to_4x4())
    return root


def array(name, loc, facing, col):
    """The inner PA either side of the hut, as in the close-up photo: a
    subwoofer on the grass, a pole rising off it, and a two-box mini line
    array at the top of the pole. The upper cabinet sits level, the lower one
    hangs under it splayed down toward the near crowd."""
    black = material("SpeakerBlack", (0.03, 0.03, 0.03), rough=0.9)

    yaw = math.atan2(facing[1], facing[0]) + math.pi / 2
    flat = Matrix.Rotation(yaw, 3, "Z")
    p = Vector(loc)
    root = empty(name, loc, flat, col)

    sub_w, sub_d, sub_h = 0.55, 0.50, 0.55
    attach(box(f"{name}/Sub", (sub_w, sub_d, sub_h), black, col), root,
           Matrix.Translation(p + Vector((0, 0, sub_h / 2))) @ flat.to_4x4())

    pole_top = 1.45
    attach(cyl(f"{name}/Pole", 0.02, pole_top - sub_h, black, col), root,
           Matrix.Translation(p + Vector((0, 0, (sub_h + pole_top) / 2))))

    # Cabinets are wider than tall, rectangular from the front, wedged from
    # the side. The lower one tips its face down about 18 degrees.
    w, d, hf, hb = 0.36, 0.30, 0.22, 0.15
    down = Matrix.Rotation(math.radians(18), 3, "X")
    lo = wedge(f"{name}/Cab", w, d, hf, hb, black, col)
    attach(lo, root, Matrix.Translation(p + Vector((0, 0, pole_top))) @ (flat @ down).to_4x4())
    hi = wedge(f"{name}/Cab", w, d, hf, hb, black, col)
    attach(hi, root, Matrix.Translation(p + Vector((0, 0, pole_top + 0.22))) @ flat.to_4x4())
    return root


# ---------------------------------------------------------------- fixtures

def mover(name, loc, facing, base, head_r, col):
    """Moving head standing on the grass: home beam points straight up."""
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
    hh = 0.16
    attach(cyl(f"{name}/Head", head_r, hh, body, col), ent,
           Matrix.Translation(p + Vector((0, 0, bh + hh / 2 + 0.05))) @ flat.to_4x4())
    attach(cyl(f"{name}/Lens", head_r * 0.8, 0.012, glass, col), ent,
           Matrix.Translation(p + Vector((0, 0, bh + hh + 0.05))) @ flat.to_4x4())
    return ent


def par(name, loc, target, col):
    """Par can on the grass, aimed at `target`."""
    body = material("FixtureBody", (0.03, 0.03, 0.035), rough=0.4, metal=0.6)
    glass = material("Lens", (0.7, 0.75, 0.8), rough=0.1)

    p = Vector(loc)
    d = (Vector(target) - p).normalized()
    ent = empty(name, loc, aim_rot(d), col)

    seat = p + Vector((0, 0, 0.10))
    aim = aim_rot(d).to_4x4()
    attach(cyl(f"{name}/Body", 0.09, 0.18, body, col), ent, Matrix.Translation(seat) @ aim)
    attach(cyl(f"{name}/Lens", 0.082, 0.012, glass, col), ent,
           Matrix.Translation(seat + d * 0.095) @ aim)
    return ent


def flat_fixture(name, size, loc, col):
    """A bar / strobe body lying on the grass, facing the audience. The sim
    replaces the proxy with its own lit rig."""
    body = material("FixtureBody", (0.03, 0.03, 0.035), rough=0.4, metal=0.6)
    ent = empty(name, loc, Matrix.Identity(3), col)
    l, w, h = size
    attach(box(f"{name}/Body", (l, w, h), body, col), ent,
           Matrix.Translation(Vector(loc) + Vector((0, 0, h / 2))))
    return ent


def spider(name, loc, col):
    """A spider on the grass: the flat rectangular body with its two tilting
    LED banks side by side on top. The sim replaces the proxy with its own
    lit rig."""
    body = material("FixtureBody", (0.03, 0.03, 0.035), rough=0.4, metal=0.6)
    glass = material("Lens", (0.7, 0.75, 0.8), rough=0.1)
    ent = empty(name, loc, Matrix.Identity(3), col)
    p = Vector(loc)
    attach(box(f"{name}/Body", (0.45, 0.25, 0.10), body, col), ent,
           Matrix.Translation(p + Vector((0, 0, 0.05))))
    for sx in (-1, 1):
        bank = box(f"{name}/Bank", (0.20, 0.16, 0.05), glass, col)
        tilt = Matrix.Rotation(sx * math.radians(20), 3, "X")
        attach(bank, ent, Matrix.Translation(p + Vector((sx * 0.11, 0, 0.13))) @ tilt.to_4x4())
    return ent


def build_fixtures(col):
    down = (0, -1)  # facing the audience

    # The movers cluster in two groups as the close-ups show: a pair between
    # each array stack and the hut, and a pair further out either side.
    BEAM = dict(base=(0.20, 0.16, 0.09), head_r=0.055)
    BIG = dict(base=(0.25, 0.20, 0.11), head_r=0.07)
    for i, (x, y) in enumerate([(-1.6, -1.4), (-1.0, -0.8), (1.0, -0.8), (1.6, -1.4)]):
        mover(f"Beam.{i}", (x, y, 0), down, col=col, **BEAM)
    for i, (x, y) in enumerate([(-4.3, -2.0), (-2.1, -1.9), (2.1, -1.9), (4.3, -2.0)]):
        mover(f"BigBeam.{i}", (x, y, 0), down, col=col, **BIG)

    # Pars: one at each array's front corner and one by each tripod as in the
    # photos, the rest uplighting the hut.
    par("Par.0", (-3.1, -1.7, 0), (-2.7, -1.3, 2.0), col)
    par("Par.1", (3.1, -1.7, 0), (2.7, -1.3, 2.0), col)
    par("Par.2", (-5.4, -2.1, 0), (-5.8, -1.5, 1.9), col)
    par("Par.3", (5.4, -2.1, 0), (5.8, -1.5, 1.9), col)
    fronts = [(-1.3, -1.2), (-0.5, -1.3), (0.5, -1.3), (1.3, -1.2)]
    for i, (x, y) in enumerate(fronts):
        par(f"Par.{i + 4}", (x, y, 0), (x * 0.4, -0.5, 3.0), col)
    for i, sx in enumerate((-1, 1)):
        par(f"Par.{i + 8}", (sx * 2.2, 0.3, 0), (sx * 1.05, 0.3, 3.2), col)

    # Spiders halfway between each array stack and its tripod speaker, bars at
    # the hut's legs, strobe dead center under the hut.
    for i, sx in enumerate((-1, 1)):
        spider(f"Spider.{i}", (sx * 4.2, -1.35, 0), col)
    for i, x in enumerate((-1.05, 1.05)):
        flat_fixture(f"Bar.{i}", (1.0, 0.07, 0.07), (x, -0.55, 0), col)
    flat_fixture("Strobe", (0.45, 0.10, 0.12), (0, -0.8, 0), col)


# ---------------------------------------------------------------- venue

def ground(name, size, cell, mat, col):
    """A flat grid, subdivided so triangle density stays uniform however far
    it reaches."""
    n = int(size / cell)
    half = size / 2
    verts = [(-half + i * cell, -half + j * cell, 0.0)
             for j in range(n + 1) for i in range(n + 1)]
    faces = [(j * (n + 1) + i, j * (n + 1) + i + 1,
              (j + 1) * (n + 1) + i + 1, (j + 1) * (n + 1) + i)
             for j in range(n) for i in range(n)]
    return mesh_obj(name, verts, faces, mat, col)


def build_venue(col):
    grass = material("Grass", (0.10, 0.16, 0.07), rough=0.95)
    black = material("SpeakerBlack", (0.03, 0.03, 0.03), rough=0.9)

    ground("Ground", 100.0, 1.0, grass, col)

    # The subwoofer in the void under the hut.
    sub = empty("Sub", (0, -0.05, 0), Matrix.Identity(3), col)
    attach(box("Sub/Box", (0.62, 0.55, 0.60), black, col), sub,
           Matrix.Translation(Vector((0, -0.05, 0.30))))

    # The tripod tops far out either side, toed in toward the crowd, and the
    # sub-plus-array stacks between them and the hut.
    speaker("Speaker.0", (-5.8, -1.5, 0), (0.3, -1), col)
    speaker("Speaker.1", (5.8, -1.5, 0), (-0.3, -1), col)
    array("Array.0", (-2.7, -1.3, 0), (0, -1), col)
    array("Array.1", (2.7, -1.3, 0), (0, -1), col)


def main():
    blend = f"{OUT}/Jetlag.blend"
    if os.path.exists(blend) and "--force" not in sys.argv:
        print(f"{blend} exists and holds hand placement; pass -- --force to start over")
        return

    clear()
    scene = bpy.context.scene
    venue = bpy.data.collections.new("Stage")
    hut = bpy.data.collections.new("Hut")
    fixtures = bpy.data.collections.new("Fixtures")
    for c in (venue, hut, fixtures):
        scene.collection.children.link(c)

    build_hut(hut)
    build_screen(venue)
    build_venue(venue)
    build_fixtures(fixtures)

    bpy.context.view_layer.update()
    bpy.ops.wm.save_as_mainfile(filepath=f"{OUT}/Jetlag.blend")
    bpy.ops.export_scene.gltf(filepath=f"{OUT}/Jetlag.glb", export_format="GLB",
                              export_apply=True, export_cameras=False, export_lights=False)
    print("WROTE", OUT)


main()
