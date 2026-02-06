# Sketchfab Bulk Downloader

Download free 3D models from Sketchfab for testing Mesh Optimizer's batch processing.

## Setup

### 1. Get Your API Token

1. Create a free account at [sketchfab.com](https://sketchfab.com)
2. Go to [Account Settings > Password & API](https://sketchfab.com/settings/password)
3. Copy your **API Token**

### 2. Set Environment Variable

```bash
export SKETCHFAB_API_TOKEN="your_token_here"
```

To make it permanent, add to your `~/.zshrc` or `~/.bashrc`:
```bash
echo 'export SKETCHFAB_API_TOKEN="your_token_here"' >> ~/.zshrc
source ~/.zshrc
```

## Usage

### Basic - Download 75 Models

```bash
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models
```

### By Category

```bash
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models --category architecture
```

Available categories:
- `architecture`
- `cultural-heritage-history`
- `nature-plants`
- `food-drink`
- `furniture-home`
- `science-technology`
- `vehicles-transportation`

### Search Query (Recommended for Demos)

For larger, more complex files that showcase optimization:

```bash
# Photogrammetry scans (large files, high detail)
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models --search "photogrammetry"

# 3D scanned objects
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models --search "3d scan"

# Architectural models
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models --search "building interior"
```

### Adjust Parallelism

Default is 3 parallel downloads. Increase for faster downloads:

```bash
python scripts/tools/sketchfab_bulk_download.py --count 75 --output ./test_models --workers 5
```

## Output

```
./test_models/
├── 001_Ancient_Temple_Scan.glb
├── 002_Victorian_Chair.glb
├── 003_City_Block.gltf
├── ...
└── manifest.json
```

The `manifest.json` contains metadata about all downloaded models.

## Testing with Mesh Optimizer

After downloading, test batch processing:

### Via Web UI
1. Go to [webdeliveryengine.com](https://webdeliveryengine.com)
2. Drop files one at a time to test settings
3. Use free 24-hour re-optimization to dial in quality

### Via API (Batch)

```bash
# Process all downloaded models
for file in ./test_models/*.glb; do
  curl -X POST https://api.webdeliveryengine.com/optimize \
    -H "Authorization: Bearer YOUR_API_KEY" \
    -F "file=@$file" \
    -F "mode=decimate" \
    -F "quality=0.5"
done
```

## Troubleshooting

### "SKETCHFAB_API_TOKEN not set"
Make sure you exported the token in your current shell session.

### "403 Forbidden" on downloads
Some models require purchase even if marked downloadable. The script skips these automatically.

### "No models found"
Try a different search query or remove the category filter.

### Slow downloads
Reduce `--workers` to 2 if you're getting rate limited.

## Notes

- Only downloads models with free/CC licenses
- Prefers GLB format, falls back to GLTF or ZIP
- Respects Sketchfab API rate limits
- Models are sorted by popularity (most liked first)
