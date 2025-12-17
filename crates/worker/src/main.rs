use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fbxcel_dom::any::AnyDocument;

use fbxcel_dom::v7400::object::model::TypedModelHandle;
use fbxcel_dom::v7400::object::TypedObjectHandle;
use image::{DynamicImage, GenericImageView, ImageBuffer};
use meshopt::VertexDataAdapter;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Cursor, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    input: PathBuf,

    #[arg(short, long)]
    output: PathBuf,

    /// Quality ratio (0.0-1.0). Lower values = more reduction.
    #[arg(long, default_value_t = 0.5)]
    ratio: f32,

    /// Target face count. If provided, ratio is calculated from original face count.
    #[arg(long)]
    target_faces: Option<i32>,

    /// Target percentage (1-100). If provided, ratio is calculated as percentage/100.
    #[arg(long)]
    target_percentage: Option<f32>,

    #[arg(long, default_value_t = false)]
    usdz: bool,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Serialize)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

fn compact_vertices(vertices: &[Vertex], indices: &[u32]) -> (Vec<Vertex>, Vec<u32>) {
    let mut used = vec![false; vertices.len()];
    for &i in indices {
        used[i as usize] = true;
    }

    let mut new_vertices = Vec::new();
    let mut old_to_new = vec![0; vertices.len()];

    for (i, &u) in used.iter().enumerate() {
        if u {
            old_to_new[i] = new_vertices.len() as u32;
            new_vertices.push(vertices[i]);
        }
    }

    let new_indices = indices.iter().map(|&i| old_to_new[i as usize]).collect();

    (new_vertices, new_indices)
}

fn process_image(img: DynamicImage) -> Option<Vec<u8>> {
    println!("TEXTURE_OPENED: {}x{}", img.width(), img.height());
    let (w, h) = img.dimensions();
    let img = if w > 2048 || h > 2048 {
        println!("RESIZING: {}x{} -> 2048x2048", w, h);
        let start = std::time::Instant::now();
        let res = img.resize(2048, 2048, image::imageops::FilterType::Triangle);
        println!("RESIZING_DONE: took {:?}", start.elapsed());
        res
    } else {
        img
    };
    let mut bytes: Vec<u8> = Vec::new();
    println!("ENCODING_TEXTURE_START");
    if let Err(e) = img.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png) {
        println!("Warning: Failed to encode texture: {:?}", e);
        None
    } else {
        println!("ENCODING_TEXTURE_DONE");
        Some(bytes)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("RUST_START: Processing {:?}", args.input);

    // 1. Detect Extension
    let extension = args
        .input
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or(anyhow!("Unknown file extension"))?;

    // 2. Load Geometry based on format
    println!("LOADING_GEOMETRY_START");
    let (vertices, indices, texture_data, base_color) = match extension.as_str() {
        "obj" => load_obj(&args.input)?,
        "glb" | "gltf" => load_gltf(&args.input)?,
        "fbx" => load_fbx(&args.input)?,
        _ => return Err(anyhow!("Unsupported format: .{}", extension)),
    };
    println!("LOADING_GEOMETRY_END");

    println!("STATS: {} verts, {} indices", vertices.len(), indices.len());
    if let Some(ref tex) = texture_data {
        println!("TEXTURE: Found ({} bytes)", tex.len());
    }
    if let Some(color) = base_color {
        println!("COLOR: Found {:?}", color);
    }
    if vertices.is_empty() {
        return Err(anyhow!("Model contains no vertices"));
    }

    // 3. Optimize (Decimate)
    println!("SIMPLIFICATION_START");

    // Calculate target_count based on priority: target_faces > target_percentage > ratio
    let original_face_count = indices.len() / 3; // Each face has 3 indices
    let target_count = if let Some(tf) = args.target_faces {
        // User specified exact face count - convert to index count
        let target_indices = (tf as usize) * 3;
        println!(
            "TARGET_MODE: face_count={} (original: {} faces)",
            tf, original_face_count
        );
        target_indices.min(indices.len()) // Don't exceed original
    } else if let Some(tp) = args.target_percentage {
        // User specified percentage (1-100)
        let ratio = tp / 100.0;
        println!("TARGET_MODE: percentage={}% (ratio: {:.3})", tp, ratio);
        (indices.len() as f32 * ratio) as usize
    } else {
        // Default: use ratio directly
        println!("TARGET_MODE: ratio={:.3}", args.ratio);
        (indices.len() as f32 * args.ratio) as usize
    };

    let vertex_data_u8 = bytemuck::cast_slice(&vertices);
    let stride = std::mem::size_of::<Vertex>();
    let adapter = VertexDataAdapter::new(vertex_data_u8, stride, 0)
        .map_err(|e| anyhow!("Adapter error: {:?}", e))?;

    let simplified_indices = meshopt::simplify(&indices, &adapter, target_count, 0.01);
    println!("SIMPLIFICATION_END");

    println!(
        "OPTIMIZED: {} -> {} indices",
        indices.len(),
        simplified_indices.len()
    );

    // Compact vertices (remove unused)
    println!("COMPACTING_START");
    let (vertices, simplified_indices) = compact_vertices(&vertices, &simplified_indices);
    println!("COMPACTING_END");
    println!("COMPACTED: {} verts", vertices.len());

    // 4. Export to GLB
    println!("EXPORT_GLB_START");
    save_glb(
        &args.output,
        &vertices,
        &simplified_indices,
        texture_data.as_deref(),
        base_color,
    )?;
    println!("EXPORT_GLB_END");

    // 5. Export to USDZ (Optional)
    if args.usdz {
        println!("EXPORT_USDZ_START");
        let usdz_path = args.output.with_extension("usdz");
        save_usdz(&usdz_path, &vertices, &simplified_indices)?;
        println!("EXPORTED: {:?}", usdz_path);
        println!("EXPORT_USDZ_END");
    }

    println!("RUST_DONE");
    Ok(())
}

