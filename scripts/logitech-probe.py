"""Inspect HID++ 2.0 features without changing mouse settings (pip install hidapi)."""
import time

import hid


def query(device, index, feature, function, data=b""):
    header = bytes([0x11, index, feature, function << 4 | 0xE])
    packet = header + data.ljust(16, b"\0")
    device.write(packet)
    deadline = time.monotonic() + 0.45
    while (remaining := deadline - time.monotonic()) > 0:
        reply = bytes(device.read(64, max(1, int(1000 * remaining))))
        if len(reply) >= 7 and reply[1:4] == header[1:4]:
            return reply[4:]
        if len(reply) >= 7 and reply[1] == index and reply[2] in (0x8F, 0xFF) and reply[3:5] == header[2:4]:
            return None
    return None


def inspect(device, index):
    feature_set = query(device, index, 0, 0, b"\x00\x01")
    if not feature_set or not feature_set[0]:
        return
    count = query(device, index, feature_set[0], 0)
    if not count:
        return
    features = {}
    for number in range(1, count[0] + 1):
        result = query(device, index, feature_set[0], 1, bytes([number]))
        if result:
            features[int.from_bytes(result[:2], "big")] = number
    print("  index", index, "features", " ".join(f"{f:04X}" for f in features))
    if 5 in features:
        length = query(device, index, features[5], 0)
        name = bytearray()
        while length and len(name) < length[0]:
            chunk = query(device, index, features[5], 1, bytes([len(name)]))
            if not chunk:
                break
            name.extend(chunk)
        print("  name", name[:length[0]].decode(errors="replace") if length else "?")
    if 0x8100 in features:
        for fn, label in [(0, "descriptor"), (2, "mode"), (4, "active profile")]:
            result = query(device, index, features[0x8100], fn)
            print(" ", label, result.hex(" ") if result else "no reply")


if __name__ == "__main__":
    for entry in hid.enumerate(0x046D, 0):
        if entry["usage_page"] != 0xFF00 or entry["usage"] != 2:
            continue
        print(f"046D:{entry['product_id']:04X}", entry["product_string"])
        device = hid.device()
        try:
            device.open_path(entry["path"])
            for idx in ([0xFF] if entry["product_id"] == 0xC088 else range(1, 7)):
                inspect(device, idx)
        except OSError as exc:
            print(" ", exc)
        finally:
            device.close()
