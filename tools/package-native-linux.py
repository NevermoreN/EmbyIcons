"""Package the Linux x86_64 font dependencies beside libembyicons_core.so.

Use an extracted Ubuntu 22.04 dependency sysroot (glibc <= 2.34). The
destination is the release package's plugins directory. Emby uses its own
glibc, so copying libraries from the build machine is not safe.
"""
import argparse
import pathlib
import shutil
import subprocess
import re
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("destination", type=pathlib.Path)
parser.add_argument("--sysroot", type=pathlib.Path, required=True)
parser.add_argument("--core", type=pathlib.Path, required=True)
parser.add_argument("--archive", type=pathlib.Path, required=True)
parser.add_argument("--skia-license", type=pathlib.Path, required=True)
args = parser.parse_args()
destination = args.destination / "embyicons-native"
destination.mkdir(parents=True, exist_ok=True)
libraries = ["libfreetype.so.6", "libfontconfig.so.1", "libexpat.so.1",
             "libz.so.1", "libbz2.so.1.0", "libpng16.so.16",
             "libbrotlidec.so.1", "libbrotlicommon.so.1", "libuuid.so.1"]
for name in libraries:
    source = args.sysroot / "usr/lib/x86_64-linux-gnu" / name
    if not source.exists():
        source = args.sysroot / "lib/x86_64-linux-gnu" / name
    versions = subprocess.check_output(["readelf", "-V", str(source)], text=True)
    required = re.findall(r"Name: GLIBC_([\d.]+)", versions)
    if any(tuple(map(int, version.split("."))) > (2, 34) for version in required):
        raise SystemExit(f"{name} requires a newer glibc than Emby's 2.34")
    shutil.copyfile(source.resolve(strict=True), destination / name)
licenses = destination / "licenses"
licenses.mkdir(exist_ok=True)
for source in (args.sysroot / "usr/share/doc").glob("*/copyright"):
    shutil.copyfile(source, licenses / (source.parent.name + ".copyright"))
shutil.copyfile(args.skia_license, licenses / "LICENSE_SKIA")
shutil.copyfile(pathlib.Path(__file__).resolve().parent.parent / "native/runtime-linux-x64-dependencies.json",
                destination / "runtime-dependencies.json")
print(f"Packaged {len(libraries)} runtime libraries in {destination}")
shutil.copyfile(args.core, args.destination / "libembyicons_core.so")
args.archive.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(args.archive, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for source in sorted(args.destination.rglob("*")):
        if source.is_file():
            entry = zipfile.ZipInfo(source.relative_to(args.destination).as_posix())
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, source.read_bytes())
print(f"Native bundle: {args.archive}")