fn load_obj(path: &PathBuf) -> Result<(Vec<Vertex>, Vec<u32>, Option<Vec<u8>>, Option<[f32; 3]>)> {
    let (models, materials) = tobj::load_obj(
        path,
        &tobj::LoadOptions {
            single_index: true,
            triangulate: true,
            ignore_lines: true,
            ignore_points: true,
        },
    )
    .context("Failed to load OBJ")?;

    // Load Texture (if any) or Color
    let mut texture_data = None;
    let mut base_color = None;

    if let Ok(mats) = materials {
        if !mats.is_empty() {
            // 1. Try Texture
            if let Some(diffuse) = &mats[0].diffuse_texture {
                if !diffuse.is_empty() {
                    let parent = path.parent().unwrap_or(std::path::Path::new("."));
                    let tex_path = parent.join(diffuse);
                    println!("LOADING TEXTURE: {:?}", tex_path);

                    match image::open(&tex_path) {
                        Ok(img) => {
                            texture_data = process_image(img);
                        }
                        Err(e) => {
                            println!("Warning: Could not load texture {:?}: {:?}", tex_path, e)
                        }
                    }
                }
            }

            // 2. If no texture, try Diffuse Color (Kd)
            if texture_data.is_none() {
                base_color = mats[0].diffuse;
            }
        }
    }

    if models.is_empty() {
        return Err(anyhow!("No models found in OBJ"));
    }

    // Merge all models into one for simplicity
    let mut all_vertices = Vec::new();
    let mut all_indices = Vec::new();
    let mut index_offset = 0;

    for model in models {
        let mesh = model.mesh;
        let n_vertices = mesh.positions.len() / 3;

        for i in 0..n_vertices {
            let pos = [
                mesh.positions[i * 3],
                mesh.positions[i * 3 + 1],
                mesh.positions[i * 3 + 2],
            ];
            let normal = if !mesh.normals.is_empty() {
                [
                    mesh.normals[i * 3],
                    mesh.normals[i * 3 + 1],
                    mesh.normals[i * 3 + 2],
                ]
            } else {
                [0.0, 1.0, 0.0]
            };
            let uv = if !mesh.texcoords.is_empty() {
                [mesh.texcoords[i * 2], mesh.texcoords[i * 2 + 1]]
            } else {
                [0.0, 0.0]
            };
            all_vertices.push(Vertex { pos, normal, uv });
        }

        for i in mesh.indices {
            all_indices.push(i + index_offset);
        }
        index_offset += n_vertices as u32;
    }

    Ok((all_vertices, all_indices, texture_data, base_color))
}

