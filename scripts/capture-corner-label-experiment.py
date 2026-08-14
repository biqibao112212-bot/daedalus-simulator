#!/usr/bin/env python3
"""Collect real TCP frames and identity evidence for exact-corner export."""

import argparse
import errno
import hashlib
import json
import socket
import struct
import time
import uuid
from pathlib import Path


PROTOCOL = "daedalus.scene-control/2"
TCP_MAGIC = 0x54494D47
TCP_VERSION = 1
TCP_HEADER_BYTES = 64
TCP_HEADER = struct.Struct(">IHHHHIIIQQQQQ")
FRAME_CAPTURE_SCHEMA = "daedalus.offline-frame-capture/1"


class StreamEnded(Exception):
    pass


def fail(message):
    raise SystemExit(f"capture-corner-label-experiment: {message}")


def recv_exact(stream, size):
    chunks = []
    received = 0
    while received < size:
        chunk = stream.recv(size - received)
        if not chunk:
            raise StreamEnded(f"TCP image stream closed after {received}/{size} bytes")
        chunks.append(chunk)
        received += len(chunk)
    return b"".join(chunks)


def scene_request(sock, address, session_id, command_id, op, args, timeout_s):
    request = {
        "protocol": PROTOCOL,
        "command_id": command_id,
        "session_id": session_id,
        "op": op,
        "args": args,
    }
    deadline = time.monotonic() + timeout_s
    payload = json.dumps(request, separators=(",", ":")).encode("utf-8")
    pending_scene_change = False
    while time.monotonic() < deadline:
        if not pending_scene_change:
            sock.sendto(payload, address)
        retry_deadline = min(deadline, time.monotonic() + 0.5)
        while time.monotonic() < retry_deadline:
            sock.settimeout(max(0.01, retry_deadline - time.monotonic()))
            try:
                response_bytes, _ = sock.recvfrom(65507)
            except socket.timeout:
                break
            except OSError as error:
                # On Windows a UDP request sent before the simulator binds can
                # surface the delayed ICMP port-unreachable as WSAECONNRESET
                # or WSAECONNREFUSED on recvfrom. Treat those as readiness
                # retries; all other socket failures remain fatal.
                if getattr(error, "winerror", None) in (10054, 10061) or error.errno in (
                    errno.ECONNRESET,
                    errno.ECONNREFUSED,
                ):
                    break
                raise
            response = json.loads(response_bytes.decode("utf-8"))
            if response.get("command_id") != command_id or response.get("session_id") != session_id:
                continue
            if response.get("protocol") != PROTOCOL:
                fail(f"scene-control {op} failed: {response}")
            if response.get("status") == "not_ready" and op == "set_scene" and "pending" in str(
                response.get("message", "")
            ):
                # The original asynchronous set_scene request is still owned
                # by the simulator. Stop retransmitting and wait for its final
                # response with the same identity.
                pending_scene_change = True
                continue
            if response.get("status") != "ok":
                fail(f"scene-control {op} failed: {response}")
            return response
    fail(f"scene-control {op} timed out after {timeout_s:.1f}s")


def connect_with_retry(address, timeout_s):
    deadline = time.monotonic() + timeout_s
    last_error = None
    while time.monotonic() < deadline:
        stream = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        stream.settimeout(2.0)
        try:
            stream.connect(address)
            stream.settimeout(5.0)
            return stream
        except OSError as error:
            last_error = error
            stream.close()
            time.sleep(0.1)
    fail(f"cannot connect to TCP image stream {address}: {last_error}")


def decode_header(wire):
    (
        magic,
        version,
        header_bytes,
        pixel_format,
        flags,
        width,
        height,
        payload_bytes,
        producer_epoch,
        frame_seq,
        timestamp_ns,
        reserved0,
        reserved1,
    ) = TCP_HEADER.unpack(wire)
    channels = {1: 3, 2: 4}.get(pixel_format)
    if (
        magic != TCP_MAGIC
        or version != TCP_VERSION
        or header_bytes != TCP_HEADER_BYTES
        or channels is None
        or flags != 0
        or width <= 0
        or height <= 0
        or width > 1440
        or height > 1080
        or payload_bytes != width * height * channels
        or producer_epoch <= 0
        or frame_seq <= 0
        or timestamp_ns <= 0
        or reserved0 != 0
        or reserved1 != 0
    ):
        fail("invalid TCP image header")
    return {
        "producer_epoch": producer_epoch,
        "frame_seq": frame_seq,
        "timestamp_ns": timestamp_ns,
        "width": width,
        "height": height,
        "payload_bytes": payload_bytes,
        "pixel_format": "rgba32" if pixel_format == 2 else "rgb24",
    }


