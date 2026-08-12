#!/usr/bin/env python3
"""Validate Daedalus exact-corner JSONL and run generic free-IPPE closure."""

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

SCHEMA_VERSION = "daedalus.offline-exact-corners/1"
ASSET_SHA256 = "1cc0a3cd1ab05bc9822b616271db3afb64d078e56b9bbf452a8acc6d9bad0a6f"
ORDER = "bl,tl,tr,br"
GUARD_NS = 100_000_000
TOP_LEVEL_FIELDS = {
    "schema_version", "producer_epoch", "frame_seq", "timestamp_ns", "camera",
    "target_id", "target_number", "relative_slot", "armor_label", "visibility",
    "exact_corners_px", "plate_geometry", "motion_uniform", "motion_uniform_guard_ns",
    "distance_m", "velocity_frame", "linear_velocity_world_mps",
    "angular_velocity_world_rad_s", "future_truth_included",
}


def fail(message):
    raise SystemExit(f"verify-corner-label-export: {message}")


def finite_vector(value, length, name):
    if not isinstance(value, list) or len(value) != length:
        fail(f"{name} must have {length} values")
    try:
        numbers = [float(item) for item in value]
    except (TypeError, ValueError):
        fail(f"{name} contains a non-number")
    if not all(math.isfinite(item) for item in numbers):
        fail(f"{name} contains a non-finite value")
    return numbers


def validate_row(row, line_number):
    prefix = f"line {line_number}"
    if not isinstance(row, dict) or set(row) != TOP_LEVEL_FIELDS:
        fail(f"{prefix}: top-level fields do not exactly match schema v1")
    if row.get("schema_version") != SCHEMA_VERSION:
        fail(f"{prefix}: unsupported schema_version")
    identity = tuple(row.get(key) for key in ("producer_epoch", "frame_seq", "timestamp_ns"))
    if any(not isinstance(value, int) or value <= 0 for value in identity):
        fail(f"{prefix}: exposure identity must be nonzero integers")
    if row.get("future_truth_included") is not False:
        fail(f"{prefix}: future_truth_included must be false")
    if row.get("target_number") != 3 or row.get("relative_slot") not in range(4):
        fail(f"{prefix}: schema v1 accepts only target #3 slots 0..3")
    if row.get("motion_uniform_guard_ns") != GUARD_NS:
        fail(f"{prefix}: unexpected motion_uniform_guard_ns")
    if not isinstance(row.get("motion_uniform"), bool):
        fail(f"{prefix}: motion_uniform must be boolean")
    finite_vector(row.get("linear_velocity_world_mps"), 3, f"{prefix} linear velocity")
    finite_vector(row.get("angular_velocity_world_rad_s"), 3, f"{prefix} angular velocity")
    if row.get("velocity_frame") != "ros_odom":
        fail(f"{prefix}: velocity_frame must be ros_odom")
    camera = row.get("camera") or {}
    if set(camera) != {"profile_id", "intrinsics", "image"} or camera.get("profile_id") != "daedalus-camera-1440x1080-v2":
        fail(f"{prefix}: invalid camera profile")
    image = camera.get("image") or {}
    intrinsics = camera.get("intrinsics") or {}
    if set(image) != {"width", "height", "pixel_format"}:
        fail(f"{prefix}: invalid image object")
    if set(intrinsics) != {"model", "fx", "fy", "cx", "cy", "distortion_model", "distortion"}:
        fail(f"{prefix}: invalid intrinsics object")
    if intrinsics.get("model") != "pinhole" or intrinsics.get("distortion_model") != "plumb_bob":
        fail(f"{prefix}: unsupported camera model")
    if intrinsics.get("distortion") != [0.0] * 5 and intrinsics.get("distortion") != [0] * 5:
        fail(f"{prefix}: schema v1 requires zero distortion")
    if image.get("pixel_format") != "rgba32":
        fail(f"{prefix}: image format must be rgba32")
    width, height = image.get("width"), image.get("height")
    if not isinstance(width, int) or not isinstance(height, int) or width <= 0 or height <= 0:
        fail(f"{prefix}: invalid image dimensions")
    k = [intrinsics.get(key) for key in ("fx", "fy", "cx", "cy")]
    if not all(isinstance(value, (int, float)) and math.isfinite(value) for value in k):
        fail(f"{prefix}: invalid effective intrinsics")
    if k[0] <= 0 or k[1] <= 0:
        fail(f"{prefix}: focal lengths must be positive")
    corners = row.get("exact_corners_px")
    if not isinstance(corners, list) or len(corners) != 4:
        fail(f"{prefix}: exact_corners_px must contain four corners")
    corners = [finite_vector(point, 2, f"{prefix} exact corner") for point in corners]
    geometry = row.get("plate_geometry") or {}
    geometry_fields = {
        "source", "asset_sha256", "armor_type", "nominal_width_m", "nominal_height_m",
        "measured_width_m", "measured_height_m", "tilt_from_vertical_deg",
        "object_corners_armor_m", "corner_order",
    }
    if set(geometry) != geometry_fields:
        fail(f"{prefix}: invalid plate_geometry object")
    if geometry.get("source") != "asset_marker_mesh" or geometry.get("armor_type") != "small":
        fail(f"{prefix}: unsupported plate geometry")
    if geometry.get("asset_sha256") != ASSET_SHA256 or geometry.get("corner_order") != ORDER:
        fail(f"{prefix}: asset hash or corner order mismatch")
    if geometry.get("nominal_width_m") != 0.135 or geometry.get("nominal_height_m") != 0.055:
        fail(f"{prefix}: nominal small-armor dimensions changed")
    measured_width = geometry.get("measured_width_m")
    measured_height = geometry.get("measured_height_m")
    tilt = geometry.get("tilt_from_vertical_deg")
    if not isinstance(measured_width, (int, float)) or not 0.1335 <= measured_width <= 0.1341:
        fail(f"{prefix}: asset-derived width is outside the audited range")
    if not isinstance(measured_height, (int, float)) or not 0.0535 <= measured_height <= 0.0542:
        fail(f"{prefix}: asset-derived height is outside the audited range")
    if not isinstance(tilt, (int, float)) or not 14.9 <= tilt <= 15.1:
        fail(f"{prefix}: armor tilt is outside the audited range")
    object_points = geometry.get("object_corners_armor_m")
    if not isinstance(object_points, list) or len(object_points) != 4:
        fail(f"{prefix}: object_corners_armor_m must contain four corners")
    object_points = [finite_vector(point, 3, f"{prefix} object corner") for point in object_points]
    if not isinstance(row.get("distance_m"), (int, float)) or row["distance_m"] <= 0:
        fail(f"{prefix}: distance_m must be positive")
    visibility = row.get("visibility") or {}
    if set(visibility) != {"classification", "scene_hidden", "corners_in_frame", "occlusion_tested"}:
        fail(f"{prefix}: invalid visibility object")
    if visibility.get("classification") not in {"fully_in_frame", "partially_in_frame", "out_of_frame", "scene_hidden"}:
        fail(f"{prefix}: unsupported visibility classification")
    if not isinstance(visibility.get("scene_hidden"), bool) or visibility.get("occlusion_tested") is not False:
        fail(f"{prefix}: visibility must preserve scene-hidden and unknown-occlusion semantics")
    if visibility.get("corners_in_frame") not in range(5):
        fail(f"{prefix}: corners_in_frame must be 0..4")
    return identity, corners, object_points, k, float(row["distance_m"])