fn load_gltf(path: &PathBuf) -> Result<(Vec<Vertex>, Vec<u32>, Option<Vec<u8>>, Option<[f32; 3]>)> {
    let (document, buffers, images) = gltf::import(path).context("Failed to load GLTF/GLB")?;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut index_offset = 0;

    let mut texture_data = None;
    let mut base_color = None;

    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|iter| iter.collect())
                .unwrap_or_default();

            if positions.is_empty() {
                continue;
            }

            let normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(|iter| iter.collect())
                .unwrap_or_default();

            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|v| v.into_f32().collect())
                .unwrap_or_default();

            for i in 0..positions.len() {
                vertices.push(Vertex {
                    pos: positions[i],
                    normal: *normals.get(i).unwrap_or(&[0.0, 1.0, 0.0]),
                    uv: *uvs.get(i).unwrap_or(&[0.0, 0.0]),
                });
            }

            if let Some(iter) = reader.read_indices() {
                for index in iter.into_u32() {
                    indices.push(index + index_offset);
                }
            }

            index_offset += positions.len() as u32;

            // Extract Texture/Color (First one found wins)
            if texture_data.is_none() {
                let material = primitive.material();
                let pbr = material.pbr_metallic_roughness();

                // 1. Base Color Factor
                if base_color.is_none() {
                    let c = pbr.base_color_factor();
                    if c[0] < 0.99 || c[1] < 0.99 || c[2] < 0.99 {
                        base_color = Some([c[0], c[1], c[2]]);
                    }
                }

                // 2. Base Color Texture
                if let Some(tex_info) = pbr.base_color_texture() {
                    let texture = tex_info.texture();
                    let source_idx = texture.source().index();

                    if let Some(image_data) = images.get(source_idx) {
                        let dyn_img = match image_data.format {
                            gltf::image::Format::R8G8B8 => {
                                ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(
                                    image_data.width,
                                    image_data.height,
                                    image_data.pixels.clone(),
                                )
                                .map(DynamicImage::ImageRgb8)
                            }
                            gltf::image::Format::R8G8B8A8 => {
                                ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                                    image_data.width,
                                    image_data.height,
                                    image_data.pixels.clone(),
                                )
                                .map(DynamicImage::ImageRgba8)
                            }
                            _ => {
                                println!(
                                    "Warning: Unsupported GLTF image format: {:?}",
                                    image_data.format
                                );
                                None
                            }
                        };

                        if let Some(img) = dyn_img {
                            texture_data = process_image(img);
                        }
                    }
                }
            }
        }
    }

    Ok((vertices, indices, texture_data, base_color))
}

