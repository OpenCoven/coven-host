"""Bundle the qualified desktop installers with a local copy of the download guide."""
import argparse
import hashlib
from pathlib import Path
from zipfile import ZIP_STORED, ZipFile

VERSION = "0.1.1"
PREFIX = f"Coven-Host-{VERSION}-"
INSTALLERS = [
    PREFIX + suffix for suffix in (
        "macOS.zip", "Windows-x64.exe", "linux-amd64.deb", "linux-arm64.deb",
        "linux-x86_64.tar.gz", "linux-aarch64.tar.gz",
    )
]


def bundle(assets: Path, guide: Path) -> None:
    html = guide.read_text(encoding="utf-8")
    for name in INSTALLERS:
        if name not in html or not (assets / name).is_file():
            raise ValueError(f"Missing installer or download link for {name}")
    marker = '<meta name="asset-location" content="release">'
    if html.count(marker) != 1:
        raise ValueError("The guide must declare the release asset location once")
    local_guide = html.replace(marker, '<meta name="asset-location" content="offline">')
    for name in INSTALLERS:
        local_guide = local_guide.replace(
            f'href="https://github.com/OpenCoven/coven-host/releases/download/v{VERSION}/{name}"',
            f'href="./{name}"',
        )
    offline = local_guide.encode("utf-8")
    zip_name = PREFIX + "Offline.zip"
    temp = assets / (zip_name + ".part")
    sums = [f"{hashlib.sha256(offline).hexdigest()}  Start-Here.html"]
    try:
        with ZipFile(temp, "w", compression=ZIP_STORED) as archive:
            archive.writestr("Start-Here.html", offline)
            for name in INSTALLERS:
                path = assets / name
                with path.open("rb") as file:
                    digest = hashlib.file_digest(file, "sha256").hexdigest()
                sums.append(f"{digest}  {name}")
                archive.write(path, name)
            archive.writestr("SHA256SUMS", "\n".join(sums) + "\n")
        temp.replace(assets / zip_name)
    finally:
        temp.unlink(missing_ok=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assets", type=Path)
    bundle(parser.parse_args().assets, Path(__file__).with_name("Start-Here.html"))
