# MCP Server Plan for MeshOpt

Future implementation plan for adding MCP (Model Context Protocol) support to MeshOpt.

## Why MCP?

- Lets users call MeshOpt directly from AI assistants (Claude Desktop, Cursor, Windsurf)
- End user pays their own LLM costs—MCP server is just a thin wrapper around the API
- Targets technical/developer users who already use AI coding assistants
- Differentiates from competitors, signals modern developer experience

## Architecture Decisions

### Authentication
- API key read from `MESHOPT_API_KEY` environment variable in MCP client config
- User adds key once, all requests authenticated automatically
- Standard pattern, secure, clean UX

### Credit Enforcement
- MCP server is stateless pass-through
- Existing API checks credits, deducts, or returns clear error
- If user runs out mid-batch, API returns friendly error message
- Claude relays the error naturally to the user

### Error Messages
- Human-readable since Claude will quote them to users
- Example: `{"error": "Insufficient credits", "balance": 0, "message": "Purchase more at webdeliveryengine.com"}`

## Tools to Expose

### 1. optimize_mesh
Upload local 3D file, optimize via API, return result

**Inputs:**
- `file_path` (required): Local path to GLB/GLTF/OBJ/FBX/ZIP
- `mode`: "decimate" (1 credit) or "remesh" (2 credits)
- `ratio` / `target_percentage` / `target_faces`: Reduction amount for decimate
- `faces`: Target face count for remesh (default: 5000)
- `texture_size`: 256-8192 for remesh baking (default: 2048)
- `format`: "glb", "usdz", or "both"
- `download_path` (optional): Save result locally

**Behavior:** Read file → upload to `/optimize` → poll `/job/:id` → return URLs

### 2. check_balance
Returns `{ credits: number, key_type: "paid" | "free" }`

### 3. get_usage_history
Returns recent transactions with amounts and descriptions

## Technology Choice

### Option A: TypeScript (Recommended)
- **Pros:** Mature MCP SDK, 95%+ of MCP servers use it, best documented
- **Cons:** Adds Node.js runtime dependency
- **Note:** Runs on user's machine, not production server. API stays pure Rust.

### Option B: Rust
- **Pros:** Same language as API, leaner runtime (~10MB vs ~100MB)
- **Cons:** `mcp-rust-sdk` less mature, fewer examples, binary distribution complexity
- **Performance:** Negligible difference—MCP server is I/O-bound, optimization happens server-side

### Recommendation
Start with TypeScript for faster development and proven ecosystem. Consider Rust later if MCP SDK matures.

## API Endpoints to Wrap

```
POST /optimize        - Multipart upload, returns jobId
GET  /job/:id         - Poll until Completed/Failed
GET  /credits         - Returns { credits: number }
GET  /history         - Returns last 50 transactions
```

All protected endpoints use `Authorization: Bearer <api_key>` header.

## Example User Experience

**User in Claude:**
> "Optimize ~/models/character.glb to 50% of original faces"

**Claude calls MCP tool, which hits API, deducts credits, returns URL**

**Claude responds:**
> "Done! I optimized character.glb from 50,000 to 25,000 faces. Download: [URL]. 1 credit used, 19 remaining."

## Example Claude Desktop Config

```json
{
  "mcpServers": {
    "meshopt": {
      "command": "npx",
      "args": ["-y", "@meshopt/mcp-server"],
      "env": {
        "MESHOPT_API_KEY": "sk_YYMMDD_xxxxxxxx"
      }
    }
  }
}
```

## Implementation Notes

- MCP server should poll with exponential backoff (1s initial, 10s max, 10min timeout)
- Return download URLs by default (expire in 1 hour), optional local download
- Parse `X-Credits-Remaining` header from optimize response
- Key type derivable from prefix: `sk_` = paid, `fr_` = free

## Future Considerations

- Could add `list_jobs` tool for job history beyond credit transactions
- Could add `cancel_job` if API supports it
- Consider MCP "resources" for exposing pricing info or documentation