def write_new_bytes(path, payload):
    """Write a protected payload once, without ever replacing a prior capture."""
    with path.open("xb") as handle:
        handle.write(payload)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True, help="existing protected session directory")
    parser.add_argument("--frames", type=int, default=60, help="fixed-frame TCP smoke only; use --until-eof for strict label identity evidence")
    parser.add_argument("--until-eof", action="store_true", help="drain complete frames until the simulator closes the connection")
    parser.add_argument("--tcp-host", default="127.0.0.1")
    parser.add_argument("--tcp-port", type=int, default=5602)
    parser.add_argument("--scene-port", type=int, default=5603)
    parser.add_argument("--connect-timeout", type=float, default=20.0)
    parser.add_argument("--motion-mode", choices=("stationary", "linear", "spin", "linear_and_spin"), default="linear")
    parser.add_argument("--linear-speed-mps", type=float, default=1.0)
    parser.add_argument("--linear-span-m", type=float, default=8.0)
    parser.add_argument("--spin-deg-s", type=float, default=0.0)
    parser.add_argument("--direction-deg", type=float, default=90.0)
    parser.add_argument("--save-first-rgba", action="store_true")
    parser.add_argument(
        "--save-rgba-frames",
        action="store_true",
        help="write one protected raw RGBA32 frame per TCP identity; requires --until-eof",
    )
    args = parser.parse_args()
    if not args.until_eof and args.frames <= 0:
        fail("--frames must be positive")
    if args.save_rgba_frames and not args.until_eof:
        fail("--save-rgba-frames requires --until-eof for complete identity evidence")
    if args.save_rgba_frames and args.save_first_rgba:
        fail("--save-rgba-frames already retains the first frame; do not also pass --save-first-rgba")
    output_dir = args.output_dir.resolve()
    if not output_dir.is_dir():
        fail(f"output directory must already exist: {output_dir}")
    identities_path = output_dir / "tcp-identities.jsonl"
    if identities_path.exists():
        fail(f"refusing to overwrite protected identity evidence: {identities_path}")
    rgba_path = output_dir / "first-frame.rgba"
    if args.save_first_rgba and rgba_path.exists():
        fail(f"refusing to overwrite protected raw frame: {rgba_path}")
    frames_dir = output_dir / "frames"
    manifest_path = output_dir / "capture-manifest.json"
    if args.save_rgba_frames and frames_dir.exists():
        fail(f"refusing to reuse protected frame directory: {frames_dir}")
    if args.save_rgba_frames and manifest_path.exists():
        fail(f"refusing to overwrite protected capture manifest: {manifest_path}")
    if args.save_rgba_frames:
        frames_dir.mkdir()

    session_id = f"corner-label-{uuid.uuid4().hex}"
    scene_address = (args.tcp_host, args.scene_port)
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as control:
        scene_request(control, scene_address, session_id, 1, "create_session", {}, args.connect_timeout)
        scene_request(
            control,
            scene_address,
            session_id,
            2,
            "set_scene",
            {"scene": "shooting_range"},
            args.connect_timeout,
        )
        scene_request(
            control,
            scene_address,
            session_id,
            3,
            "set_range_target_motion",
            {
                "target": 3,
                "mode": args.motion_mode,
                "direction_deg": args.direction_deg,
                "linear_speed_mps": args.linear_speed_mps,
                "linear_span_m": args.linear_span_m,
                "spin_deg_s": args.spin_deg_s,
            },
            args.connect_timeout,
        )

    stream = connect_with_retry((args.tcp_host, args.tcp_port), args.connect_timeout)
    ready_path = output_dir / "collector-ready"
    if ready_path.exists():
        fail(f"refusing to overwrite collector readiness evidence: {ready_path}")
    ready_path.write_text("connected\n", encoding="utf-8")
    first_epoch = None
    last_seq = 0
    received_frames = 0
    with stream, identities_path.open("x", encoding="utf-8", newline="\n") as identities:
        while args.until_eof or received_frames < args.frames:
            try:
                header = decode_header(recv_exact(stream, TCP_HEADER_BYTES))
                payload = recv_exact(stream, header["payload_bytes"])
            except StreamEnded as error:
                if args.until_eof:
                    break
                fail(str(error))
            except OSError as error:
                if args.until_eof and (
                    getattr(error, "winerror", None) in (10054, 10061)
                    or error.errno in (errno.ECONNRESET, errno.ECONNREFUSED)
                ):
                    break
                raise
            if header["pixel_format"] != "rgba32":
                fail("exact-corner experiment requires the real RGBA32 TCP stream")
            if first_epoch is None:
                first_epoch = header["producer_epoch"]
            if header["producer_epoch"] != first_epoch or header["frame_seq"] <= last_seq:
                fail("TCP producer epoch changed or frame sequence did not increase")
            last_seq = header["frame_seq"]
            header["payload_sha256"] = hashlib.sha256(payload).hexdigest()
            if args.save_rgba_frames:
                name = "{}_{}_{}.rgba".format(
                    header["producer_epoch"], header["frame_seq"], header["timestamp_ns"]
                )
                destination = frames_dir / name
                write_new_bytes(destination, payload)
                header["raw_rgba_file"] = str(Path("frames") / name)
                header["raw_rgba_sha256"] = hashlib.sha256(destination.read_bytes()).hexdigest()
            identities.write(json.dumps(header, separators=(",", ":")) + "\n")
            identities.flush()
            if received_frames == 0 and args.save_first_rgba:
                rgba_path.write_bytes(payload)
            received_frames += 1

    if received_frames == 0:
        fail("no complete TCP frames were received")
    if args.save_rgba_frames:
        with manifest_path.open("x", encoding="utf-8", newline="\n") as handle:
            json.dump(
                {
                    "schema_version": FRAME_CAPTURE_SCHEMA,
                    "capture_mode": "until_eof",
                    "frame_count": received_frames,
                    "producer_epoch": first_epoch,
                    "last_frame_seq": last_seq,
                    "image_format": "rgba32-raw",
                    "frame_directory": "frames",
                    "identity_ledger": "tcp-identities.jsonl",
                    "online_truth_read": False,
                    "future_truth_included": False,
                },
                handle,
                indent=2,
            )
            handle.write("\n")

    print(
        f"corner_label_capture_ok frames={received_frames} producer_epoch={first_epoch} "
        f"last_frame_seq={last_seq} identities={identities_path} "
        f"full_frames={args.save_rgba_frames}"
    )


if __name__ == "__main__":
    main()
