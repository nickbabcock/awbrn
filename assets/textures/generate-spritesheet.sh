#!/bin/bash

set -euo pipefail

DIR="$( dirname -- "${BASH_SOURCE[0]}"; )";
cd "$DIR"

# Fill in with the path to AWBW-Replay-Player/AWBWApp.Resources/Textures
TEXTURES_DIR="${TEXTURES_DIR:-.}"

montage -tile 64x -mode concatenate -gravity southeast -geometry '16x32>' -background transparent $(cat \
   <(find "$TEXTURES_DIR/Map/AW2" -maxdepth 1 -type f -not -name "*_Snow*" -not -name "*_Rain*" | sort) \
   <(echo "stubby.png") \
   <(find "$TEXTURES_DIR/Map/AW2" -mindepth 2 -type f -not -name "*_Snow*" -not -name "*_Rain*" | sort) \
   <(find "$TEXTURES_DIR/Map/AW2" -maxdepth 1 -type f -name "*_Snow*" | sort) \
   <(echo "stubby-snow.png") \
   <(find "$TEXTURES_DIR/Map/AW2" -mindepth 2 -type f -name "*_Snow*" | sort) \
   <(find "$TEXTURES_DIR/Map/AW2" -maxdepth 1 -type f -name "*_Rain*" | sort) \
   <(find "$TEXTURES_DIR/Map/AW2" -mindepth 2 -type f -name "*_Rain*" | sort)
) tiles.png

montage -tile 64x -mode concatenate -gravity southeast -geometry '23x24>' -background transparent \
    $(find "$TEXTURES_DIR/Units" -type f | sort) units.png

# Optimize PNG files if optipng is available
if command -v optipng &> /dev/null; then
    echo "Optimizing PNG files with optipng..."
    optipng tiles.png
    optipng units.png
    echo "Optimization complete!"
else
    echo "optipng not found. Skipping optimization step."
fi


