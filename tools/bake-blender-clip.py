"""Bake an evaluated Blender scene into ChemSpec's interpolated clip format.

Usage:

    blender --background --python tools/bake-blender-clip.py -- \
      source.blend destination.clip

The baker samples every source frame, including armatures and modifiers, then
quantizes positions and normals. Runtime playback linearly interpolates adjacent
samples, so a 30 FPS authored clip remains smooth at the display frame rate.
"""

from __future__ import annotations

import math
import os
import struct
import sys
from array import array
from dataclasses import dataclass, field
from pathlib import Path

import bpy


MAGIC = b"CMSCLIP1"
VERSION = 1
MODULE_IDS = {
    "beaker": 0,
    "water": 1,
    "metal": 2,
    "flame": 3,
    "bubbles": 4,
    "splashes": 5,
    "vapour": 6,
    "mixing": 7,
    "salt": 8,
    "stirrer": 9,
    "vessel_anchor": 10,
    "sparks": 11,
    "plume": 12,
    "soot": 13,
    "cloud": 14,
    "particles": 15,
    "pour": 16,
    "sediment": 17,
    "bursts": 18,
    "input": 19,
    "initial_solution": 20,
    "final_solution": 21,
    "original_metal": 22,
    "metal_erosion": 23,
    "replacement_metal_deposit": 24,
    "deposited_metal_flakes": 25,
    "reactant_a": 26,
    "reactant_b": 27,
    "product": 28,
    "reaction_front": 29,
    "reaction_vessel": 30,
    "mixing_tool": 31,
    # Appended only: prior clip IDs remain byte-compatible.
    "beaker_shards": 32,
    "explosion": 33,
}
COLOUR_SLOT_IDS = {
    "MAT_Glass": 0,
    "MAT_Water": 1,
    "MAT_Water_Highlight": 2,
    "MAT_Reactive_Metal": 3,
    "MAT_Flame_Outer": 4,
    "MAT_Flame_Inner": 5,
    "MAT_Flame_Core": 6,
    "MAT_Fizz_Bubble": 7,
    "MAT_Vapour": 8,
    "MAT_Mixture_A": 9,
    "MAT_Mixture_B": 10,
    "MAT_SaltResidue": 11,
    "MAT_Fuel": 12,
    "MAT_IgnitionSpark": 13,
    "MAT_ProductPlume": 14,
    "MAT_CompleteFlame_Outer": 4,
    "MAT_CompleteFlame_Inner": 5,
    "MAT_CompleteFlame_Core": 6,
    "MAT_IncompleteFlame_Outer": 4,
    "MAT_IncompleteFlame_Inner": 5,
    "MAT_IncompleteFlame_Core": 6,
    "MAT_CombustionSmoke": 15,
    "MAT_Soot": 16,
    "MAT_SootDeposit": 17,
    "MAT_LiquidInitial": 18,
    "MAT_LiquidAdded": 19,
    "MAT_PrecipitateCloud": 20,
    "MAT_Precipitate": 21,
    "MAT_GasBubble": 22,
    "MAT_GasCloud": 23,
    "MAT_SolidReactant": 24,
    "MAT_SolutionInitial": 25,
    "MAT_SolutionFinal": 26,
    "MAT_OriginalMetal": 27,
    "MAT_DepositedMetal": 28,
    "MAT_MetalErosion": 29,
    "MAT_ReactantA": 30,
    "MAT_ReactantB": 31,
    "MAT_Product": 32,
    "MAT_ReactionFront": 33,
    "MAT_ReactionVessel": 34,
    "MAT_MixingTool": 35,
}
PASS_IDS = {
    "MAT_Reactive_Metal": 0,
    "MAT_Glass": 1,
    "MAT_Water": 1,
    "MAT_Water_Highlight": 1,
    "MAT_Flame_Outer": 1,
    "MAT_Fizz_Bubble": 1,
    "MAT_Vapour": 1,
    "MAT_Flame_Inner": 2,
    "MAT_Flame_Core": 2,
    "MAT_Mixture_A": 1,
    "MAT_Mixture_B": 1,
    "MAT_SaltResidue": 0,
    "MAT_Fuel": 1,
    "MAT_IgnitionSpark": 2,
    "MAT_ProductPlume": 1,
    "MAT_CompleteFlame_Outer": 1,
    "MAT_CompleteFlame_Inner": 2,
    "MAT_CompleteFlame_Core": 2,
    "MAT_IncompleteFlame_Outer": 1,
    "MAT_IncompleteFlame_Inner": 2,
    "MAT_IncompleteFlame_Core": 2,
    "MAT_CombustionSmoke": 1,
    "MAT_Soot": 0,
    "MAT_SootDeposit": 1,
    "MAT_LiquidInitial": 1,
    "MAT_LiquidAdded": 1,
    "MAT_PrecipitateCloud": 1,
    "MAT_Precipitate": 0,
    "MAT_GasBubble": 1,
    "MAT_GasCloud": 1,
    "MAT_SolidReactant": 0,
    "MAT_SolutionInitial": 1,
    "MAT_SolutionFinal": 1,
    "MAT_OriginalMetal": 0,
    "MAT_DepositedMetal": 0,
    "MAT_MetalErosion": 0,
    "MAT_ReactantA": 0,
    "MAT_ReactantB": 0,
    "MAT_Product": 0,
    "MAT_ReactionFront": 2,
    "MAT_ReactionVessel": 0,
    "MAT_MixingTool": 0,
}

