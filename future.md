# Future: Pure Rust Mesh Processing Pipeline

## Goal

Replace headless Blender (`remesh.py`) with a pure Rust implementation to eliminate process spawn overhead, reduce memory usage, and improve concurrency.

**Hardware context**: Hetzner dedicated server (i5-13500, 64GB RAM, no GPU) — see `hardware.md`

---

## Current Blender Pipeline Steps

When the UI "remesh" mode is selected, `remesh.py` executes these steps in order:

| # | Step | What It Does |
|---|------|--------------|
| 1 | **Import Model** | Parse GLB/GLTF/OBJ/FBX, extract mesh data |
| 2 | **Join Meshes** | Combine multiple mesh objects into single "HighPoly" |
| 3 | **Mesh Cleanup** | Merge duplicate vertices (0.0001 threshold), remove loose verts/edges |
| 4 | **Duplicate** | Create "LowPoly" copy for remeshing |
| 5 | **QuadriFlow Remesh** | Retopology to target face count (falls back to Decimate) |
| 6 | **UV Unwrap** | Smart UV Project + island packing |
| 7 | **Recalculate Normals** | Ensure consistent outward-facing normals |
| 8 | **Bake Diffuse** | Ray cast high→low, sample base color, write texture |
| 9 | **Bake Normal** | Ray cast, compute tangent-space normal differences |
| 10 | **Bake AO** | Hemisphere ray sampling for self-occlusion |
| 11 | **Bake Roughness** | Transfer roughness values via ray cast |
| 12 | **Bake Metallic** | Transfer metallic values via ray cast |
| 13 | **Pack ORM** | Combine AO(R), Roughness(G), Metallic(B) into single texture |
| 14 | **Export GLB** | Write mesh + textures to glTF binary |
| 15 | **Export USDZ** | Convert to Apple AR format |

---

## Rust Crate Maturity Assessment

### Import/Export

