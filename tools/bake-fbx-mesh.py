"""Bake the largest mesh in an FBX file into ChemSpec's compact mesh format.

Run with Blender so the repository build never needs an FBX parser:

    blender --background --python tools/bake-fbx-mesh.py -- input.fbx output.mesh

The output is deterministic for identical evaluated Blender geometry. Positions
are normalized around the horizontal origin with their lowest point at y=0.
"""

from __future__ import annotations

import math
import os
import struct
import sys
from pathlib import Path

import bpy


MAGIC = b"CMSHMESH"
VERSION = 1


def arguments() -> tuple[Path, Path]:
    try:
        separator = sys.argv.index("--")
        source, destination = sys.argv[separator + 1 : separator + 3]
    except (ValueError, IndexError):
        raise SystemExit("expected: -- input.fbx output.mesh") from None
    return Path(source).resolve(), Path(destination).resolve()


def runtime_axis(vector) -> tuple[float, float, float]:
    """Convert Blender's Z-up coordinates to ChemSpec's Y-up coordinates."""

    return float(vector.x), float(vector.z), float(-vector.y)


def normalize(vector: list[float]) -> tuple[float, float, float]:
    length = math.sqrt(sum(component * component for component in vector))
    if length <= 1.0e-12:
        return 0.0, 1.0, 0.0
    return tuple(component / length for component in vector)


def main() -> None:
    source, destination = arguments()
    if source.suffix.lower() != ".fbx":
        raise SystemExit(f"expected an FBX source, got {source}")

    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.fbx(filepath=str(source))
    candidates = [item for item in bpy.context.scene.objects if item.type == "MESH"]
    if not candidates:
        raise SystemExit("FBX contains no mesh objects")
    selected = max(candidates, key=lambda item: len(item.data.polygons))

    dependency_graph = bpy.context.evaluated_depsgraph_get()
    evaluated = selected.evaluated_get(dependency_graph)
    mesh = evaluated.to_mesh(preserve_all_data_layers=False, depsgraph=dependency_graph)
    mesh.calc_loop_triangles()

    positions = [
        runtime_axis(evaluated.matrix_world @ vertex.co) for vertex in mesh.vertices
    ]
    minimum = [min(position[axis] for position in positions) for axis in range(3)]
    maximum = [max(position[axis] for position in positions) for axis in range(3)]
    extent = [maximum[axis] - minimum[axis] for axis in range(3)]
    longest = max(extent)
    if longest <= 1.0e-9:
        raise SystemExit("FBX mesh has zero extent")
    centre_x = (minimum[0] + maximum[0]) * 0.5
    centre_z = (minimum[2] + maximum[2]) * 0.5
    positions = [
        (
            (position[0] - centre_x) / longest,
            (position[1] - minimum[1]) / longest,
            (position[2] - centre_z) / longest,
        )
        for position in positions
    ]

    normals = [[0.0, 0.0, 0.0] for _ in positions]
    indices: list[int] = []
    for triangle in mesh.loop_triangles:
        first, second, third = triangle.vertices
        a, b, c = positions[first], positions[second], positions[third]
        edge_ab = [b[axis] - a[axis] for axis in range(3)]
        edge_ac = [c[axis] - a[axis] for axis in range(3)]
        face = [
            edge_ab[1] * edge_ac[2] - edge_ab[2] * edge_ac[1],
            edge_ab[2] * edge_ac[0] - edge_ab[0] * edge_ac[2],
            edge_ab[0] * edge_ac[1] - edge_ab[1] * edge_ac[0],
        ]
        for index in (first, second, third):
            for axis in range(3):
                normals[index][axis] += face[axis]
        indices.extend((first, second, third))
    normals = [normalize(normal) for normal in normals]

    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("wb") as output:
        output.write(MAGIC)
        output.write(struct.pack("<III", VERSION, len(positions), len(indices)))
        for position, normal in zip(positions, normals, strict=True):
            output.write(struct.pack("<ffffff", *position, *normal))
        output.write(struct.pack(f"<{len(indices)}I", *indices))
    evaluated.to_mesh_clear()
    print(
        f"baked {selected.name}: {len(positions)} vertices, "
        f"{len(indices) // 3} triangles -> {destination}",
        flush=True,
    )
    os._exit(0)


main()
