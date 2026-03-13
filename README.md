# MeshOpt

3D mesh optimization API for web and mobile delivery. Supports decimation (fast polygon reduction) and remeshing (retopology with texture baking) of GLB, GLTF, OBJ, FBX, and ZIP files.

Try it at [webdeliveryengine.com](https://webdeliveryengine.com)

## MCP Server

MeshOpt includes an MCP (Model Context Protocol) server so AI assistants can optimize 3D files directly from conversation.

**Works with:** Claude Desktop, Claude Code, Cursor, Windsurf, and any MCP-compatible client.

### Quick Start

1. **Download** the latest binary for your platform from [Releases](https://github.com/usermac/mesh-optimizer/releases/latest)
2. **Get an API key** at [webdeliveryengine.com](https://webdeliveryengine.com)
3. **Configure** your MCP client (see below)

### Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "meshopt": {
      "command": "/path/to/meshopt-mcp-server",
      "env": {
        "MESHOPT_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Cursor

Add to your Cursor MCP configuration:

```json
{
  "mcpServers": {
    "meshopt": {
      "command": "/path/to/meshopt-mcp-server",
      "env": {
        "MESHOPT_API_KEY": "your-api-key"
      }
    }
  }
}
```

### Available Tools

| Tool | Description |
|------|-------------|
| `optimize_mesh` | Optimize a single 3D file. Supports decimate and remesh modes. |
| `optimize_batch` | Optimize all matching files in a directory. |
| `check_balance` | Check your current credit balance. |
| `get_usage` | View your recent optimization history. |

### Example Prompts

```
Optimize ~/models/chair.glb to 50% quality for web
Batch optimize all GLB files in ~/assets/ using remesh mode
Check my MeshOpt credit balance
```

### Build from Source

```bash
cargo build --release -p meshopt-mcp-server
```

Binary will be at `target/release/meshopt-mcp-server`.

## Supported Formats

| Input | Output |
|-------|--------|
| GLB (recommended) | GLB |
| GLTF | USDZ |
| OBJ | Both |
| FBX | |
| ZIP | |

## Processing Modes

**Decimate** (1 credit) — Fast polygon reduction. Set a target ratio (0.0-1.0) to control how much geometry to keep.

**Remesh** (2 credits) — Full retopology via QuadriFlow with normal and diffuse texture baking. Set a target face count and texture resolution.

Re-optimizing the same file within 24 hours is free.

## Pricing

Purchase credits at [webdeliveryengine.com](https://webdeliveryengine.com). Free credits available to get started.

## License

MIT
