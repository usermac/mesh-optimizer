import argparse
import os
import sys
import traceback

import bpy


def reset_scene():
    """Clears the scene effectively."""
    bpy.ops.wm.read_factory_settings(use_empty=True)


def enable_gpu():
    """Attempts to enable GPU for Cycles."""
    try:
        preferences = bpy.context.preferences
        cycles_preferences = preferences.addons["cycles"].preferences
        cycles_preferences.refresh_devices()

        # Try to find a GPU device
        devices = cycles_preferences.devices
        device_types = ["OPTIX", "CUDA", "METAL", "HIP"]

        found = False
        for device_type in device_types:
            for device in devices:
                if device.type == device_type:
                    device.use = True
                    cycles_preferences.compute_device_type = device_type
                    found = True
                    print(f"[INFO] Enabled GPU: {device.name} ({device_type})")
                    break
            if found:
                break

        if not found:
            print("[INFO] No GPU found, using CPU.")
    except Exception as e:
        print(f"[WARN] Failed to configure GPU: {e}")


def import_model(filepath):
    """Imports the model based on extension."""
    # Convert to absolute path to avoid working directory issues
    filepath = os.path.abspath(filepath)

    if not os.path.exists(filepath):
        raise FileNotFoundError(f"Input file not found: {filepath}")

    ext = os.path.splitext(filepath)[1].lower()
    print(f"[INFO] Importing {filepath}...")

    try:
        if ext == ".obj":
            bpy.ops.wm.obj_import(filepath=filepath)
        elif ext == ".glb" or ext == ".gltf":
            bpy.ops.import_scene.gltf(filepath=filepath)
        elif ext == ".fbx":
            bpy.ops.import_scene.fbx(filepath=filepath)
        else:
            raise ValueError(f"Unsupported extension: {ext}")
    except Exception as e:
        print(f"[ERROR] Import failed: {e}")
        raise

    # Join all imported meshes into one 'HighPoly' object
    bpy.ops.object.select_all(action="DESELECT")
    mesh_objs = [o for o in bpy.context.scene.objects if o.type == "MESH"]

    if not mesh_objs:
        raise ValueError("No mesh objects imported.")

    print(f"[INFO] Found {len(mesh_objs)} mesh object(s), joining...")

    # Select all mesh objects and set first as active
    for obj in mesh_objs:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = mesh_objs[0]

    # Join using Blender 4.x temp_override API
    if len(mesh_objs) > 1:
        with bpy.context.temp_override(
            active_object=mesh_objs[0], selected_editable_objects=mesh_objs
        ):
            bpy.ops.object.join()

    high_poly = bpy.context.view_layer.objects.active
    high_poly.name = "HighPoly"

    # Ensure it's active and selected
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.context.view_layer.objects.active = high_poly

    print(f"[INFO] HighPoly mesh created with {len(high_poly.data.polygons)} faces")
    return high_poly