def validate_schema_contract(path):
    try:
        schema = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read schema {path}: {error}")
    properties = schema.get("properties") or {}
    geometry = (properties.get("plate_geometry") or {}).get("properties") or {}
    if (
        schema.get("$schema") != "https://json-schema.org/draft/2020-12/schema"
        or set(schema.get("required") or []) != TOP_LEVEL_FIELDS
        or (properties.get("schema_version") or {}).get("const") != SCHEMA_VERSION
        or (properties.get("future_truth_included") or {}).get("const") is not False
        or (properties.get("target_number") or {}).get("const") != 3
        or (properties.get("motion_uniform_guard_ns") or {}).get("const") != GUARD_NS
        or (geometry.get("asset_sha256") or {}).get("const") != ASSET_SHA256
        or (geometry.get("corner_order") or {}).get("const") != ORDER
    ):
        fail(f"schema contract mismatch: {path}")


def default_schema_path():
    root = Path(__file__).resolve().parents[1]
    packaged = root / "schemas" / "offline-exact-corners-v1.schema.json"
    if packaged.is_file():
        return packaged
    return root / "sdk" / "schemas" / "offline-exact-corners-v1.schema.json"


def ippe_closure(corners, object_points, intrinsics, distance_m):
    try:
        import cv2
        import numpy as np
    except ImportError as error:
        fail(f"OpenCV and NumPy are required for free-IPPE closure: {error}")
    obj = np.asarray(object_points, dtype=np.float64)
    img = np.asarray(corners, dtype=np.float64)
    fx, fy, cx, cy = intrinsics
    camera = np.asarray([[fx, 0.0, cx], [0.0, fy, cy], [0.0, 0.0, 1.0]], dtype=np.float64)
    distortion = np.zeros((5, 1), dtype=np.float64)
    result = cv2.solvePnPGeneric(obj, img, camera, distortion, flags=cv2.SOLVEPNP_IPPE)
    if not result[0] or not result[1]:
        fail("generic free-IPPE returned no pose")
    errors = []
    for rvec, tvec in zip(result[1], result[2]):
        reprojected, _ = cv2.projectPoints(obj, rvec, tvec, camera, distortion)
        delta = reprojected.reshape((-1, 2)) - img
        errors.append(float(np.sqrt(np.mean(np.sum(delta * delta, axis=1)))))
    pixel_rms = min(errors)
    equivalent_m = pixel_rms * distance_m / ((fx + fy) * 0.5)
    return pixel_rms, equivalent_m


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("jsonl", type=Path)
    parser.add_argument("--tcp-identities", type=Path, help="optional JSONL containing received TCP identity triples")
    parser.add_argument("--asset", type=Path, default=Path(__file__).resolve().parents[1] / "assets" / "vehicle.glb")
    parser.add_argument("--schema", type=Path, default=default_schema_path())
    parser.add_argument("--require-complete-z4", action="store_true")
    parser.add_argument("--require-uniform-and-excluded", action="store_true")
    # The real marker mesh is intentionally preserved, including its audited
    # 4--5 um non-coplanarity. A generic planar IPPE solve is therefore an
    # independent closure check, not an exact algebraic inverse at grazing
    # views. 0.025 px covers the observed real-asset bound while remaining
    # sub-pixel by a wide margin; callers may request a tighter value.
    parser.add_argument("--max-reprojection-px", type=float, default=0.025)
    parser.add_argument("--max-equivalent-error-m", type=float, default=1.25e-4)
    args = parser.parse_args()
    if not args.jsonl.is_file():
        fail(f"missing JSONL: {args.jsonl}")
    if hashlib.sha256(args.asset.read_bytes()).hexdigest() != ASSET_SHA256:
        fail(f"small-armor asset hash mismatch: {args.asset}")
    validate_schema_contract(args.schema)
    tcp_identities = None
    if args.tcp_identities:
        tcp_identities = set()
        for line in args.tcp_identities.read_text(encoding="utf-8-sig").splitlines():
            if line.strip():
                value = json.loads(line)
                tcp_identities.add(tuple(value[key] for key in ("producer_epoch", "frame_seq", "timestamp_ns")))
    rows = 0
    identities = set()
    slots = set()
    uniform_rows = 0
    max_pixel_rms = 0.0
    max_equivalent_m = 0.0
    exposure_slots = {}
    excluded_rows = 0
    with args.jsonl.open("r", encoding="utf-8-sig") as handle:
        for line_number, line in enumerate(handle, 1):
            if not line.strip():
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as error:
                fail(f"line {line_number}: invalid JSON: {error}")
            identity, corners, object_points, intrinsics, distance_m = validate_row(row, line_number)
            if tcp_identities is not None and identity not in tcp_identities:
                fail(f"line {line_number}: label identity has no received TCP image")
            unique = identity + (row["target_id"], row["relative_slot"])
            if unique in slots:
                fail(f"line {line_number}: duplicate exposure/target/slot")
            slots.add(unique)
            identities.add(identity)
            if row.get("motion_uniform") is True:
                uniform_rows += 1
            else:
                excluded_rows += 1
            exposure_slots.setdefault(identity, set()).add(row["relative_slot"])
            pixel_rms, equivalent_m = ippe_closure(corners, object_points, intrinsics, distance_m)
            max_pixel_rms = max(max_pixel_rms, pixel_rms)
            max_equivalent_m = max(max_equivalent_m, equivalent_m)
            rows += 1
    if rows == 0:
        fail("JSONL contains no label rows")
    if max_pixel_rms > args.max_reprojection_px:
        fail(f"free-IPPE reprojection RMS {max_pixel_rms:.9g}px exceeds {args.max_reprojection_px}px")
    if max_equivalent_m > args.max_equivalent_error_m:
        fail(f"free-IPPE equivalent error {max_equivalent_m:.9g}m exceeds {args.max_equivalent_error_m}m")
    if args.require_complete_z4 and any(slots != {0, 1, 2, 3} for slots in exposure_slots.values()):
        fail("at least one exposure does not contain exactly the four Z4 relative slots")
    if args.require_uniform_and_excluded and (uniform_rows == 0 or excluded_rows == 0):
        fail("capture must contain both certified-uniform and motion-excluded rows")
    print(
        "corner_label_export_ok "
        f"rows={rows} exposures={len(identities)} uniform_rows={uniform_rows} excluded_rows={excluded_rows} "
        f"max_ippe_rms_px={max_pixel_rms:.9g} max_ippe_equivalent_m={max_equivalent_m:.9g}"
    )


if __name__ == "__main__":
    main()
