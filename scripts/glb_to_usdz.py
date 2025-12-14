import argparse
import os
import sys

import bpy


def run():
    # Get arguments after "--"
    argv = sys.argv
    if "--" in argv:
        argv = argv[argv.index("--") + 1 :]
    else:
        argv = []

    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="Input GLB file")
    parser.add_argument("--output", required=True, help="Output USDZ file")
    args = parser.parse_args(argv)

    input_path = os.path.abspath(args.input)
    output_path = os.path.abspath(args.output)

    print(f"Converting '{input_path}' to '{output_path}'...")

    # Clear scene
    bpy.ops.wm.read_factory_settings(use_empty=True)

    # Import GLB
    if not os.path.exists(input_path):
        print("Error: Input file does not exist.")
        sys.exit(1)

    try:
        bpy.ops.import_scene.gltf(filepath=input_path)
    except Exception as e:
        print(f"Error importing GLB: {e}")
        sys.exit(1)

    # Export USDZ
    # Blender's USD exporter handles .usdz if the extension is provided.
    try:
        bpy.ops.wm.usd_export(
            filepath=output_path,
            check_existing=False,
            export_materials=True,
            export_textures=True,  # Ensure textures are written
            # relative_paths=True, # Often helps with packing
            selected_objects_only=False,
        )
    except Exception as e:
        print(f"Error exporting USDZ: {e}")
        sys.exit(1)

    print("Conversion Complete.")


if __name__ == "__main__":
    run()
