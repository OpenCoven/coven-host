"""Bundle the qualified desktop installers with a local copy of the download guide."""
import argparse
import hashlib
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from zipfile import ZipFile, ZipInfo

VERSION = "0.1.1"
PREFIX = f"Coven-Host-{VERSION}-"
INSTALLERS = [
    PREFIX + suffix for suffix in (
        "macOS.zip", "Windows-x64.exe", "linux-amd64.deb", "linux-arm64.deb",
        "linux-x86_64.tar.gz", "linux-aarch64.tar.gz",
    )
]


def zip_entry(name: str) -> ZipInfo:
    # Use the ZIP epoch and portable file permissions regardless of the build host.
    entry = ZipInfo(name)
    entry.create_system = 3  # Unix
    entry.external_attr = 0o100644 << 16
    return entry


def bundle(assets: Path, guide: Path) -> None:
    html = guide.read_text(encoding="utf-8")
    for name in INSTALLERS:
        if name not in html or not (assets / name).is_file():
            raise ValueError(f"Missing installer or download link for {name}")
    marker = '<html lang="en" data-distribution="release">'
    if html.count(marker) != 1:
        raise ValueError("The guide must declare the release asset location once")
    local_guide = html.replace(marker, '<html lang="en" data-distribution="offline">')
    for name in INSTALLERS:
        local_guide = local_guide.replace(
            f'href="https://github.com/OpenCoven/coven-host/releases/download/v{VERSION}/{name}"',
            f'href="./{name}"',
        )
    offline = local_guide.encode("utf-8")
    zip_name = PREFIX + "Offline.zip"
    sums = [f"{hashlib.sha256(offline).hexdigest()}  Start-Here.html"]
    with TemporaryDirectory(dir=assets, prefix=".offline-kit-") as staging:
        temp = Path(staging) / zip_name
        with ZipFile(temp, "w") as archive:
            archive.writestr(zip_entry("Start-Here.html"), offline)
            for name in INSTALLERS:
                digest = hashlib.sha256()
                with (assets / name).open("rb") as source:
                    entry = zip_entry(name)
                    entry.file_size = os.fstat(source.fileno()).st_size
                    with archive.open(entry, "w") as member:
                        while chunk := source.read(1024 * 1024):
                            member.write(chunk)
                            digest.update(chunk)
                sums.append(f"{digest.hexdigest()}  {name}")
            archive.writestr(zip_entry("SHA256SUMS"), "\n".join(sums) + "\n")
        temp.replace(assets / zip_name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assets", type=Path)
    bundle(parser.parse_args().assets, Path(__file__).with_name("Start-Here.html"))
