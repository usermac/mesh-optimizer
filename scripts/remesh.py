import argparse
import os
import sys
import traceback

import bpy
import numpy as np


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

    # Clean up mesh - critical for AI-generated meshes and many exports
    # Without this, QuadriFlow fails and baking produces artifacts
    verts_before = len(high_poly.data.vertices)
    print(f"[INFO] Cleaning mesh ({verts_before} vertices)...")

    # Use bmesh for reliable vertex merging (works in all Blender contexts)
    import bmesh
    bm = bmesh.new()
    bm.from_mesh(high_poly.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0001)  # 0.1mm threshold
    # Remove loose verts/edges
    loose_verts = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose_verts, context='VERTS')
    loose_edges = [e for e in bm.edges if not e.link_faces]
    bmesh.ops.delete(bm, geom=loose_edges, context='EDGES')
    bm.to_mesh(high_poly.data)
    bm.free()
    high_poly.data.update()

    verts_after = len(high_poly.data.vertices)
    verts_removed = verts_before - verts_after
    if verts_removed > 0:
        print(f"[INFO] Mesh cleanup removed {verts_removed} duplicate/loose vertices")

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

    # Set color management for accurate baking - Standard = raw sRGB, no cinematic grading
    bpy.context.scene.display_settings.display_device = 'sRGB'
    bpy.context.scene.view_settings.view_transform = 'Standard'
    print("[INFO] Color management set to Standard for accurate baking")

    # 1. Import
    high_poly = import_model(input_path)
    original_face_count = len(high_poly.data.polygons)

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

    quadriflow_success = False
    remesh_method = "decimate"  # Track which method was actually used
    pre_remesh_faces = len(low_poly.data.polygons)

    try:
        with bpy.context.temp_override(
            active_object=low_poly, selected_objects=[low_poly]
        ):
            bpy.ops.object.quadriflow_remesh(target_faces=target_faces)
        post_remesh_faces = len(low_poly.data.polygons)
        print(f"[INFO] Quadriflow completed. New face count: {post_remesh_faces}")
        # Check if Quadriflow actually changed the mesh meaningfully
        # If faces didn't change (or barely changed), consider it a failure
        if (
            post_remesh_faces < pre_remesh_faces * 0.95
            or post_remesh_faces != pre_remesh_faces
        ):
            # Quadriflow made a meaningful change OR hit the target
            if abs(post_remesh_faces - pre_remesh_faces) > 10:
                quadriflow_success = True
                remesh_method = "quadriflow"
            else:
                print(
                    f"[WARN] Quadriflow didn't change face count ({pre_remesh_faces} -> {post_remesh_faces})"
                )
        else:
            print(
                f"[WARN] Quadriflow didn't reduce faces as expected ({pre_remesh_faces} -> {post_remesh_faces})"
            )
    except Exception as e:
        print(f"[WARN] Quadriflow failed with exception: {e}")

    # Fallback to Decimate if Quadriflow didn't work
    if not quadriflow_success:
        print(f"[INFO] Falling back to Decimate modifier...")
        remesh_method = "decimate"
        # If Quadriflow ran but didn't help, we need to work with current state
        current_faces = len(low_poly.data.polygons)
        if current_faces > target_faces:
            mod = low_poly.modifiers.new(name="Decimate", type="DECIMATE")
            mod.ratio = max(0.01, target_faces / current_faces)
            print(
                f"[INFO] Applying Decimate with ratio {mod.ratio:.4f} ({current_faces} -> target {target_faces})"
            )
            with bpy.context.temp_override(object=low_poly):
                bpy.ops.object.modifier_apply(modifier="Decimate")
            print(
                f"[INFO] Decimate fallback completed. New face count: {len(low_poly.data.polygons)}"
            )
        else:
            print(
                f"[INFO] Current face count ({current_faces}) already at or below target ({target_faces})"
            )

    # 4. UV Unwrap
    print("[INFO] UV Unwrapping...")
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")

    # Clean up mesh before UV unwrap - merge loose geometry
    # This prevents fragmented UV islands from disconnected faces
    bpy.ops.mesh.remove_doubles(threshold=0.0001)

    # Diagnostic: count disconnected mesh islands
    bpy.ops.mesh.select_all(action='DESELECT')
    bpy.ops.mesh.select_all(action='SELECT')
    # Get island count by selecting linked and counting iterations
    import bmesh
    bm_check = bmesh.from_edit_mesh(low_poly.data)
    island_count = 0
    unvisited = set(range(len(bm_check.verts)))
    bm_check.verts.ensure_lookup_table()
    while unvisited:
        start_idx = next(iter(unvisited))
        stack = [start_idx]
        island = set()
        while stack:
            vi = stack.pop()
            if vi in island:
                continue
            island.add(vi)
            unvisited.discard(vi)
            v = bm_check.verts[vi]
            for e in v.link_edges:
                ov = e.other_vert(v)
                if ov.index in unvisited:
                    stack.append(ov.index)
        island_count += 1
    print(f"[INFO] Mesh has {island_count} disconnected islands - this affects UV quality")
    bpy.ops.mesh.select_all(action='SELECT')

    # Smart UV Project with tuned settings for production
    # 66° angle = fewer islands with acceptable distortion
    bpy.ops.uv.smart_project(
        angle_limit=1.1519,   # 66 degrees - fewer islands, acceptable distortion
        island_margin=0.01,   # Small gap to prevent texture bleed
        area_weight=0.0,
        correct_aspect=True,
        scale_to_bounds=True,
    )

    # CRITICAL: Use Blender 3.6+ packer for near-xatlas quality
    # This is much better than the old packer and stable on all platforms
    bpy.ops.uv.pack_islands(
        margin=0.005,           # Tight packing margin
        rotate=True,            # Allow rotation for better fit
        shape_method='AABB'     # Fast axis-aligned bounding box (CONCAVE/CONVEX/AABB)
    )
    bpy.ops.object.mode_set(mode="OBJECT")

    # Recalculate normals - QuadriFlow can produce inconsistent normals
    # that cause black spots during baking
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.normals_make_consistent(inside=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    print("[INFO] Recalculated normals for consistent baking")

    # Calculate dynamic bake settings based on model complexity
    dimensions = low_poly.dimensions
    max_dim = max(dimensions)
    high_poly_faces = len(high_poly.data.polygons)

    # High-poly models (>100k faces): use proven fixed values
    # Low-poly models (<100k faces): generous dynamic settings for QuadriFlow gaps
    if high_poly_faces > 100000:
        # Dense mesh - fixed values that worked well for textured models
        cage_ext = 0.1
        ray_dist = 0.15
        print(f"[INFO] Dense mesh ({high_poly_faces} faces): fixed bake settings")
    else:
        # Simple mesh - scale to model size for clean geometry
        cage_ext = max(0.001, min(0.5, max_dim * 0.005))  # 0.5% of model
        ray_dist = max(0.01, max_dim * 0.05)              # 5% of model
        print(f"[INFO] Simple mesh ({high_poly_faces} faces): dynamic bake settings")

    print(f"[INFO] Model size: {max_dim:.3f}, cage_extrusion: {cage_ext:.4f}, ray_dist: {ray_dist:.4f}")

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

    def setup_emission_for_bake(obj):
        """Convert materials to emission shaders for accurate color baking.

        This ensures we bake pure albedo (Color In = Color Out) without
        any lighting/shading contribution from Principled BSDF.
        """
        for mat in obj.data.materials:
            if mat is None or not mat.use_nodes:
                continue
            mat_nodes = mat.node_tree.nodes
            mat_links = mat.node_tree.links

            # Find the Principled BSDF
            principled = None
            for node in mat_nodes:
                if node.type == 'BSDF_PRINCIPLED':
                    principled = node
                    break

            if principled is None:
                continue

            # Get the Base Color input (could be texture or color value)
            base_color_input = principled.inputs.get('Base Color')
            if base_color_input is None:
                continue

            # Create Emission shader
            emission = mat_nodes.new('ShaderNodeEmission')
            emission.location = (principled.location.x, principled.location.y - 200)

            # Copy color or link texture to emission
            if base_color_input.is_linked:
                # Texture connected - relink it to emission
                from_socket = base_color_input.links[0].from_socket
                mat_links.new(from_socket, emission.inputs['Color'])
            else:
                # Just a color value
                emission.inputs['Color'].default_value = base_color_input.default_value

            emission.inputs['Strength'].default_value = 1.0

            # Find Material Output and connect emission
            for node in mat_nodes:
                if node.type == 'OUTPUT_MATERIAL':
                    mat_links.new(emission.outputs['Emission'], node.inputs['Surface'])
                    break

    def setup_metallic_emission_for_bake(obj):
        """Route Metallic value to Emission shader for baking.

        Blender has no direct METALLIC bake type, so we route the metallic
        value through an Emission shader and bake EMIT.
        """
        for mat in obj.data.materials:
            if mat is None or not mat.use_nodes:
                continue
            mat_nodes = mat.node_tree.nodes
            mat_links = mat.node_tree.links

            # Find the Principled BSDF
            principled = None
            for node in mat_nodes:
                if node.type == 'BSDF_PRINCIPLED':
                    principled = node
                    break

            if principled is None:
                continue

            # Get Metallic input (could be texture or value)
            metallic_input = principled.inputs.get('Metallic')
            if metallic_input is None:
                continue

            # Create Emission shader
            emission = mat_nodes.new('ShaderNodeEmission')
            emission.location = (principled.location.x, principled.location.y - 400)

            # Copy metallic value or link texture to emission
            if metallic_input.is_linked:
                # Texture connected - relink it to emission
                from_socket = metallic_input.links[0].from_socket
                mat_links.new(from_socket, emission.inputs['Color'])
            else:
                # Just a value - convert to grayscale color
                metallic_value = metallic_input.default_value
                emission.inputs['Color'].default_value = (metallic_value, metallic_value, metallic_value, 1.0)

            emission.inputs['Strength'].default_value = 1.0

            # Find Material Output and connect emission
            for node in mat_nodes:
                if node.type == 'OUTPUT_MATERIAL':
                    mat_links.new(emission.outputs['Emission'], node.inputs['Surface'])
                    break

    def pack_orm_texture(ao_img, rough_img, metal_img, tex_size):
        """Pack AO, Roughness, Metallic into single ORM texture.

        Channel layout (glTF 2.0 standard):
        - Red: Ambient Occlusion
        - Green: Roughness
        - Blue: Metallic
        """
        size = tex_size * tex_size

        # Extract first channel (grayscale) from each image
        ao_pixels = np.array(ao_img.pixels[:]).reshape(-1, 4)[:, 0]
        rough_pixels = np.array(rough_img.pixels[:]).reshape(-1, 4)[:, 0]
        metal_pixels = np.array(metal_img.pixels[:]).reshape(-1, 4)[:, 0]

        # Create ORM image
        orm_img = bpy.data.images.new("BakedORM", tex_size, tex_size)
        orm_img.colorspace_settings.name = 'Non-Color'

        # Pack channels
        orm_pixels = np.zeros((size, 4), dtype=np.float32)
        orm_pixels[:, 0] = ao_pixels       # R = AO
        orm_pixels[:, 1] = rough_pixels    # G = Roughness
        orm_pixels[:, 2] = metal_pixels    # B = Metallic
        orm_pixels[:, 3] = 1.0             # A = opaque

        orm_img.pixels[:] = orm_pixels.flatten()
        return orm_img

    # 6. Bake Color (EMIT) - pure albedo without lighting
    print("[INFO] Baking Color (EMIT)...")
    diffuse_node = create_bake_image("BakedDiffuse", is_color=True)
    nodes.active = diffuse_node

    # Calculate dynamic margin based on texture size
    # Rule: ~4 pixels per 1024px of resolution (scales with texture)
    bake_margin = max(4, texture_size // 256)  # 512→2, 1024→4, 2048→8, 4096→16
    print(f"[INFO] Using bake margin: {bake_margin}px for {texture_size}px texture")

    # Convert high-poly materials to emission for accurate color baking
    print("[INFO] Setting up emission shaders for color bake...")
    setup_emission_for_bake(high_poly)

    # Selection: Select High, then Low (Active)
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    # Bake EMIT for pure albedo (no lighting/shading contribution)
    try:
        bpy.ops.object.bake(
            type="EMIT",  # EMIT = raw color, no lighting
            use_selected_to_active=True,
            cage_extrusion=cage_ext,
            max_ray_distance=ray_dist,
            margin=bake_margin,
            margin_type='EXTEND',
        )
        print("[INFO] Color bake completed")
    except Exception as e:
        print(f"[WARN] Color bake failed: {e}")

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
            cage_extrusion=cage_ext,   # Dynamic: matches diffuse
            max_ray_distance=ray_dist, # Dynamic: matches diffuse
            margin=bake_margin,
            margin_type='EXTEND',      # Prevents black seam bleeding
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

    # 8. Bake AO (Ambient Occlusion)
    print("[INFO] Baking Ambient Occlusion...")
    ao_node = create_bake_image("BakeAO", is_color=False)
    nodes.active = ao_node

    # AO bake only needs low-poly (bakes self-occlusion)
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    try:
        bpy.ops.object.bake(
            type='AO',
            margin=bake_margin,
            margin_type='EXTEND',
        )
        print("[INFO] AO bake completed")
    except Exception as e:
        print(f"[WARN] AO bake failed: {e}")

    # 9. Bake Roughness
    print("[INFO] Baking Roughness...")
    rough_node = create_bake_image("BakeRoughness", is_color=False)
    nodes.active = rough_node

    # Re-select for roughness bake (high to low)
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    try:
        bpy.ops.object.bake(
            type='ROUGHNESS',
            use_selected_to_active=True,
            cage_extrusion=cage_ext,
            max_ray_distance=ray_dist,
            margin=bake_margin,
            margin_type='EXTEND',
        )
        print("[INFO] Roughness bake completed")
    except Exception as e:
        print(f"[WARN] Roughness bake failed: {e}")

    # 10. Bake Metallic (via EMIT workaround - Blender has no METALLIC bake type)
    print("[INFO] Baking Metallic...")
    metal_node = create_bake_image("BakeMetal", is_color=False)
    nodes.active = metal_node

    # Setup emission routing for metallic on high-poly
    setup_metallic_emission_for_bake(high_poly)

    # Re-select for metallic bake
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    try:
        bpy.ops.object.bake(
            type='EMIT',
            use_selected_to_active=True,
            cage_extrusion=cage_ext,
            max_ray_distance=ray_dist,
            margin=bake_margin,
            margin_type='EXTEND',
        )
        print("[INFO] Metallic bake completed")
    except Exception as e:
        print(f"[WARN] Metallic bake failed: {e}")

    # 11. Pack ORM texture
    print("[INFO] Packing ORM texture...")
    orm_img = pack_orm_texture(
        ao_node.image,
        rough_node.image,
        metal_node.image,
        texture_size
    )
    print("[INFO] ORM texture packed (R=AO, G=Roughness, B=Metallic)")

    # Create ORM texture node with Separate Color for glTF export
    orm_tex_node = nodes.new("ShaderNodeTexImage")
    orm_tex_node.image = orm_img
    orm_tex_node.location = (-500, -400)

    separate_node = nodes.new("ShaderNodeSeparateColor")
    separate_node.location = (-200, -400)
    mat.node_tree.links.new(orm_tex_node.outputs["Color"], separate_node.inputs["Color"])

    # Connect to Principled BSDF (glTF exporter recognizes this pattern)
    mat.node_tree.links.new(separate_node.outputs["Green"], bsdf.inputs["Roughness"])
    mat.node_tree.links.new(separate_node.outputs["Blue"], bsdf.inputs["Metallic"])

    # Cleanup temp bake images (keep only packed ORM)
    for img_name in ['BakeAO', 'BakeRoughness', 'BakeMetal']:
        if img_name in bpy.data.images:
            bpy.data.images.remove(bpy.data.images[img_name])

    # 12. Export
    # Delete High Poly so it doesn't get exported
    bpy.ops.object.select_all(action="DESELECT")
    high_poly.select_set(True)
    bpy.ops.object.delete()

    print(f"[INFO] Exporting to {output_path}...")
    bpy.ops.object.select_all(action="DESELECT")
    low_poly.select_set(True)
    bpy.context.view_layer.objects.active = low_poly

    # Export GLB
    # Note: AUTO uses PNG for non-color data (ORM, Normal), JPEG for color (Diffuse)
    bpy.ops.export_scene.gltf(
        filepath=output_path,
        use_selection=True,
        export_format="GLB",
        export_image_format="AUTO",
    )

    # Verify GLB output was created
    if not os.path.exists(output_path):
        raise RuntimeError(
            f"GLB export failed - output file not created: {output_path}"
        )

    output_size = os.path.getsize(output_path)
    if output_size == 0:
        raise RuntimeError(f"GLB export failed - output file is empty: {output_path}")

    print(f"[INFO] GLB export complete: {output_path} ({output_size} bytes)")

    # Export USDZ (same base name, different extension)
    # Blender automatically creates USDZ archive when filepath ends in .usdz
    usdz_path = os.path.splitext(output_path)[0] + ".usdz"
    print(f"[INFO] Exporting USDZ to {usdz_path}...")

    try:
        # Ensure low_poly is still selected
        bpy.ops.object.select_all(action="DESELECT")
        low_poly.select_set(True)
        bpy.context.view_layer.objects.active = low_poly

        # Use absolute minimal parameters - just filepath and selection
        # The .usdz extension triggers USDZ archive format automatically
        bpy.ops.wm.usd_export(filepath=usdz_path, selected_objects_only=True)

        if os.path.exists(usdz_path) and os.path.getsize(usdz_path) > 0:
            usdz_size = os.path.getsize(usdz_path)
            print(f"[INFO] USDZ export complete: {usdz_path} ({usdz_size} bytes)")
        else:
            print(f"[WARN] USDZ export may have failed - file missing or empty")
    except Exception as e:
        print(f"[WARN] USDZ export failed: {e}")
        traceback.print_exc()
        print("[WARN] Continuing without USDZ - GLB is available")

    # Output face counts and method for API to parse
    final_face_count = len(low_poly.data.polygons)
    print(f"FACE_COUNTS: {original_face_count} {final_face_count} {remesh_method}")

    print(f"[SUCCESS] Remesh complete! GLB: {output_path} ({output_size} bytes)")


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