fn load_fbx(path: &PathBuf) -> Result<(Vec<Vertex>, Vec<u32>, Option<Vec<u8>>, Option<[f32; 3]>)> {
    let file = File::open(path).context("Failed to open FBX file")?;
    let reader = BufReader::new(file);

    // FBX loading is complex. We handle FBX 7.4+
    match AnyDocument::from_seekable_reader(reader)? {
        AnyDocument::V7400(_, doc) => {
            let mut vertices = Vec::new();
            let mut indices = Vec::new();
            let mut index_offset = 0;

            for object in doc.objects() {
                if let TypedObjectHandle::Model(TypedModelHandle::Mesh(mesh)) = object.get_typed() {
                    let geometry = mesh.geometry()?;
                    let polygon_vertices_obj = geometry.polygon_vertices()?;
                    let control_points: Vec<_> =
                        polygon_vertices_obj.raw_control_points()?.collect();
                    let raw_indices = polygon_vertices_obj.raw_polygon_vertices();

                    // Triangulate
                    // FBX polygons are lists of indices. Last index of a poly is negative (XOR -1).
                    let mut poly_indices = Vec::new();

                    for (_raw_idx_in_buffer, &raw_index) in raw_indices.iter().enumerate() {
                        // Decode index
                        let index = if raw_index < 0 {
                            (raw_index ^ -1) as usize
                        } else {
                            raw_index as usize
                        };

                        poly_indices.push(index);

                        // End of Polygon detected
                        if raw_index < 0 {
                            // Triangulate Fan (v0, v1, v2), (v0, v2, v3)...
                            if poly_indices.len() >= 3 {
                                let v0_idx = poly_indices[0];

                                for i in 1..poly_indices.len() - 1 {
                                    let v1_idx = poly_indices[i];
                                    let v2_idx = poly_indices[i + 1];

                                    // Add these 3 vertices
                                    // Note: We duplicate vertices here because FBX attributes (Normals/UVs)
                                    // are often ByPolygonVertex, meaning the same position can have different normals.
                                    // Meshopt will clean this up later.

                                    for &v_idx in &[v0_idx, v1_idx, v2_idx] {
                                        let p = control_points
                                            .get(v_idx)
                                            .ok_or(anyhow!("Index out of bounds"))?;

                                        // TODO: Implement robust Normal/UV extraction for FBX.
                                        // It requires handling MappingMode and ReferenceMode combinations.
                                        // For now, we load geometry correctly and default attributes.

                                        // Z-up to Y-up conversion (X, Y, Z) -> (X, Z, -Y)
                                        vertices.push(Vertex {
                                            pos: [p.x as f32, p.z as f32, -p.y as f32],
                                            normal: [0.0, 1.0, 0.0],
                                            uv: [0.0, 0.0],
                                        });

                                        indices.push(index_offset);
                                        index_offset += 1;
                                    }
                                }
                            }
                            poly_indices.clear();
                        }
                    }
                }
            }
            Ok((vertices, indices, None, None))
        }
        _ => Err(anyhow!("Unsupported FBX version (Must be 7.4 or newer)")),
    }
}

fn save_usdz(path: &PathBuf, vertices: &[Vertex], indices: &[u32]) -> Result<()> {
    // Basic USDA export (Text based USD) packed into ZIP (USDZ)
    // This is a placeholder. For real USDZ, use a library like `usd-rs` or write valid USDA structure.
    let file = File::create(path)?;
    let mut zip = zip::ZipWriter::new(file);

    let options =
        zip::write::FileOptions::<()>::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("model.usda", options)?;
    let mut w = zip; // writes to zip entry

    writeln!(w, "#usda 1.0")?;
    writeln!(w, "(    defaultPrim = \"Mesh\"")?;
    writeln!(w, "    upAxis = \"Y\"")?;
    writeln!(w, "    metersPerUnit = 1")?;
    writeln!(w, ")")?;

    writeln!(w, "def Mesh \"Mesh\" {{")?;
    writeln!(w, "    int[] faceVertexCounts = [")?;
    for _ in 0..indices.len() / 3 {
        write!(w, "3,")?;
    }
    writeln!(w, "]")?;

    writeln!(w, "    int[] faceVertexIndices = [")?;
    for i in indices {
        write!(w, "{},", i)?;
    }
    writeln!(w, "]")?;

    writeln!(w, "    point3f[] points = [")?;
    for v in vertices {
        write!(w, "({},{},{}),", v.pos[0], v.pos[1], v.pos[2])?;
    }
    writeln!(w, "]")?;

    // UVs would go here as primvars:st

    writeln!(w, "}}")?;

    zip = w;
    zip.finish()?;
    Ok(())
}

