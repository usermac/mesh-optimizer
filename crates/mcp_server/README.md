# MeshOpt MCP Server

A [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server for the MeshOpt 3D mesh optimization service.

This server allows AI assistants like Claude Desktop, Cursor, and other MCP-compatible clients to optimize 3D mesh files directly.

## Features

- **optimize_mesh**: Optimize GLB, GLTF, OBJ, FBX, and ZIP files for web/mobile delivery
  - **Decimate mode**: Fast polygon reduction (1 credit)
  - **Remesh mode**: High-quality retopology with texture baking (2 credits)
- **check_balance**: View your current credit balance
- **get_usage**: View your optimization history

## Installation

### Build from Source

```bash
cargo build --release -p meshopt-mcp-server
```

The binary will be at `target/release/meshopt-mcp-server`.

### Download Pre-built Binary

Pre-built binaries are available on the [releases page](https://github.com/meshopt/meshopt-mcp-server/releases).

## Configuration

Set your API key as an environment variable:

```bash
export MESHOPT_API_KEY="your-api-key-here"
```

Get your API key at [webdeliveryengine.com](https://webdeliveryengine.com).

### Optional Environment Variables

- `MESHOPT_API_URL`: API base URL (default: `https://api.webdeliveryengine.com`)
- `MESHOPT_DEBUG`: Enable debug logging (set to `1` or `true`)

## Usage with Claude Desktop

Add to your Claude Desktop configuration (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "meshopt": {
      "command": "/path/to/meshopt-mcp-server",
      "env": {
        "MESHOPT_API_KEY": "your-api-key-here"
      }
    }
  }
}
```

## Usage with Cursor

Add to your Cursor MCP configuration:

```json
{
  "mcpServers": {
    "meshopt": {
      "command": "/path/to/meshopt-mcp-server",
      "env": {
        "MESHOPT_API_KEY": "your-api-key-here"
      }
    }
  }
}
```

## Tools

### optimize_mesh

Optimize a 3D mesh file.

**Parameters:**
- `file_path` (required): Path to the 3D model file. Supports `~` for home directory.
- `mode` (required): Processing mode - `"decimate"` or `"remesh"`
- `ratio` (optional): Target reduction ratio for decimate mode (0.0-1.0). Default: 0.5
- `faces` (optional): Target face count for remesh mode. Default: 10000
- `texture_size` (optional): Texture resolution for remesh mode (256, 512, 1024, 2048, 4096, 8192). Default: 1024
- `format` (optional): Output format - `"glb"`, `"usdz"`, or `"both"`. Default: `"glb"`
- `output_dir` (optional): Output directory. Default: same directory as input file

**Example:**
```
Optimize ~/models/character.glb using decimate mode with 50% reduction
```

### check_balance

Check your current credit balance.

**Example:**
```
Check my MeshOpt credit balance
```

### get_usage

View your usage history.

**Parameters:**
- `limit` (optional): Maximum entries to return (1-100). Default: 10

**Example:**
```
Show my last 5 MeshOpt optimizations
```

## Pricing

- **Decimate mode**: 1 credit per optimization
- **Remesh mode**: 2 credits per optimization
- **Free re-optimization**: Re-optimizing the same file within 24 hours is free

Purchase credits at [webdeliveryengine.com](https://webdeliveryengine.com).

## Supported File Formats

- GLB (recommended)
- GLTF
- OBJ
- FBX
- ZIP (containing any of the above)
- USDZ (output only)

## License

MIT