# The supplied heavy-alkali scenes use metal-specific material names for
# semantic aliases already represented by the runtime contract. Preserve
# source transparency/emission by resolving to existing slots rather than
# inventing reaction-specific colour IDs.
SOURCE_MATERIAL_ALIASES = {
    **{
        f"MAT_Reactive_Metal_{symbol}": "MAT_Reactive_Metal"
        for symbol in ("Rb", "Cs", "Fr")
    },
    **{
        f"MAT_Flame_Outer_{symbol}": "MAT_Flame_Outer"
        for symbol in ("Rb", "Cs", "Fr")
    },
    **{
        f"MAT_Flame_Inner_{symbol}": "MAT_Flame_Inner"
        for symbol in ("Rb", "Cs", "Fr")
    },
    **{
        f"MAT_Flame_Core_{symbol}": "MAT_Flame_Core"
        for symbol in ("Rb", "Cs", "Fr")
    },
}


@dataclass
class Track:
    source: bpy.types.Object
    module: int
    render_pass: int
    colour_slot: int
    vertex_count: int = 0
    indices: tuple[int, ...] = ()
    samples: array = field(default_factory=lambda: array("f"))
    minimum: list[float] = field(
        default_factory=lambda: [math.inf, math.inf, math.inf]
    )
    maximum: list[float] = field(
        default_factory=lambda: [-math.inf, -math.inf, -math.inf]
    )
    is_anchor: bool = False


def arguments() -> tuple[Path, Path, set[str], str | None]:
    try:
        separator = sys.argv.index("--")
        source, destination = sys.argv[separator + 1 : separator + 3]
    except (ValueError, IndexError):
        raise SystemExit(
            "expected: -- source.blend destination.clip "
            "[--exclude-module name] [--anchor-object name]"
        ) from None
    options = sys.argv[separator + 3 :]
    excluded_modules: set[str] = set()
    anchor_object = None
    index = 0
    while index < len(options):
        option = options[index]
        if option == "--exclude-module" and index + 1 < len(options):
            excluded_modules.add(options[index + 1])
            index += 2
        elif option == "--anchor-object" and index + 1 < len(options):
            anchor_object = options[index + 1]
            index += 2
        else:
            raise SystemExit(f"unsupported or incomplete option: {option}")
    return (
        Path(source).resolve(),
        Path(destination).resolve(),
        excluded_modules,
        anchor_object,
    )


def runtime_axis(vector) -> tuple[float, float, float]:
    return float(vector.x), float(vector.z), float(-vector.y)


def colour_slot_for(obj: bpy.types.Object) -> str:
    custom = str(obj.get("color_slot", ""))
    custom = SOURCE_MATERIAL_ALIASES.get(custom, custom)
    if custom in COLOUR_SLOT_IDS:
        return custom
    materials = [slot.material.name for slot in obj.material_slots if slot.material]
    if len(materials) != 1:
        raise ValueError(f"{obj.name} has no unique supported colour slot")
    slot = SOURCE_MATERIAL_ALIASES.get(materials[0], materials[0])
    if slot not in COLOUR_SLOT_IDS:
        raise ValueError(f"{obj.name} has no unique supported colour slot")
    return slot


