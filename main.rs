use clap::Parser;
use std::path::PathBuf;
use std::fs::File;
use std::io::Write;
use anyhow::{Result, Context};
use meshopt::VertexDataAdapter;
use serde::Serialize;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    input: PathBuf,
    #[arg(short, long)]
    output: PathBuf,
    #[arg(short, long, default_value_t = 0.5)]
    ratio: f32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    // Debug print to stdout so we know it started
    println!("RUST_START: Processing {:?}", args.input);

    let (models, _) = tobj::load_obj(
        &args.input,
        &tobj::LoadOptions { single_index: true, triangulate: true, ..Default::default() }
    ).context("Failed to load OBJ")?;

    if models.is_empty() { return Err(anyhow::anyhow!("No models found")); }

    let mesh = &models[0].mesh;
    let positions = &mesh.positions;
    let indices = &mesh.indices;

    let target_count = (indices.len() as f32 * args.ratio) as usize;
    let vertex_data_u8 = bytemuck::cast_slice(positions);
    
    let adapter = VertexDataAdapter::new(vertex_data_u8, 12, 0)?;
    let simplified_indices = meshopt::simplify(indices, &adapter, target_count, 0.01);

    println!("RUST_STATS: {} -> {} indices", indices.len(), simplified_indices.len());

    save_glb(&args.output, positions, &simplified_indices)?;
    println!("RUST_DONE");
    Ok(())
}

#[derive(Serialize)] struct GltfHeader { asset: Asset, scenes: Vec<Scene>, nodes: Vec<Node>, meshes: Vec<Mesh>, buffers: Vec<Buffer>, bufferViews: Vec<BufferView>, accessors: Vec<Accessor> }
#[derive(Serialize)] struct Asset { version: String }
#[derive(Serialize)] struct Scene { nodes: Vec<u32> }
#[derive(Serialize)] struct Node { mesh: u32 }
#[derive(Serialize)] struct Mesh { primitives: Vec<Primitive> }
#[derive(Serialize)] struct Primitive { attributes: Attributes, indices: u32, mode: u32 }
#[derive(Serialize)] struct Attributes { POSITION: u32 }
#[derive(Serialize)] struct Buffer { byteLength: usize }
#[derive(Serialize)] struct BufferView { buffer: u32, byteOffset: usize, byteLength: usize, target: u32 }
#[derive(Serialize)] struct Accessor { bufferView: u32, componentType: u32, count: usize, r#type: String, min: [f32; 3], max: [f32; 3] }

fn save_glb(path: &PathBuf, positions: &[f32], indices: &[u32]) -> Result<()> {
    let mut min = [f32::MAX; 3];
    let mut max = [f32::MIN; 3];
    for i in 0..positions.len() / 3 {
        let x = positions[i*3]; let y = positions[i*3+1]; let z = positions[i*3+2];
        if x < min[0] { min[0] = x; } if y < min[1] { min[1] = y; } if z < min[2] { min[2] = z; }
        if x > max[0] { max[0] = x; } if y > max[1] { max[1] = y; } if z > max[2] { max[2] = z; }
    }

    let indices_u8: &[u8] = bytemuck::cast_slice(indices);
    let positions_u8: &[u8] = bytemuck::cast_slice(positions);
    let i_pad = (4 - (indices_u8.len() % 4)) % 4;
    let p_pad = (4 - (positions_u8.len() % 4)) % 4;
    let total_bin_len = indices_u8.len() + i_pad + positions_u8.len() + p_pad;

    let header = GltfHeader {
        asset: Asset { version: "2.0".to_string() },
        scenes: vec![Scene { nodes: vec![0] }],
        nodes: vec![Node { mesh: 0 }],
        meshes: vec![Mesh { primitives: vec![Primitive { attributes: Attributes { POSITION: 1 }, indices: 0, mode: 4 }] }],
        buffers: vec![Buffer { byteLength: total_bin_len }],
        bufferViews: vec![
            BufferView { buffer: 0, byteOffset: 0, byteLength: indices_u8.len(), target: 34963 },
            BufferView { buffer: 0, byteOffset: indices_u8.len() + i_pad, byteLength: positions_u8.len(), target: 34962 },
        ],
        accessors: vec![
            Accessor { bufferView: 0, componentType: 5125, count: indices.len(), r#type: "SCALAR".to_string(), min: [0.0;3], max: [0.0;3] },
            Accessor { bufferView: 1, componentType: 5126, count: positions.len()/3, r#type: "VEC3".to_string(), min, max },
        ],
    };

    let mut json_bytes = serde_json::to_vec(&header)?;
    while json_bytes.len() % 4 != 0 { json_bytes.push(0x20); }

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
    for _ in 0..i_pad { file.write_all(&[0])?; }
    file.write_all(positions_u8)?;
    for _ in 0..p_pad { file.write_all(&[0])?; }

    Ok(())
}
