import os
import json
import re
import argparse

parser = argparse.ArgumentParser(description='Generate unit animation data')
parser.add_argument('--rust', action='store_true', help='Generate Rust code instead of file paths')
args = parser.parse_args()

# Read the Units.json file and remove comments
script_dir = os.path.dirname(__file__)
units_json_path = os.path.join(script_dir, 'data', 'Units.json')
with open(units_json_path, 'r') as f:
    content = f.read()
    # Remove comments (// to end of line)
    content = re.sub(r'//.*', '', content)
    units_data = json.loads(content)

if args.rust:
    print("// This file is generated by unit_animation.py --rust")
    print("fn get_animation_data(unit: Unit) -> (u16, UnitAnimationData) {")
    print("match unit {")
    frames = 0
    
    # Iterate through each unit in Units.json
    for unit_name, unit_data in units_data.items():
        # Get the texture name from IdleAnimation and remove dashes for Rust enum
        idle_texture = unit_data['IdleAnimation']['Texture'].replace('-', '')
        
        # Get frame arrays for each animation type
        idle_frames = unit_data['IdleAnimation']['Frames']
        move_up_frames = unit_data['MoveUpAnimation']['Frames']
        move_down_frames = unit_data['MoveDownAnimation']['Frames']
        move_side_frames = unit_data['MoveSideAnimation']['Frames']
        
        # Format frames as Rust arrays
        idle_frames_str = ', '.join(map(str, idle_frames))
        move_up_frames_str = ', '.join(map(str, move_up_frames))
        move_down_frames_str = ', '.join(map(str, move_down_frames))
        move_side_frames_str = ', '.join(map(str, move_side_frames))
        
        print(f"    Unit::{idle_texture} => ({frames}, UnitAnimationData::new(")
        print(f"        &[{idle_frames_str}],")
        print(f"        &[{move_up_frames_str}],")
        print(f"        &[{move_down_frames_str}],")
        print(f"        &[{move_side_frames_str}],")
        print(f"    )),")
        frames += len(idle_frames) + len(move_up_frames) + len(move_down_frames) + len(move_side_frames)
    
    print("}")
    print("}")

    print("impl UnitAnimationData {")
    print(f"pub const TOTAL_FRAMES: usize = {frames};")
    print("}")
else:
    # Define the base path for textures (only needed for file paths)
    base_path = os.environ.get('TEXTURES_DIR', "../AWBW-Replay-Player/AWBWApp.Resources/Textures")
    base_path = os.path.join(base_path, 'Units')

    # Get all faction directories in alphabetical order
    factions = sorted([d for d in os.listdir(base_path) if os.path.isdir(os.path.join(base_path, d))])
    
    for faction in factions:
        for unit_name, unit_data in units_data.items():
            animations = ['IdleAnimation',  'MoveUpAnimation', 'MoveDownAnimation', 'MoveSideAnimation']
            
            for anim_type in animations:
                if anim_type in unit_data:
                    anim_data = unit_data[anim_type]
                    texture_name = anim_data['Texture']
                    frame_count = len(anim_data['Frames'])
                    
                    # Generate file paths for each frame
                    for i in range(frame_count):
                        file_path = f"{base_path}/{faction}/{texture_name}-{i}.png"
                        print(file_path)