def process(input_path, output_path, target_faces, texture_size):
    """Main processing function."""
    print("[INFO] Starting remesh process")
    print(f"[INFO] Input: {input_path}")
    print(f"[INFO] Output: {output_path}")
    print(f"[INFO] Target faces: {target_faces}")
    print(f"[INFO] Texture size: {texture_size}")

    # Convert paths to absolute
    input_path = os.path.abspath(input_path)
    output_path = os.path.abspath(output_path)

    reset_scene()

    # Setup Rendering
    bpy.context.scene.render.engine = "CYCLES"
    enable_gpu()
    bpy.context.scene.cycles.samples = (
        16  # Low samples for baking is usually sufficient
    )
    bpy.context.scene.cycles.use_adaptive_sampling = False

    # 1. Import
    high_poly = import_model(input_path)

    # 2. Duplicate for Remeshing
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.context.view_layer.objects.active = high_poly
    bpy.ops.object.duplicate()
    low_poly = bpy.context.active_object
    low_poly.name = "LowPoly"

    # 3. Remesh
    print(f"[INFO] Remeshing to approx {target_faces} faces using Quadriflow...")
    bpy.ops.object.mode_set(mode="OBJECT")

    try:
        with bpy.context.temp_override(
            active_object=low_poly, selected_objects=[low_poly]
        ):
            bpy.ops.object.quadriflow_remesh(target_faces=target_faces)
        print(
            f"[INFO] Quadriflow completed. New face count: {len(low_poly.data.polygons)}"
        )
    except Exception as e:
        print(f"[WARN] Quadriflow failed ({e}), falling back to Decimate modifier")
        # Fallback: Use Decimate modifier
        mod = low_poly.modifiers.new(name="Decimate", type="DECIMATE")
        # Calculate ratio based on target faces vs current faces
        current_faces = len(low_poly.data.polygons)
        if current_faces > 0:
            mod.ratio = min(1.0, target_faces / current_faces)
        else:
            mod.ratio = 0.1
        with bpy.context.temp_override(object=low_poly):
            bpy.ops.object.modifier_apply(modifier="Decimate")
        print(
            f"[INFO] Decimate fallback completed. New face count: {len(low_poly.data.polygons)}"
        )

    # 4. UV Unwrap
    print("[INFO] UV Unwrapping...")
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(island_margin=0.01)
    bpy.ops.object.mode_set(mode="OBJECT")

    # 5. Prepare Materials for Baking
    print("[INFO] Preparing materials for baking...")
    mat = bpy.data.materials.new(name="BakedMaterial")
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    nodes.clear()

    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (0, 0)

    # Add material output node
    output_node = nodes.new("ShaderNodeOutputMaterial")
    output_node.location = (300, 0)
    mat.node_tree.links.new(bsdf.outputs["BSDF"], output_node.inputs["Surface"])

    low_poly.data.materials.clear()
    low_poly.data.materials.append(mat)

    def create_bake_image(name, is_color=True):
        img = bpy.data.images.new(name, width=texture_size, height=texture_size)
        img.colorspace_settings.name = "sRGB" if is_color else "Non-Color"
        node = nodes.new("ShaderNodeTexImage")
        node.image = img
        node.location = (-300, 200 if is_color else -200)
        return node

    # 6. Bake Diffuse
    print("[INFO] Baking Diffuse...")
    diffuse_node = create_bake_image("BakedDiffuse", is_color=True)
    nodes.active = diffuse_node

    # Selection: Select High, then Low (Active)
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    # Bake call
    try:
        bpy.ops.object.bake(
            type="DIFFUSE",
            pass_filter={"COLOR"},
            use_selected_to_active=True,
            max_ray_distance=0.1,
            margin=16,
        )
        print("[INFO] Diffuse bake completed")
    except Exception as e:
        print(f"[WARN] Diffuse bake failed: {e}")

    # Connect Diffuse
    mat.node_tree.links.new(diffuse_node.outputs["Color"], bsdf.inputs["Base Color"])

    # 7. Bake Normal
    print("[INFO] Baking Normal...")
    normal_node = create_bake_image("BakedNormal", is_color=False)
    nodes.active = normal_node

    # Re-select for normal bake
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    try:
        bpy.ops.object.bake(
            type="NORMAL",
            use_selected_to_active=True,
            max_ray_distance=0.1,
            margin=16,
        )
        print("[INFO] Normal bake completed")
    except Exception as e:
        print(f"[WARN] Normal bake failed: {e}")

    # Connect Normal
    normal_map_node = nodes.new("ShaderNodeNormalMap")
    normal_map_node.location = (-150, -100)
    mat.node_tree.links.new(
        normal_node.outputs["Color"], normal_map_node.inputs["Color"]
    )
    mat.node_tree.links.new(normal_map_node.outputs["Normal"], bsdf.inputs["Normal"])

    # 8. Export
    # Delete High Poly so it doesn't get exported
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.ops.object.delete()

    print(f"[INFO] Exporting to {output_path}...")
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    bpy.ops.export_scene.gltf(
        filepath=output_path,
        use_selection=True,
        export_format="GLB",
        export_image_format="JPEG",
    )

    # Verify output was created
    if not os.path.exists(output_path):
        raise RuntimeError(f"Export failed - output file not created: {output_path}")

    output_size = os.path.getsize(output_path)
    if output_size == 0:
        raise RuntimeError(f"Export failed - output file is empty: {output_path}")

    print(f"[SUCCESS] Remesh complete! Output: {output_path} ({output_size} bytes)")


if __name__ == "__main__":
    # Filter args for this script (everything after --)
    if "--" in sys.argv:
        argv = sys.argv[sys.argv.index("--") + 1 :]
    else:
        argv = []

    parser = argparse.ArgumentParser(description="Blender 4.x Remeshing Script")
    parser.add_argument("--input", required=True, help="Input file path")
    parser.add_argument("--output", required=True, help="Output file path (GLB)")
    parser.add_argument("--faces", type=int, default=5000, help="Target face count")
    parser.add_argument(
        "--texture_size", type=int, default=2048, help="Texture resolution"
    )

    args = parser.parse_args(argv)

    try:
        process(args.input, args.output, args.faces, args.texture_size)
        sys.exit(0)
    except FileNotFoundError as e:
        print(f"[ERROR] File not found: {e}")
        traceback.print_exc()
        sys.exit(1)
    except ValueError as e:
        print(f"[ERROR] Invalid input: {e}")
        traceback.print_exc()
        sys.exit(1)
    except RuntimeError as e:
        print(f"[ERROR] Processing failed: {e}")
        traceback.print_exc()
        sys.exit(1)
    except Exception as e:
        print(f"[ERROR] Unexpected error: {e}")
        traceback.print_exc()
        sys.exit(1)
