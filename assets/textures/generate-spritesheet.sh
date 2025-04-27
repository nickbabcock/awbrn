#!/bin/bash

set -euo pipefail

DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
cd "$DIR"

# Fill in with the path to AWBW-Replay-Player/AWBWApp.Resources/Textures
TEXTURES_DIR="${TEXTURES_DIR:-.}"

cd "$TEXTURES_DIR/Map/AW2"
montage -tile 64x -mode concatenate -gravity southeast -geometry '16x32>' -background transparent \
  $(cd "$DIR" && cargo spritesheet-list "$TEXTURES_DIR/Map/AW2") "$DIR/tiles.png"

cd "$TEXTURES_DIR"
montage -tile 64x -mode concatenate -gravity southeast -geometry '23x24>' -background transparent \
    $(find "./Units" -type f | sort --ignore-case -V)  "$DIR/units.png"

# Optimize PNG files if optipng is available
if command -v optipng &> /dev/null; then
    echo "Optimizing PNG files with optipng..."
    optipng "$DIR/tiles.png"
    optipng "$DIR/units.png"
    echo "Optimization complete!"
else
    echo "optipng not found. Skipping optimization step."
fi