def exported_module_for(obj: bpy.types.Object) -> str:
    """Recover tags which Blender's FBX exporter does not preserve."""
    name = obj.name
    prefixes = (
        ("GEO_Beaker_Shard", "beaker_shards"),
        ("GEO_Beaker", "beaker"),
        ("GEO_Stage", "stage"),
        ("GEO_Water", "water"),
        ("GEO_ReactiveMetalChunk", "metal"),
        ("GEO_MetalFragment", "metal"),
        ("FX_MoltenMetal", "metal"),
        ("FX_Explosion", "explosion"),
        ("FX_SmokeWisp", "explosion"),
        ("FX_WaterEruption", "explosion"),
        ("FX_Flame", "flame"),
        ("FX_Spark", "sparks"),
        ("FX_Bubble", "bubbles"),
        ("FX_Droplet", "splashes"),
        ("FX_Vapour", "vapour"),
        ("FX_AddedSolution", "pour"),
        ("GEO_AddedSolution", "pour"),
        ("FX_PourMixingCurrent", "mixing"),
        ("FX_ConnectedGasPlume", "plume"),
        ("FX_GasBubble", "bubbles"),
        ("FX_SurfaceBubbleBurst", "bursts"),
        ("FX_Ripple", "water"),
        ("GEO_GasEvolutionLiquid", "water"),
        ("GEO_AddedSolidReactant", "input"),
        ("GEO_DisplacementInitialSolution", "initial_solution"),
        ("GEO_FinalSolution", "final_solution"),
        ("GEO_OriginalMetal", "original_metal"),
        ("FX_MetalErosion", "metal_erosion"),
        ("GEO_ReplacementMetalDeposit", "replacement_metal_deposit"),
        ("FX_DepositedMetalFlake", "deposited_metal_flakes"),
        ("GEO_SynthesisCeramicDish", "reaction_vessel"),
        ("GEO_SynthesisSpatula", "mixing_tool"),
    )
    return next(
        (module for prefix, module in prefixes if name.startswith(prefix)),
        "",
    )


def collect_tracks(excluded_modules: set[str], anchor_object: str | None) -> list[Track]:
    tracks = []
    for obj in sorted(bpy.context.scene.objects, key=lambda item: item.name):
        if obj.type != "MESH":
            continue
        module = str(obj.get("asset_module", "")) or exported_module_for(obj)
        if module not in MODULE_IDS or module in excluded_modules:
            continue
        slot = colour_slot_for(obj)
        tracks.append(
            Track(
                source=obj,
                module=MODULE_IDS[module],
                render_pass=PASS_IDS[slot],
                colour_slot=COLOUR_SLOT_IDS[slot],
            )
        )
    if anchor_object is not None:
        source = bpy.context.scene.objects.get(anchor_object)
        if source is None:
            raise ValueError(f"anchor object {anchor_object!r} does not exist")
        tracks.append(
            Track(
                source=source,
                module=MODULE_IDS["vessel_anchor"],
                render_pass=0,
                colour_slot=COLOUR_SLOT_IDS["MAT_Glass"],
                vertex_count=1,
                is_anchor=True,
            )
        )
    if not tracks:
        raise ValueError("scene contains no supported animated mesh tracks")
    return tracks


