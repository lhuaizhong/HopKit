"""Render the existing HopKit mark as a macOS icon using only the stdlib."""
from pathlib import Path
import math
import struct
import zlib

icons = Path(__file__).resolve().parents[1] / 'src-tauri/icons'
def png(size):
    def segment(x, y, a, b):
        dx, dy = b[0]-a[0], b[1]-a[1]
        t = max(0, min(1, ((x-a[0])*dx+(y-a[1])*dy)/(dx*dx+dy*dy)))
        return math.hypot(x-a[0]-t*dx, y-a[1]-t*dy) < 1.15
    raw = bytearray()
    for y in range(size):
        raw.append(0)
        for x in range(size):
            px, py = (x+.5)*32/size, (y+.5)*32/size
            rounded = math.hypot(max(abs(px-16)-8,0), max(abs(py-16)-8,0)) < 7
            mark = any(math.hypot(px-a,py-b)<2.8 for a,b in [(16,9),(8,23),(24,23)]) or any(segment(px,py,a,b) for a,b in [((16,9),(8,23)),((16,9),(24,23)),((8,23),(24,23))])
            raw.extend((255,255,255,255) if mark else (51,120,230,255 if rounded else 0))
    def chunk(tag, data):
        return struct.pack('!I',len(data))+tag+data+struct.pack('!I',zlib.crc32(tag+data))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('!2I5B',size,size,8,6,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
# ICNS PNG chunks retain full-resolution artwork without platform tools.
chunks = []
for tag, size in [(b'icp4',16),(b'icp5',32),(b'icp6',64),(b'ic07',128),(b'ic08',256),(b'ic09',512),(b'ic10',1024)]:
    data = png(size)
    chunks.append(tag+struct.pack('!I',len(data)+8)+data)
(icons/'icon.png').write_bytes(png(128))
body = b''.join(chunks)
(icons/'icon.icns').write_bytes(b'icns'+struct.pack('!I',len(body)+8)+body)
print(icons/'icon.icns')
