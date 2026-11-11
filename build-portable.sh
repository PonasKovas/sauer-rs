#!/usr/bin/env bash
# Builds a deathrun server binary that runs on any x86_64 Linux with glibc
# 2.28 or newer (Debian 10+, Ubuntu 18.10+, RHEL 8+), not just this machine.
#
#   ./build-portable.sh            -> dist/deathrun_server
#   GLIBC=2.31 ./build-portable.sh -> target a different minimum glibc
#
# Uses cargo-zigbuild, which links against stubs of the chosen glibc and the
# standard /lib64 loader instead of the local (NixOS) one. Zig comes from
# nixpkgs, the Rust toolchain from shell.nix.
set -euo pipefail
cd "$(dirname "$0")"

GLIBC="${GLIBC:-2.28}"
TRIPLE=x86_64-unknown-linux-gnu
TARGET="$TRIPLE.$GLIBC"
export CARGO_TARGET_DIR="$PWD/target/portable"
DEPS="$CARGO_TARGET_DIR/$TRIPLE/release/deps"

ZIG_PATH=$(nix-build '<nixpkgs>' -A zig -A cargo-zigbuild --no-out-link | sed 's|$|/bin|' | paste -sd:)

zigbuild() {
    nix-shell --run "PATH=$ZIG_PATH:\$PATH cargo zigbuild --release --target $TARGET $*"
}

build_server() {
    zigbuild -p deathrun --example server
}

# Lenet's archive bundles LLVM's libc++, whose objects carry .deplibs notes
# ("also link pthread/rt"). Zig's linker can't resolve those, so strip them;
# the real libraries are on the link line anyway. Building lenet-sys on its
# own resolves features differently and gets rebuilt later, so instead let
# the first server build fail at the link, fix the archive it produced, and
# link again.
strip_deplibs() {
    local work changed=1
    work=$(mktemp -d)
    for rlib in "$DEPS"/liblenet_sys-*.rlib; do
        (cd "$work" && rm -f lenet.o && ar x "$rlib" lenet.o)
        if readelf -S "$work/lenet.o" | grep '\.deplibs' >/dev/null; then
            objcopy --remove-section=.deplibs "$work/lenet.o"
            (cd "$work" && ar r "$rlib" lenet.o)
            changed=0
        fi
    done
    rm -rf "$work"
    return $changed
}

if ! build_server; then
    strip_deplibs || { echo "build failed, and not because of .deplibs" >&2; exit 1; }
    echo "Stripped .deplibs from the Lenet archive, linking again."
    build_server
fi

# Ship it without debug info (the release profile keeps it).
mkdir -p dist
objcopy --strip-debug "$CARGO_TARGET_DIR/$TRIPLE/release/examples/server" dist/deathrun_server
echo "Built dist/deathrun_server (glibc >= $GLIBC):"
objdump -T dist/deathrun_server | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1
