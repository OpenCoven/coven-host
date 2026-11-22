"""Build byte-reproducible Linux desktop packages using Python's standard library."""
import argparse
import gzip
import hashlib
import io
import json
import posixpath
import re
import struct
import tarfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
VERSION = "0.1.2"
DESKTOP = """[Desktop Entry]
Type=Application
Name=Coven Host
Comment=Open your synchronized Coven folder
Exec=/usr/bin/coven-host-desktop
Icon=folder
Terminal=false
Categories=Utility;FileTools;
StartupNotify=false
Actions=Diagnostics;

[Desktop Action Diagnostics]
Name=Show diagnostics
Exec=/usr/bin/coven-host-desktop --diagnostics
"""


def archive(files):
    raw = io.BytesIO()
    with tarfile.open(mode="w", fileobj=raw, format=tarfile.USTAR_FORMAT) as tar:
        directories = set()
        for name in files:
            directory = posixpath.dirname(name)
            while directory and directory not in directories:
                directories.add(directory)
                directory = posixpath.dirname(directory)
        for name in sorted(directories):
            item = tarfile.TarInfo(name + "/")
            item.type = tarfile.DIRTYPE
            item.mode = 0o755
            item.uname = item.gname = "root"
            tar.addfile(item)
        for name, (data, mode) in sorted(files.items()):
            item = tarfile.TarInfo(name)
            item.mode, item.size = mode, len(data)
            item.uname = item.gname = "root"
            tar.addfile(item, io.BytesIO(data))
    out = io.BytesIO()
    with gzip.GzipFile(filename="", fileobj=out, mode="wb", compresslevel=6, mtime=0) as gz:
        gz.write(raw.getbuffer())
    return out.getvalue()


def debian(members):
    result = bytearray(b"!<arch>\n")
    for name, data in members:
        result.extend(f"{name + '/':<16}{0:<12}{0:<6}{0:<6}{'100644':<8}{len(data):<10}`\n".encode())
        result.extend(data)
        if len(data) % 2:
            result.extend(b"\n")
    return result


def build(args):
    machine, elf_machine = {"amd64": ("x86_64", 62), "arm64": ("aarch64", 183)}[args.arch]
    if args.native:
        cli = args.native.read_bytes()
    else:
        with tarfile.open(args.package) as package:
            cli = package.extractfile("package/bin/coven").read()
    files = {"LICENSE": (args.coven_license.read_bytes(), 0o644),
             "LICENSE-COVEN-HOST": ((HERE.parent.parent / "LICENSE").read_bytes(), 0o644)}
    digest = hashlib.sha256(cli).hexdigest()
    if digest != args.sha256 or cli[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", cli, 18)[0] != elf_machine:
        raise SystemExit("The executable did not match the required architecture and qualification SHA-256.")
    versions = [tuple(map(int, v.split(b"."))) for v in re.findall(rb"\bGLIBC_(\d+\.\d+(?:\.\d+)?)\x00", cli)]
    libc = ".".join(map(str, max(versions)))
    files.update({"coven": (cli, 0o755), "desktop.py": ((HERE / "desktop.py").read_bytes(), 0o755),
                  "platform.json": (json.dumps(dict(machine=machine, sha256=digest, glibc=libc)).encode(), 0o644)})
    installer = (HERE / "launch.sh").read_bytes()
    portable = {"Coven Host/" + name: item for name, item in files.items()}
    portable["Coven Host/Install Coven Host"] = (installer, 0o755)
    portable["Coven Host/Read me.txt"] = ((HERE / "portable.txt").read_bytes(), 0o644)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"Coven-Host-{VERSION}-linux-{machine}.tar.gz").write_bytes(archive(portable))
    data = {"./usr/lib/coven-host-desktop/" + name: item for name, item in files.items()}
    data["./usr/bin/coven-host-desktop"] = (installer, 0o755)
    data["./usr/share/applications/org.opencoven.CovenHost.desktop"] = (DESKTOP.encode(), 0o644)
    control = f"""Package: coven-host-desktop
Version: {VERSION}
Architecture: {args.arch}
Maintainer: OpenCoven <opencoven@users.noreply.github.com>
Section: utils
Priority: optional
Depends: libc6 (>= {libc}), python3 (>= 3.8), libgcc-s1
Description: Open and synchronize a local Coven folder
 Select Coven Host from Applications to set up your private peer and folder.
"""
    package = debian([("debian-binary", b"2.0\n"), ("control.tar.gz", archive({"./control": (control.encode(), 0o644)})),
                      ("data.tar.gz", archive(data))])
    (args.out / f"Coven-Host-{VERSION}-linux-{args.arch}.deb").write_bytes(package)
    print(f"{machine} glibc {libc}+; CLI SHA-256 {digest}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    binary = parser.add_mutually_exclusive_group(required=True)
    binary.add_argument("--package", type=Path, help="Qualified Linux npm tarball")
    binary.add_argument("--native", type=Path, help="Qualified standalone executable")
    parser.add_argument("--coven-license", type=Path, required=True, help="LICENSE from the pinned Coven source checkout")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--arch", choices=["amd64", "arm64"], default="amd64")
    parser.add_argument("--sha256", required=True, help="Executable SHA-256 from installed qualification")
    build(parser.parse_args())
