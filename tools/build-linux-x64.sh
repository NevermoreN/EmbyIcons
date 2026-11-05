#!/usr/bin/env bash
set -euo pipefail

task_repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
task_output="${1:-$task_repo/artifacts/linux-x64}"
mkdir -p "$task_output"
task_output="$(cd "$task_output" && pwd)"
task_scratch="$(mktemp -d -t embyicons-build-XXXXXXXX)"
trap 'rm -rf "$task_scratch"' EXIT
cd "$task_repo"

python3 tools/fetch-native-dependencies.py "$task_scratch/deps"
mkdir "$task_scratch/link"
for task_lib in freetype fontconfig; do
    task_soname=6
    if [ "$task_lib" = fontconfig ]; then task_soname=1; fi
    ln -s "$task_scratch/deps/sysroot/usr/lib/x86_64-linux-gnu/lib$task_lib.so.$task_soname" \
        "$task_scratch/link/lib$task_lib.so"
done
LIBRARY_PATH="$task_scratch/link${LIBRARY_PATH:+:$LIBRARY_PATH}" \
    CARGO_TARGET_DIR="$task_scratch/native" cargo build --release --locked \
    --manifest-path native/embyicons-core/Cargo.toml
task_skia_licenses=("$task_scratch"/native/release/build/skia-bindings-*/out/skia/LICENSE_SKIA)
python3 tools/package-native-linux.py "$task_scratch/runtime" \
    --sysroot "$task_scratch/deps/sysroot" \
    --core "$task_scratch/native/release/libembyicons_core.so" \
    --archive "$task_scratch/runtime-linux-x64.zip" \
    --skia-license "${task_skia_licenses[0]}"
dotnet build EmbyIcons.csproj -c Release \
    -p:NativeBundlePath="$task_scratch/runtime-linux-x64.zip" \
    --artifacts-path "$task_scratch/build"
cp "$task_scratch/build/bin/EmbyIcons/release/EmbyIcons.dll" "$task_output/embyicons.dll"
sha256sum "$task_output/embyicons.dll"
