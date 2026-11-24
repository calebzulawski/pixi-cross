# Build for ARM64 from the current Pixi workspace platform.
set -euo pipefail
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

# Keep the example's relative channel path and start with a fresh lock and prefixes.
example="$work/examples/openssl"
mkdir -p "$example/.pixi-cross"
cp examples/openssl/{pixi.toml,Makefile,openssl-version.cpp} "$example/"
cp examples/openssl/.pixi-cross/pixi.toml "$example/.pixi-cross/"
"$PIXI_EXE" run --locked packages "$work/target/channel"
mkdir -p "$work/recipes" "$work/target/release"
cp -r recipes/pixi-cross "$work/recipes/"
cp target/release/pixi-cross "$work/target/release/"

cd "$example"
"$PIXI_EXE" run cross sync
"$PIXI_EXE" run --locked -e dev-linux-aarch64 build
"$PIXI_EXE" run --locked -e dev-linux-aarch64 sh -ec '
    "$BUILD_PREFIX/bin/pixi-cross" --version
    test ! -e "$PREFIX/bin/pixi-cross"
    LC_ALL=C "$READELF" -h openssl-version-linux-aarch64
' | grep 'Machine:.*AArch64'

# Switching targets inside pixi run must discard the outer compiler's include path.
CXXFLAGS=-DUSER_FLAG "$PIXI_EXE" run --locked -e dev-linux-aarch64 \
    "$PIXI_EXE" run --locked -e dev-linux-64 bash -ec '
    [[ $target_platform == linux-64 ]]
    [[ $CXXFLAGS == *"$PREFIX/include"* && $CXXFLAGS != *"/linux-aarch64/include"* ]]
    . "$CONDA_PREFIX/etc/conda/deactivate.d/000-pixi-cross.sh"
    [[ $CXXFLAGS == -DUSER_FLAG ]]
'
