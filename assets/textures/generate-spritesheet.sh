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

# Generate a list of valid unit files
# Format: [unit_name frames_idle frames_mdown frames_mside frames_mup]
# This necessary due to BlackNoir having blank sprites for naval units.
UNITS=(
    "Anti-Air   4 3 3 3"
    "APC        4 3 3 3"
    "Artillery  4 3 3 3"
    "Battleship 2 2 2 2"
    "BlackBoat  4 2 2 2"
    "BlackBomb  4 3 3 3"
    "Bomber     2 3 3 3"
    "B-Copter   4 2 2 2"
    "Carrier    2 2 2 2"
    "Cruiser    2 2 2 2"
    "Fighter    2 3 3 3"
    "Infantry   4 4 4 4"
    "Lander     2 2 2 2"
    "MdTank     4 3 3 3"
    "Mech       2 4 4 4"
    "MegaTank   4 3 3 3"
    "Missile    2 3 3 3"
    "NeoTank    4 3 3 3"
    "PipeRunner 2 3 3 3"
    "Recon      4 3 3 3"
    "Rocket     2 3 3 3"
    "Stealth    2 3 3 3"
    "Sub        4 2 2 2"
    "Tank       4 3 3 3"
    "T-Copter   4 2 2 2"
)

# Create a temporary file for the filtered list of unit files
UNIT_FILES_LIST=$(mktemp)
trap 'rm -rf "$UNIT_FILES_LIST"' EXIT

# Find all faction directories under Units
FACTION_DIRS=$(find "./Units" -mindepth 1 -maxdepth 1 -type d)

# Generate the filtered list of unit files
for UNIT_DATA in "${UNITS[@]}"; do
    # Extract unit name and frame counts
    read -r UNIT COUNT_DEFAULT COUNT_MDOWN COUNT_MSIDE COUNT_MUP <<< "$UNIT_DATA"
    
    # Process each faction directory
    for FACTION_DIR in $FACTION_DIRS; do
        # Idle
        for ((j=0; j<$COUNT_DEFAULT; j++)); do
            echo "${FACTION_DIR}/${UNIT}-${j}.png" >> "$UNIT_FILES_LIST"
        done

        # Down
        for ((j=0; j<$COUNT_MDOWN; j++)); do
            echo "${FACTION_DIR}/${UNIT}_MDown-${j}.png" >> "$UNIT_FILES_LIST"
        done

        # Side
        for ((j=0; j<$COUNT_MSIDE; j++)); do
            echo "${FACTION_DIR}/${UNIT}_MSide-${j}.png" >> "$UNIT_FILES_LIST"
        done

        # Up
        for ((j=0; j<$COUNT_MUP; j++)); do
            echo "${FACTION_DIR}/${UNIT}_MUp-${j}.png" >> "$UNIT_FILES_LIST"
        done
    done
done

# Use the filtered list for montage
montage -tile 64x -mode concatenate -gravity southeast -geometry '23x24>' -background transparent \
    $(cat "$UNIT_FILES_LIST" | sort --ignore-case -V) "$DIR/units.png"

# Optimize PNG files if optipng is available
if command -v optipng &> /dev/null; then
    echo "Optimizing PNG files with optipng..."
    optipng "$DIR/tiles.png"
    optipng "$DIR/units.png"
    echo "Optimization complete!"
else
    echo "optipng not found. Skipping optimization step."
fi
