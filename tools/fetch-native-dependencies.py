"""Download and extract the pinned runtime packages without installing them."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("destination", type=pathlib.Path)
args = parser.parse_args()
args.destination.mkdir(parents=True, exist_ok=True)
manifest = pathlib.Path(__file__).resolve().parent.parent / "native/runtime-linux-x64-dependencies.json"
for package in json.loads(manifest.read_text()):
    with urllib.request.urlopen(package["url"], timeout=60) as response:
        content = response.read()
    if hashlib.sha256(content).hexdigest() != package["sha256"]:
        raise SystemExit("Package checksum mismatch: " + package["package"])
    archive = args.destination / (package["package"] + ".deb")
    try:
        archive.write_bytes(content)
        subprocess.run(["dpkg-deb", "-x", str(archive), str(args.destination / "sysroot")], check=True)
    finally:
        archive.unlink(missing_ok=True)
    print(package["package"], package["version"])