def sample_track(track: Track, dependency_graph, first_frame: bool) -> None:
    if track.is_anchor:
        position = runtime_axis(track.source.evaluated_get(dependency_graph).matrix_world.translation)
        normal = (0.0, 1.0, 0.0)
        track.samples.extend((*position, *normal))
        for axis in range(3):
            track.minimum[axis] = min(track.minimum[axis], position[axis])
            track.maximum[axis] = max(track.maximum[axis], position[axis])
        return
    evaluated = track.source.evaluated_get(dependency_graph)
    mesh = evaluated.to_mesh(preserve_all_data_layers=False, depsgraph=dependency_graph)
    try:
        mesh.calc_loop_triangles()
        if first_frame:
            track.vertex_count = len(mesh.vertices)
            track.indices = tuple(
                int(index)
                for triangle in mesh.loop_triangles
                for index in triangle.vertices
            )
        elif len(mesh.vertices) != track.vertex_count:
            raise ValueError(f"{track.source.name} changes vertex count across frames")
        elif tuple(
            int(index)
            for triangle in mesh.loop_triangles
            for index in triangle.vertices
        ) != track.indices:
            raise ValueError(f"{track.source.name} changes topology across frames")

        world = evaluated.matrix_world
        normal_matrix = world.to_3x3().inverted_safe().transposed()
        for vertex in mesh.vertices:
            position = runtime_axis(world @ vertex.co)
            normal = runtime_axis((normal_matrix @ vertex.normal).normalized())
            track.samples.extend((*position, *normal))
            for axis in range(3):
                track.minimum[axis] = min(track.minimum[axis], position[axis])
                track.maximum[axis] = max(track.maximum[axis], position[axis])
    finally:
        evaluated.to_mesh_clear()


def quantized_position(value: float, origin: float, scale: float) -> int:
    return max(-32767, min(32767, round((value - origin) / scale)))


def quantized_normal(value: float) -> int:
    return max(-127, min(127, round(value * 127.0)))


def write_track(output, track: Track, frame_count: int) -> None:
    origins = [
        (track.minimum[axis] + track.maximum[axis]) * 0.5 for axis in range(3)
    ]
    scales = [
        max((track.maximum[axis] - track.minimum[axis]) / 65_534.0, 1.0e-8)
        for axis in range(3)
    ]
    output.write(
        struct.pack(
            "<BBBBIIffffff",
            track.module,
            track.render_pass,
            track.colour_slot,
            0,
            track.vertex_count,
            len(track.indices),
            *origins,
            *scales,
        )
    )
    output.write(struct.pack(f"<{len(track.indices)}I", *track.indices))
    expected_values = frame_count * track.vertex_count * 6
    if len(track.samples) != expected_values:
        raise ValueError(f"{track.source.name} has an incomplete sample stream")
    for offset in range(0, len(track.samples), 6):
        position = track.samples[offset : offset + 3]
        normal = track.samples[offset + 3 : offset + 6]
        output.write(
            struct.pack(
                "<hhhbbbB",
                *(
                    quantized_position(position[axis], origins[axis], scales[axis])
                    for axis in range(3)
                ),
                *(quantized_normal(component) for component in normal),
                0,
            )
        )


def main() -> None:
    source, destination, excluded_modules, anchor_object = arguments()
    if source.suffix.lower() == ".fbx":
        bpy.ops.wm.read_factory_settings(use_empty=True)
        bpy.ops.import_scene.fbx(filepath=str(source))
        action_ranges = [action.frame_range[:] for action in bpy.data.actions]
        if not action_ranges:
            raise ValueError("FBX contains no animation actions")
        bpy.context.scene.frame_start = math.floor(
            min(frame_range[0] for frame_range in action_ranges)
        )
        bpy.context.scene.frame_end = math.ceil(
            max(frame_range[1] for frame_range in action_ranges)
        )
    else:
        bpy.ops.wm.open_mainfile(filepath=str(source))
    scene = bpy.context.scene
    frame_start = scene.frame_start
    frame_end = scene.frame_end
    frame_count = frame_end - frame_start + 1
    fps = round(scene.render.fps / scene.render.fps_base)
    tracks = collect_tracks(excluded_modules, anchor_object)
    for frame in range(frame_start, frame_end + 1):
        scene.frame_set(frame)
        dependency_graph = bpy.context.evaluated_depsgraph_get()
        for track in tracks:
            sample_track(track, dependency_graph, frame == frame_start)

    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("wb") as output:
        output.write(MAGIC)
        output.write(struct.pack("<IIII", VERSION, frame_count, fps, len(tracks)))
        for track in tracks:
            write_track(output, track, frame_count)
    print(
        f"baked {len(tracks)} tracks, {frame_count} frames at {fps} FPS "
        f"to {destination}",
        flush=True,
    )
    os._exit(0)


main()