#[derive(Serialize)]
struct GltfHeader {
    asset: Asset,
    scenes: Vec<Scene>,
    nodes: Vec<Node>,
    meshes: Vec<Mesh>,
    buffers: Vec<Buffer>,
    bufferViews: Vec<BufferView>,
    accessors: Vec<Accessor>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<Image>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    textures: Vec<Texture>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    materials: Vec<Material>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    samplers: Vec<Sampler>,
}

#[derive(Serialize)]
struct Asset {
    version: String,
}

#[derive(Serialize)]
struct Scene {
    nodes: Vec<u32>,
}

#[derive(Serialize)]
struct Node {
    mesh: u32,
}

#[derive(Serialize)]
struct Mesh {
    primitives: Vec<Primitive>,
}

#[derive(Serialize)]
struct Primitive {
    attributes: Attributes,
    indices: u32,
    mode: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    material: Option<u32>,
}

#[derive(Serialize)]
struct Attributes {
    POSITION: u32,
    NORMAL: u32,
    TEXCOORD_0: u32,
}

#[derive(Serialize)]
struct Buffer {
    byteLength: usize,
}

#[derive(Serialize)]
struct BufferView {
    buffer: u32,
    byteOffset: usize,
    byteLength: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    byteStride: Option<usize>,
    target: u32,
}

#[derive(Serialize)]
struct Accessor {
    bufferView: u32,
    byteOffset: usize,
    componentType: u32,
    count: usize,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    min: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<[f32; 3]>,
}

#[derive(Serialize)]
struct Image {
    bufferView: u32,
    mimeType: String,
}

#[derive(Serialize)]
struct Texture {
    sampler: u32,
    source: u32,
}

#[derive(Serialize)]
struct Material {
    pbrMetallicRoughness: PbrMetallicRoughness,
}

#[derive(Serialize)]
struct PbrMetallicRoughness {
    #[serde(skip_serializing_if = "Option::is_none")]
    baseColorTexture: Option<TextureInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    baseColorFactor: Option<[f32; 4]>,
    metallicFactor: f32,
    roughnessFactor: f32,
}

#[derive(Serialize)]
struct TextureInfo {
    index: u32,
}

#[derive(Serialize)]
struct Sampler {
    magFilter: u32,
    minFilter: u32,
}

