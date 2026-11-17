#!/usr/bin/env bash
# Install Ubuntu packages on a CI runner, through other mirrors when the
# runner's own cannot be reached (the 0.11.0 release builds failed for an
# hour on "Temporary failure resolving 'azure.archive.ubuntu.com'" and
# 'ports.ubuntu.com').
#
# Usage: packaging/linux/apt-install.sh <package>...
# Each attempt updates the indexes and installs; after a failed attempt the
# apt sources are pointed at the next mirror of the list (amd64 uses the
# Ubuntu archive, other architectures the ports archive).
set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <package>..." >&2
    exit 2
fi

if [ "$(dpkg --print-architecture)" = amd64 ]; then
    mirrors=(
        https://mirror.us.leaseweb.net/ubuntu
        https://mirrors.ocf.berkeley.edu/ubuntu
    )
else
    mirrors=(
        https://mirror.us.leaseweb.net/ubuntu-ports
        https://mirrors.ocf.berkeley.edu/ubuntu-ports
        https://ftp.tu-chemnitz.de/pub/linux/ubuntu-ports
    )
fi

apt=(sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30)

attempt() {
    "${apt[@]}" update && "${apt[@]}" install -y --no-install-recommends "$@"
}

# Point every Ubuntu archive URL of the apt sources (sources.list, the
# sources.list.d files and the runner's apt-mirrors.txt) at `$1`.
use_mirror() {
    local files=()
    for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources \
        /etc/apt/apt-mirrors.txt; do
        [ -f "$f" ] && files+=("$f")
    done
    [ "${#files[@]}" -gt 0 ] || return 0
    # The Ubuntu archives, and the mirror a previous attempt switched to.
    local script="s#https?://([a-z0-9-]+\.)*(archive|security|ports)\.ubuntu\.com/ubuntu(-ports)?/?#$1/#g"
    if [ -n "$current" ]; then
        script+="; s#${current//./\\.}/?#$1/#g"
    fi
    sudo sed -E -i "$script" "${files[@]}"
    current=$1
    echo "apt sources now use $1"
}

current=
if attempt "$@"; then
    exit 0
fi
for mirror in "${mirrors[@]}"; do
    echo "apt failed; trying $mirror" >&2
    use_mirror "$mirror"
    sleep 5
    if attempt "$@"; then
        exit 0
    fi
done
echo "apt could not install: $*" >&2
exit 1
