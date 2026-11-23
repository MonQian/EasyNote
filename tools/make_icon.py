"""Generate the small app and tray icons without external dependencies."""
import struct
import zlib
from pathlib import Path

size = 64
pixels = []
for y in range(size):
    row = bytearray()
    for x in range(size):
        if 5 <= x < 59 and 5 <= y < 59:
            color = (255, 239, 166, 255) if y < 48 else (242, 211, 121, 255)
            if 15 <= x < 49 and y in (21, 30, 39):
                color = (88, 76, 55, 255)
        else:
            color = (0, 0, 0, 0)
        row.extend(color)
    pixels.append(b"\0" + row)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", size, size, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(b"".join(pixels))) + chunk(b"IEND", b"")
icons = Path(__file__).resolve().parents[1] / "src-tauri" / "icons"
icons.mkdir(parents=True, exist_ok=True)
(icons / "icon.png").write_bytes(png)
(icons / "icon.ico").write_bytes(struct.pack("<HHHBBBBHHII", 0, 1, 1, size, size, 0, 0, 1, 32, len(png), 22) + png)