fn save_glb(
    path: &PathBuf,
    vertices: &[Vertex],
    indices: &[u32],
    texture: Option<&[u8]>,
    base_color: Option<[f32; 3]>,
) -> Result<()> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for v in vertices {
        for i in 0..3 {
            if v.pos[i] < min[i] {
                min[i] = v.pos[i];
            }
            if v.pos[i] > max[i] {
                max[i] = v.pos[i];
            }
        }
    }

    let indices_u8: &[u8] = bytemuck::cast_slice(indices);
    let vertices_u8: &[u8] = bytemuck::cast_slice(vertices);

    let i_pad = (4 - (indices_u8.len() % 4)) % 4;
    let v_pad = (4 - (vertices_u8.len() % 4)) % 4;

    // Calculate total length including texture
    let texture_len = texture.map(|t| t.len()).unwrap_or(0);
    let t_pad = if texture_len > 0 {
        (4 - (texture_len % 4)) % 4
    } else {
        0
    };

    let total_bin_len = indices_u8.len() + i_pad + vertices_u8.len() + v_pad + texture_len + t_pad;
    let stride = std::mem::size_of::<Vertex>();

    let mut buffer_views = vec![
        BufferView {
            buffer: 0,
            byteOffset: 0,
            byteLength: indices_u8.len(),
            byteStride: None,
            target: 34963,
        },
        BufferView {
            buffer: 0,
            byteOffset: indices_u8.len() + i_pad,
            byteLength: vertices_u8.len(),
            byteStride: Some(stride),
            target: 34962,
        },
    ];

    // Optional: Add Texture BufferView
    if let Some(tex_bytes) = texture {
        buffer_views.push(BufferView {
            buffer: 0,
            byteOffset: indices_u8.len() + i_pad + vertices_u8.len() + v_pad,
            byteLength: tex_bytes.len(),
            byteStride: None,
            target: 0, // 0 for Image data
        });
    }

    let mut images = Vec::new();
    let mut textures = Vec::new();
    let mut materials = Vec::new();
    let mut samplers = Vec::new();
    let mut material_idx = None;

    if texture.is_some() || base_color.is_some() {
        let mut pbr = PbrMetallicRoughness {
            baseColorTexture: None,
            baseColorFactor: None,
            metallicFactor: 0.0,
            roughnessFactor: 1.0,
        };

        if texture.is_some() {
            images.push(Image {
                bufferView: 2, // Index of the buffer view we just added
                mimeType: "image/png".to_string(),
            });
            samplers.push(Sampler {
                magFilter: 9729, // LINEAR
                minFilter: 9987, // LINEAR_MIPMAP_LINEAR (approx) or 9729
            });
            textures.push(Texture {
                sampler: 0,
                source: 0,
            });
            pbr.baseColorTexture = Some(TextureInfo { index: 0 });
        }

        if let Some(c) = base_color {
            // GLTF uses RGBA
            pbr.baseColorFactor = Some([c[0], c[1], c[2], 1.0]);
        }

        materials.push(Material {
            pbrMetallicRoughness: pbr,
        });
        material_idx = Some(0);
    }

    let header = GltfHeader {
        asset: Asset {
            version: "2.0".to_string(),
        },
        scenes: vec![Scene { nodes: vec![0] }],
        nodes: vec![Node { mesh: 0 }],
        meshes: vec![Mesh {
            primitives: vec![Primitive {
                attributes: Attributes {
                    POSITION: 1,
                    NORMAL: 2,
                    TEXCOORD_0: 3,
                },
                indices: 0,
                mode: 4,
                material: material_idx,
            }],
        }],
        buffers: vec![Buffer {
            byteLength: total_bin_len,
        }],
        bufferViews: buffer_views,
        accessors: vec![
            Accessor {
                bufferView: 0,
                byteOffset: 0,
                componentType: 5125,
                count: indices.len(),
                r#type: "SCALAR".to_string(),
                min: None,
                max: None,
            },
            Accessor {
                bufferView: 1,
                byteOffset: 0,
                componentType: 5126,
                count: vertices.len(),
                r#type: "VEC3".to_string(),
                min: Some(min),
                max: Some(max),
            },
            Accessor {
                bufferView: 1,
                byteOffset: 12,
                componentType: 5126,
                count: vertices.len(),
                r#type: "VEC3".to_string(),
                min: None,
                max: None,
            },
            Accessor {
                bufferView: 1,
                byteOffset: 24,
                componentType: 5126,
                count: vertices.len(),
                r#type: "VEC2".to_string(),
                min: None,
                max: None,
            },
        ],
        images,
        textures,
        materials,
        samplers,
    };

    let mut json_bytes = serde_json::to_vec(&header)?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(0x20);
    }

    let mut file = File::create(path)?;
    let total_size = 12 + 8 + json_bytes.len() as u32 + 8 + total_bin_len as u32;
    file.write_all(b"glTF")?;
    file.write_all(&2u32.to_le_bytes())?;
    file.write_all(&total_size.to_le_bytes())?;
    file.write_all(&(json_bytes.len() as u32).to_le_bytes())?;
    file.write_all(b"JSON")?;
    file.write_all(&json_bytes)?;
    file.write_all(&(total_bin_len as u32).to_le_bytes())?;
    file.write_all(b"BIN\0")?;
    file.write_all(indices_u8)?;
    for _ in 0..i_pad {
        file.write_all(&[0])?;
    }
    file.write_all(vertices_u8)?;
    for _ in 0..v_pad {
        file.write_all(&[0])?;
    }

    if let Some(tex_bytes) = texture {
        file.write_all(tex_bytes)?;
        for _ in 0..t_pad {
            file.write_all(&[0])?;
        }
    }

    Ok(())
}
