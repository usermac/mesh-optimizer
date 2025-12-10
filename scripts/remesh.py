import argparse
import os
import sys

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
                    print(f"Enabled GPU: {device.name} ({device_type})")
                    break
            if found:
                break

        if not found:
            print("No GPU found, using CPU.")
    except Exception as e:
        print(f"Failed to configure GPU: {e}")


def import_model(filepath):
    """Imports the model based on extension."""
    if not os.path.exists(filepath):
        raise FileNotFoundError(f"Input file not found: {filepath}")

    ext = os.path.splitext(filepath)[1].lower()
    print(f"Importing {filepath}...")

    try:
        if ext == ".obj":
            bpy.ops.import_scene.obj(filepath=filepath)
        elif ext == ".glb" or ext == ".gltf":
            bpy.ops.import_scene.gltf(filepath=filepath)
        elif ext == ".fbx":
            bpy.ops.import_scene.fbx(filepath=filepath)
        else:
            raise ValueError(f"Unsupported extension: {ext}")
    except Exception as e:
        print(f"Import failed: {e}")
        # Try generic import or fail
        raise

    # Join all imported meshes into one 'HighPoly' object
    bpy.ops.object.select_all(action="DESELECT")
    mesh_objs = [o for o in bpy.context.scene.objects if o.type == "MESH"]

    if not mesh_objs:
        raise ValueError("No mesh objects imported.")

    ctx = bpy.context.copy()
    ctx["active_object"] = mesh_objs[0]
    ctx["selected_editable_objects"] = mesh_objs
    bpy.ops.object.join(ctx)

    high_poly = mesh_objs[0]
    high_poly.name = "HighPoly"

    # Ensure it's active and selected
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.context.view_layer.objects.active = high_poly

    return high_poly


def process(input_path, output_path, target_faces, texture_size):
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
    bpy.ops.object.duplicate()
    low_poly = bpy.context.active_object
    low_poly.name = "LowPoly"

    # 3. Remesh
    print(f"Remeshing to approx {target_faces} faces using Quadriflow...")
    # Ensure mode is OBJECT
    bpy.ops.object.mode_set(mode="OBJECT")

    # Quadriflow Remesh
    # Note: If this fails or hangs, Voxel remesh + Decimate is a fallback strategy,
    # but Quadriflow gives better topology for organic/scanned meshes.
    try:
        bpy.ops.object.quadriflow_remesh(target_faces=target_faces)
    except Exception as e:
        print(f"Quadriflow failed ({e}), falling back to Voxel Remesh + Decimate")
        # Fallback: Voxel remesh usually requires a size, not face count.
        # Let's use Decimate modifier instead if Quadriflow fails,
        # or maybe just Decimate directly if the topology isn't critical?
        # Assuming we want clean geo, let's try a simple decimate as fallback
        mod = low_poly.modifiers.new(name="Decimate", type="DECIMATE")
        mod.ratio = 0.1  # Arbitrary fallback
        bpy.ops.object.modifier_apply(modifier="Decimate")

    # 4. UV Unwrap
    print("UV Unwrapping...")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(island_margin=0.01)
    bpy.ops.object.mode_set(mode="OBJECT")

    # 5. Prepare Materials for Baking
    # Create target material
    mat = bpy.data.materials.new(name="BakedMaterial")
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    nodes.clear()

    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    bsdf.location = (0, 0)

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
    print("Baking Diffuse...")
    diffuse_node = create_bake_image("BakedDiffuse", is_color=True)
    nodes.active = diffuse_node  # Active node receives the bake

    # Selection: Select High, then Low (Active)
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    # Bake call
    # Ray distance might need tuning based on scale.
    # 0.0 means automatic if cage is not used, but using a small max_ray_distance helps.
    bpy.ops.object.bake(
        type="DIFFUSE",
        pass_filter={"COLOR"},
        use_selected_to_active=True,
        max_ray_distance=0.1,  # 10cm safety margin? Adjust if scale is huge/tiny.
        margin=16,
    )

    # Connect Diffuse
    mat.node_tree.links.new(diffuse_node.outputs["Color"], bsdf.inputs["Base Color"])

    # 7. Bake Normal
    print("Baking Normal...")
    normal_node = create_bake_image("BakedNormal", is_color=False)
    nodes.active = normal_node

    bpy.ops.object.bake(
        type="NORMAL", use_selected_to_active=True, max_ray_distance=0.1, margin=16
    )

    # Connect Normal
    normal_map_node = nodes.new("ShaderNodeNormalMap")
    normal_map_node.location = (-150, -100)
    mat.node_tree.links.new(
        normal_node.outputs["Color"], normal_map_node.inputs["Color"]
    )
    mat.node_tree.links.new(normal_map_node.outputs["Normal"], bsdf.inputs["Normal"])

    # 8. Export
    # Delete High Poly so it doesn't get exported if we used 'export all' (though we use use_selection)
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.ops.object.delete()

    print(f"Exporting to {output_path}...")
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    bpy.ops.export_scene.gltf(
        filepath=output_path,
        use_selection=True,
        export_format="GLB",
        export_image_format="JPEG",  # Compresses textures
    )
    print("Done.")


if __name__ == "__main__":
    # Filter args for this script
    if "--" in sys.argv:
        argv = sys.argv[sys.argv.index("--") + 1 :]
    else:
        argv = []

    parser = argparse.ArgumentParser(description="Blender Remeshing Script")
    parser.add_argument("--input", required=True, help="Input file path")
    parser.add_argument("--output", required=True, help="Output file path (GLB)")
    parser.add_argument("--faces", type=int, default=5000, help="Target face count")
    parser.add_argument(
        "--texture_size", type=int, default=2048, help="Texture resolution"
    )

    args = parser.parse_args(argv)

    process(args.input, args.output, args.faces, args.texture_size)
