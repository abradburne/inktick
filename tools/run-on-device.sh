#!/bin/bash
# Builds inktick in the build machine, copies it, a unit's set and its artwork to the handheld,
# and runs it on the screens in place of DSlot, its output here. DSlot is started again when it
# exits (Menu, Save and quit, Ctrl-C here, or the time limit).
#
#   tools/run-on-device.sh SET [ARTWORK] [-- INKTICK_ARGS...]
#
# SET and ARTWORK are files on the Mac (gnw_dkong.zip, and the artwork's gnw_dkong.zip).
# INKTICK_DEBUG and LIMIT (seconds, default 600) are passed on: LIMIT ends the run however it
# is going. The run is silent (INKTICK_DEBUG gets "mute") unless SOUND=on. Takes the shared device lock (nibOS's tools/device-lock.sh; NIBOS_DIR says where
# nibOS is, ../nibos by default) and refuses while a game is being played. Nothing is
# installed: everything goes to /tmp/inktick-run on the device.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source-path=SCRIPTDIR source=lib.sh
. "$here/lib.sh"

[ $# -ge 1 ] || usage 2
[ "$1" = -h ] && usage
set_file="$1"
shift
art=""
if [ $# -ge 1 ] && [ "$1" != -- ]; then
    art="$1"
    shift
fi
[ "${1:-}" = -- ] && shift
limit="${LIMIT:-600}"

# The build machine shares this disk: the binary comes back through target/, not a pipe (its
# output piped out can end early), and the copy on the device is checked against the build.
bin="$root/target/device/inktick"
mkdir -p "$root/target/device"
in_vm bash -lc "cd '$root' && CARGO_TARGET_DIR=$TARGET_DIR cargo build --release --quiet -p inktick && cp $TARGET_DIR/release/inktick '$bin' && sha256sum $TARGET_DIR/release/inktick" >"$bin.sha256"
on_device "mkdir -p /tmp/inktick-run/artwork && cat > /tmp/inktick-run/inktick && chmod 755 /tmp/inktick-run/inktick" <"$bin"
[ "$(on_device sha256sum /tmp/inktick-run/inktick | cut -d' ' -f1)" = "$(cut -d' ' -f1 "$bin.sha256")" ] ||
    die "the copy on the device is not the build"
remote_set="/tmp/inktick-run/$(basename "$set_file")"
on_device "cat > '$remote_set'" <"$set_file"
if [ -n "$art" ]; then
    art_name=$(basename "$art")
    on_device "cat > '/tmp/inktick-run/artwork/$art_name'" <"$art"
fi

lock="${NIBOS_DIR:-$root/../nibos}/tools/device-lock.sh"
[ -x "$lock" ] || die "no $lock: set NIBOS_DIR to the nibOS repository"
"$lock" game >/dev/null 2>&1 && die "a game is running on the device: try again when it isn't"
"$lock" take "inktick: run-on-device $(basename "$set_file")" || die "the device is in use (above): try again later"
dslot=$(on_device 'systemctl is-active dslot.service || true')
restore() {
    [ "$dslot" = active ] && on_device 'sudo systemctl start dslot.service' >/dev/null 2>&1
    "$lock" drop >/dev/null 2>&1 || true
}
trap restore EXIT
[ "$dslot" = active ] && on_device 'sudo systemctl stop dslot.service'
args=$(printf ' %q' "$@" --saves /tmp/inktick-run/saves "$remote_set")
debug="${INKTICK_DEBUG:-}"
[ "${SOUND:-}" = on ] || debug="mute${debug:+,$debug}"
debug=$(printf '%q' "$debug")
ssh "${ssh_opts[@]}" -t "$DEVICE" \
    "INKTICK_DEBUG=$debug SDL_VIDEODRIVER=kmsdrm SDL_KMSDRM_ATOMIC=0 XDG_RUNTIME_DIR=/run/user/\$(id -u) timeout -s TERM $limit /tmp/inktick-run/inktick$args"
