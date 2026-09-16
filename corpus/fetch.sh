#!/bin/sh
set -eu

corpus_dir=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
sample="$corpus_dir/BBC1-1985-12-28-goodenough.t42"
download=$(mktemp "$corpus_dir/.download.XXXXXX")
trap 'rm -f "$download"' EXIT
trap 'exit 1' HUP INT TERM
curl -fL --retry 3 \
  https://zxnet.co.uk/teletext/emulators/BBC1-1985-12-28-goodenough.t42 \
  -o "$download"
mv "$download" "$sample"
