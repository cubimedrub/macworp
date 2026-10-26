#!/bin/sh
# End-to-end test image, does the same as ../workflow/main.nf.
# Receives `--name value` pairs. Params are the fixed_* and dyn_* sets plus `--token`.
#
# * writes every parameter value into results/params.txt
# * overwrites every given file with the test token
# * replaces `old.txt` in every given folder with `<token>.marker`
# * overwrites every file matched by the glob with the test token
set -eu

values=$(mktemp -d)
while [ $# -gt 0 ]; do
    case "$1" in
        --*)
            [ $# -ge 2 ] || { echo "missing value for $1" >&2; exit 2; }
            printf '%s' "$2" > "$values/${1#--}"
            shift 2
            ;;
        *)
            echo "unexpected argument: $1" >&2
            exit 2
            ;;
    esac
done

get() { cat "$values/$1"; }

token=$(get token)
[ -n "$token" ] || { echo "token is required" >&2; exit 1; }

report=$(mktemp)
line() { printf '%s\n' "$1" >> "$report"; }

for prefix in fixed dyn; do
    glob=$(get "${prefix}_glob")
    matches=""
    for f in $glob; do
        [ -e "$f" ] || continue
        printf '%s' "$token" > "$f"
        matches="${matches:+$matches,}$(basename "$f")"
    done

    IFS=,
    for p in $(get "${prefix}_file") $(get "${prefix}_files"); do
        printf '%s' "$token" > "$p"
    done
    for p in $(get "${prefix}_folder") $(get "${prefix}_folders"); do
        rm -f "$p/old.txt"
        printf '%s' "$token" > "$p/$token.marker"
    done
    unset IFS

    line "${prefix}_text => $(get "${prefix}_text")"
    line "${prefix}_multiline => $(get "${prefix}_multiline")"
    line "${prefix}_integer => $(get "${prefix}_integer")"
    line "${prefix}_float => $(get "${prefix}_float")"
    line "${prefix}_glob => $glob"
    line "${prefix}_glob_matches => $matches"
    line "${prefix}_file => $(get "${prefix}_file")"
    line "${prefix}_files => $(get "${prefix}_files")"
    line "${prefix}_folder => $(get "${prefix}_folder")"
    line "${prefix}_folders => $(get "${prefix}_folders")"
    line "${prefix}_select => $(get "${prefix}_select")"
    line "${prefix}_multiselect => $(get "${prefix}_multiselect")"
done
line "token => $token"

mkdir -p results
cp "$report" results/params.txt
echo "wrote results/params.txt"
# The container may run as root, keep the project removable by the host user
chmod -R a+rwX .
