use anyhow::{Context, Result};
use clap::Parser;
use meshopt::VertexDataAdapter;
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(short, long, default_value_t = 0.5)]
    ratio: f32,
}

// NEW: A "Fat Vertex" that holds everything a GPU needs
// #[repr(C)] guarantees C-style memory layout (no padding)
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("RUST_START: Processing {:?}", args.input);

    // Load OBJ with "single_index" (Unifies P/N/UV into one index buffer)
    let (models, _) = tobj::load_obj(
        &args.input,
        &tobj::LoadOptions {
            single_index: true,
            triangulate: true,
            ignore_lines: true,
            ignore_points: true,
        },
    )
    .context("Failed to load OBJ")?;

    if models.is_empty() {
        return Err(anyhow::anyhow!("No models found"));
    }

    let mesh = &models[0].mesh;

    // --- STEP 1: CONSTRUCT FAT VERTICES ---
    // We need to merge the separate float arrays from tobj into our nice struct
    let n_vertices = mesh.positions.len() / 3;
    let mut vertices = Vec::with_capacity(n_vertices);

    for i in 0..n_vertices {
        let pos = [
            mesh.positions[i * 3],
            mesh.positions[i * 3 + 1],
            mesh.positions[i * 3 + 2],
        ];

        // Handle missing normals (Default to Up vector)
        let normal = if !mesh.normals.is_empty() {
            [
                mesh.normals[i * 3],
                mesh.normals[i * 3 + 1],
                mesh.normals[i * 3 + 2],
            ]
        } else {
            [0.0, 1.0, 0.0]
        };

        // Handle missing UVs (Default to 0,0)
        let uv = if !mesh.texcoords.is_empty() {
            [mesh.texcoords[i * 2], mesh.texcoords[i * 2 + 1]]
        } else {
            [0.0, 0.0]
        };

        vertices.push(Vertex { pos, normal, uv });
    }

    let indices = &mesh.indices;
    println!("STATS: {} verts, {} indices", vertices.len(), indices.len());

    // --- STEP 2: OPTIMIZE ---
    let target_count = (indices.len() as f32 * args.ratio) as usize;

    // Cast our Vertex struct to raw bytes for meshopt
    let vertex_data_u8 = bytemuck::cast_slice(&vertices);

    // Stride is now 32 bytes (3+3+2 floats * 4 bytes)
    let stride = std::mem::size_of::<Vertex>();
    let adapter = VertexDataAdapter::new(vertex_data_u8, stride, 0)
        .map_err(|e| anyhow::anyhow!("Adapter error: {:?}", e))?;

    let simplified_indices = meshopt::simplify(indices, &adapter, target_count, 0.01);

    println!(
        "OPTIMIZED: {} -> {} indices",
        indices.len(),
        simplified_indices.len()
    );

    // --- STEP 3: EXPORT ---
    save_glb(&args.output, &vertices, &simplified_indices)?;
    println!("RUST_DONE");
    Ok(())
}

// --- GLB WRITER (Updated for Interleaved Data) ---

#[derive(Serialize)]
struct GltfHeader {
    asset: Asset,
    scenes: Vec<Scene>,
    nodes: Vec<Node>,
    meshes: Vec<Mesh>,
    buffers: Vec<Buffer>,
    bufferViews: Vec<BufferView>,
    accessors: Vec<Accessor>,
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
}
// Attributes now track Normal and UV (TEXCOORD_0)
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
    byteStride: Option<usize>,
    target: u32,
}
#[derive(Serialize)]
struct Accessor {
    bufferView: u32,
    byteOffset: usize,
    componentType: u32,
    count: usize,
    r#type: String,
    min: Option<[f32; 3]>,
    max: Option<[f32; 3]>,
}

// Safety check for casting
unsafe impl bytemuck::Pod for Vertex {}
unsafe impl bytemuck::Zeroable for Vertex {}

fn save_glb(path: &PathBuf, vertices: &[Vertex], indices: &[u32]) -> Result<()> {
    // Calculate Bounding Box
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

    // Padding
    let i_pad = (4 - (indices_u8.len() % 4)) % 4;
    let v_pad = (4 - (vertices_u8.len() % 4)) % 4;
    let total_bin_len = indices_u8.len() + i_pad + vertices_u8.len() + v_pad;
    let stride = std::mem::size_of::<Vertex>(); // 32 bytes

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
            }],
        }],
        buffers: vec![Buffer {
            byteLength: total_bin_len,
        }],
        bufferViews: vec![
            // View 0: Indices (Scalar)
            BufferView {
                buffer: 0,
                byteOffset: 0,
                byteLength: indices_u8.len(),
                byteStride: None,
                target: 34963,
            },
            // View 1: Interleaved Vertices (Pos + Norm + UV)
            BufferView {
                buffer: 0,
                byteOffset: indices_u8.len() + i_pad,
                byteLength: vertices_u8.len(),
                byteStride: Some(stride),
                target: 34962,
            },
        ],
        accessors: vec![
            // 0: Indices
            Accessor {
                bufferView: 0,
                byteOffset: 0,
                componentType: 5125,
                count: indices.len(),
                r#type: "SCALAR".to_string(),
                min: None,
                max: None,
            },
            // 1: Position (Offset 0)
            Accessor {
                bufferView: 1,
                byteOffset: 0,
                componentType: 5126,
                count: vertices.len(),
                r#type: "VEC3".to_string(),
                min: Some(min),
                max: Some(max),
            },
            // 2: Normal (Offset 12 bytes - after 3 floats)
            Accessor {
                bufferView: 1,
                byteOffset: 12,
                componentType: 5126,
                count: vertices.len(),
                r#type: "VEC3".to_string(),
                min: None,
                max: None,
            },
            // 3: UV (Offset 24 bytes - after 6 floats)
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
    };

    // Standard GLB Writing Boilerplate
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

    Ok(())
}