| Format | Crate | Maturity | Notes |
|--------|-------|----------|-------|
| **glTF/GLB** | [`gltf`](https://github.com/gltf-rs/gltf) | ✅ Mature | Used by Bevy, v1.4+, production-ready |
| **OBJ** | [`obj-rs`](https://crates.io/crates/obj-rs) | ✅ Stable | Simple format, works well |
| **FBX** | ❌ None | 🔴 Gap | Proprietary Autodesk format, no Rust option |
| **USDZ** | [`openusd`](https://github.com/mxpv/openusd) | 🟡 Early | v0.1.4, native Rust, reads all USD formats |

### Mesh Operations

| Operation | Crate | Maturity | Notes |
|-----------|-------|----------|-------|
| **Decimation** | [`meshopt`](https://lib.rs/crates/meshopt) | ✅ Mature | Bindings to meshoptimizer |
| **Vertex merge/cleanup** | [`meshopt`](https://lib.rs/crates/meshopt) | ✅ Mature | `generate_vertex_remap` |
| **Isotropic Remesh** | [`baby_shark`](https://github.com/dima634/baby_shark) | 🟡 Usable | Pure Rust, v0.3.12 |
| **QuadriFlow** | ❌ None | 🔴 Gap | Need FFI bindings to [C++ lib](https://github.com/hjwdzh/QuadriFlow) |

### UV Unwrapping

| Crate | Maturity | Notes |
|-------|----------|-------|
| [`xatlas-rs`](https://crates.io/crates/xatlas-rs) | 🟠 Stale | v0.1.3 (6 years old), but [xatlas C++](https://github.com/jpcy/xatlas) maintained (2024) |

**Action needed**: Fork/update bindings or write fresh FFI

### Ray Tracing (for Baking)

| Crate | Maturity | Notes |
|-------|----------|-------|
| [`embree4-rs`](https://crates.io/crates/embree4-rs) | ✅ Active | High-level wrapper for Embree 4 |
| [`embree4-sys`](https://github.com/psytrx/embree4-sys) | ✅ Active | Low-level FFI bindings |

**Key insight**: Blender Cycles already uses Embree internally for CPU ray tracing. Same core performance.

### Texture Baking

| Component | Status | Notes |
|-----------|--------|-------|
| **Baking framework** | 🔴 Gap | Build on Embree — main engineering effort |
| **Normal map math** | ✅ Trivial | Standard tangent-space calculation |
| **AO reference** | 🟡 Exists | [aobaker](https://github.com/prideout/aobaker) (C++) shows algorithm |
| **Image I/O** | ✅ Mature | `image` crate handles PNG/JPEG |

---

## Performance Comparison

### Overhead Breakdown

| Phase | Blender | Pure Rust | Savings |
|-------|---------|-----------|---------|
| Process spawn + startup | 2-5s | 0 | 2-5s |
| Python interpreter | 0.5-1s | 0 | ~1s |
| Scene graph setup | 0.3-0.5s | 0 | ~0.4s |
| Operator undo stacks | 0.5-2s | 0 | ~1s |
| **Fixed overhead total** | **~5s** | **~0.1s** | **~50x** |

### End-to-End Estimates

#### Small Model (10k faces, 1024 texture)
| | Blender | Rust | Speedup |
|-|---------|------|---------|
| Total | ~15-20s | ~8-12s | **1.5-2x** |

#### Medium Model (50k faces, 2048 texture)
| | Blender | Rust | Speedup |
|-|---------|------|---------|
| Total | ~45-60s | ~30-45s | **1.3-1.5x** |

#### Large Model (200k faces, 4096 texture)
| | Blender | Rust | Speedup |
|-|---------|------|---------|
| Total | ~3-5 min | ~2.5-4 min | **1.2-1.3x** |

**Pattern**: Speedup inversely proportional to model complexity (fixed overhead matters less for large jobs)

### Memory & Concurrency

| Metric | Blender | Pure Rust |
|--------|---------|-----------|
| Per-job overhead | ~500MB | ~100MB |
| 10 concurrent jobs | 5GB RAM | <2GB RAM |
| **Concurrent capacity** | 10 jobs | **25-30 jobs** |

---

## Recommended Approach

### Phase 1: Quick Wins (Low Effort)
- Move decimation to Rust (`meshopt`) — already have `mesh-worker`
- Move mesh cleanup to Rust (`meshopt`)
- Keep Blender for remesh/baking

### Phase 2: UV & Remeshing (Medium Effort)
- Update/fork `xatlas-rs` for UV unwrapping
- Evaluate `baby_shark` isotropic remesh vs QuadriFlow FFI bindings
- Alternative: Write QuadriFlow FFI bindings if quad topology required

### Phase 3: Baking Pipeline (High Effort)
Build custom baking on `embree4-rs`:
- Diffuse/color transfer
- Normal map baking
- AO baking
- Roughness/metallic transfer
- ORM packing

Reference: [aobaker](https://github.com/prideout/aobaker) shows Embree-based AO baking

### Phase 4: Full Integration
- USDZ export via `openusd` or keep minimal Blender call
- FBX import: require GLB conversion or keep Blender for FBX→GLB only

---

## Recommended Compromises

1. **Drop FBX** — require users to convert to GLB first, or keep minimal Blender call just for FBX→GLB
2. **Replace QuadriFlow** — use `baby_shark` isotropic remeshing (uniform triangles vs quad topology)
3. **USDZ via Blender** — until `openusd` matures, keep one Blender call for USDZ export only

---

## Summary

| Metric | Expected Improvement |
|--------|---------------------|
| Small job latency | 2x faster |
| Large job latency | 1.2x faster |
| Throughput | 1.5x more jobs/hour |
| Memory per job | 3-5x less |
| Concurrent capacity | 2-3x more |
| Deployment complexity | Simpler (no Blender/Python) |

**The real value**: Eliminating the fixed 5-second overhead on every job + dramatically better memory efficiency for concurrent workloads.

---

## Resources

- [embree4-rs](https://crates.io/crates/embree4-rs) — Rust Embree bindings
- [gltf-rs](https://github.com/gltf-rs/gltf) — glTF import/export
- [xatlas](https://github.com/jpcy/xatlas) — UV unwrapping (C++)
- [baby_shark](https://github.com/dima634/baby_shark) — Pure Rust geometry processing
- [meshopt-rs](https://github.com/gwihlidal/meshopt-rs) — Mesh optimization
- [openusd](https://github.com/mxpv/openusd) — Native Rust USD/USDZ
- [QuadriFlow](https://github.com/hjwdzh/QuadriFlow) — Quad remeshing (C++)
- [aobaker](https://github.com/prideout/aobaker) — Embree-based AO baking reference
