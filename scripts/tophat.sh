#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Usage: scripts/tophat.sh [--keep] [--port PORT]

Builds rgent, generates a throwaway tophat blog from tophat/content, and previews it.
The blog is deleted when the preview stops (Ctrl+C), unless --keep is given.

Options:
  --keep         Keep the generated blog after the preview stops
  --port PORT    Port to preview on (defaults to preview.port from the config)
  -h, --help     Show this help
EOF
}

keep=false
port=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep)
            keep=true
            shift
            ;;
        --port)
            [[ $# -ge 2 ]] || { echo "--port needs a value" >&2; exit 2; }
            port="$2"
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tophat_dir="$(mktemp -d "${TMPDIR:-/tmp}/rgent-tophat.XXXXXX")"
site_dir="$tophat_dir/site"
preview_pid=""

cleanup() {
    if [[ -n "$preview_pid" ]]; then
        kill "$preview_pid" 2>/dev/null || true
        wait "$preview_pid" 2>/dev/null || true
    fi

    if [[ "$keep" == true ]]; then
        echo "Kept tophat blog at $site_dir"
    else
        rm -rf "$tophat_dir"
        echo "Removed tophat blog at $site_dir"
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

cargo build --quiet --manifest-path "$repo_root/Cargo.toml"
rgent="$repo_root/target/debug/rgent"

"$rgent" new "$site_dir"
cp -R "$repo_root/tophat/content/." "$site_dir/content/"

config="$site_dir/rgent.toml"
grep -q "^postsperindex = " "$config" || {
    echo "Could not find postsperindex in $config" >&2
    exit 1
}
sed -i.bak "s/^postsperindex = .*/postsperindex = 3/" "$config"
rm -f "$config.bak"

cd "$site_dir"
"$rgent" preview ${port:+--port "$port"} &
preview_pid=$!
wait "$preview_pid"
