# MeshOpt: A Git Journey

**250 commits across 29 days** — from synced server code to a full SaaS product with newsletter integration.

---

## The Beginning: Phase 3 Baseline (Dec 2)

The first commit lands: `Baseline: Phase 3 Complete (Synced from Server)`. This isn't the true beginning — the project clearly had prior life on a server somewhere — but it's where version control begins for this chapter.

*Editor: Unknown*
*AI: Unknown*

---

## The Sprint: Phases 4-8 in a Single Day (Dec 3)

Five commits. One day. The pace is frantic:

- **Phase 4**: Full fidelity (normals + UVs)
- **Phase 5**: 2GB upload limit, 20-minute timeouts
- **Phase 6**: Dashboard UI and ratio control
- **Phase 8**: Payments fully working

> "SaaS is LIVE."

Phase 7 is mysteriously absent. Perhaps it was the friends we made along the way.

---

## The Tooling Pivot (Dec 4)

```
2025-12-04 Moved to Zed editor with paid Gemini 3 key
```

A single commit marks a major shift. **Zed** becomes the editor of choice, and **Google Gemini 3** enters as the AI pair programmer. The future has arrived.

*Editor: Zed*
*AI: Gemini 3*

---

## Format Wars & USDZ (Dec 5-6)

The mesh optimizer grows teeth:

- FBX and GLB support added
- SSL persistence fixed
- USDZ export for Apple ecosystem
- FBX rotation bugs squashed
- Texture resizing and solid color support for OBJ

The workflow improves. The UI polishes.

---

## Architecture: The Cargo Workspace Era (Dec 7)

A pivotal refactor:

```
Converte to cargo workspaces
```

(Yes, "Converte" — move fast and break spelling.)

The system becomes "cleaner, faster, and fully separated." The monolith splits into `mesh-api` and `mesh-worker` crates. Input filesize jumps to 5GB. This is enterprise territory.

---

## Security & Billing Foundations (Dec 8)

Six commits bring grown-up concerns:

- `.env` created for secrets
- Docker deployment scripts (`ship.sh`)
- SQLite metrics recording
- **Ledger-based billing** with fair-use logic
- Transaction history UI

The SaaS is getting serious.

---

## The Multi-AI Day (Dec 9)

December 9th is fascinating — **two different AI models** appear in commit messages:

```
Haiku 4.5 Implemented complete credit/billing transaction system...
```

and later:

```
Gemini 3 pro - Refactor optimization to async background tasks with polling
```

**Claude Haiku 4.5** handles the billing and transaction systems — CSV downloads, search/filter, UX improvements. **Gemini 3 Pro** tackles the harder infrastructure problem: async background jobs with polling to prevent HTTP timeouts on large meshes.

The developer is tool-switching based on task. Different strengths for different jobs.

*AI: Haiku 4.5 + Gemini 3 Pro (task-based switching)*

---

## Blender Integration (Dec 10)

The project levels up with professional-grade features:

- ZIP upload support for multi-file OBJ packages
- **Blender 4.1 integration** for QuadriFlow remeshing
- Weighted concurrency (heavy Blender jobs cost more slots)
- Tiered pricing (Decimate vs Remesh)
- User guide and help pages

The README writes itself: this isn't just polygon reduction anymore — it's a full retopology service.

---

## The Great CSS Consolidation (Dec 11)

Fifteen commits of frontend cleanup:

- Inline CSS extracted to `styles.css`
- Semantic HTML elements (`<main>`, `<aside>`)
- Fixed sidebar layout
- localStorage for API key persistence
- Test key removed from production

The codebase grows up. The HTML stops looking like it was written at 3 AM.

---

## The 35-Commit Marathon (Dec 12)

The busiest day. Thirty-five commits. Some highlights:

- Health check scripts and Blender watchdogs
- Critical bug: "Blender remesh feature - was completely broken" (fixed thrice!)
- **Sonnet 4.5** appears: `100% of critical issues resolved per Sonnet 4.5 in launch_prep.md`
- AES-256-GCM encryption for database.json
- CORS restriction to webdeliveryengine.com
- Rate-limited admin endpoints
- Market analysis and revenue projections

**Claude Sonnet 4.5** enters as the launch preparation auditor.

*AI: Sonnet 4.5 (launch review)*

---

## The Model Experiment (Dec 13)

```
2025-12-13 attempted change to G35P using Ultra account +api key
```

An upgrade attempt to **Gemini 3.5 Pro** with a paid Ultra account. Later that day:

```
Post AI review G3P & C45, minor clean up before heavy testing before launch
```

"G3P & C45" — **Gemini 3 Pro and Claude 4.5** working in tandem. The multi-model approach continues.

The day also brings hot-reloadable pricing configs, purchase modal refinements, and accessibility improvements.

---

## API Documentation & Claude Code Arrives (Dec 14-15)

Accessibility gets serious — WCAG 2.1 AA compliance, skip navigation, ARIA attributes.

And then:

```
docs: Add CLAUDE.md for Claude Code context
```

**Claude Code** officially enters the workflow. The CLAUDE.md file gives the AI context about the project architecture, build commands, and patterns.

*AI: Claude Code (officially adopted)*

---

## Easter Eggs & Developer Joy (Dec 16-17)

The project gets personality:

```
feat: Add easter egg - funny status messages while processing
```

Context-aware jokes based on Decimate vs Remesh mode. A Founder's Story easter egg modal appears. The product has soul now.

Batch script generators, PowerShell support, and hyphen-free UUIDs (double-click selectable!) round out the week.

---

## Testing & Polish (Dec 18-19)

Unit tests arrive for billing logic. The search gets date ranges. CSV filenames become intelligent. Single ISO date search works. Multi-term search lands.

The product is hardening for production.

---

## The Content Explosion (Dec 26)

Twenty commits in one day. **Claude Code with Opus 4.5** is clearly driving:

```
🤖 Generated with [Claude Code](https://claude.com/claude-code)
Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
```

The additions are massive:

- **Blog section** with Industrial HUD design
- **GLB Inspector tool** with 3D preview
- **Polygon Budget Calculator**
- **Texture Size Calculator**
- Comparison pages: MeshOpt vs Blender, Simplygon, InstaLOD, RapidCompact
- Shopify AR guide, Three.js performance guide, OBJ to GLB guide

Content marketing meets engineering. The product gains a voice.

*AI: Claude Code (Opus 4.5)*

---

## The Final Push (Dec 27-30)

Bug fixes and refinements:

- 512px texture size option for mobile/AR
- Blog search functionality
- Auto-expanding Nightmare Banner for new users
- **Critical fix**: OBJ UV V-coordinate flip for inverted textures
- Deploy script hardening to prevent database wipes
- Backup system test suite
- Newsletter integration via Listmonk

---

## The Numbers

| Metric | Value |
|--------|-------|
| Total commits | 250 |
| Days active | 29 |
| Peak day | Dec 12 (35 commits) |
| Editors used | Zed |
| AI models used | Gemini 3, Gemini 3 Pro, Haiku 4.5, Sonnet 4.5, Claude Code (Opus 4.5) |

---

## The AI Evolution

```
Dec 2-3:  Unknown
Dec 4:    Gemini 3 (Zed integration)
Dec 9:    Haiku 4.5 (billing) + Gemini 3 Pro (infrastructure)
Dec 12:   Sonnet 4.5 (launch audit)
Dec 13:   Gemini 3.5 Pro Ultra (attempted) + Claude 4.5
Dec 15+:  Claude Code (Opus 4.5) — primary going forward
Dec 24:   Gemini (one-off script)
Dec 26+:  Claude Code (Opus 4.5) — confirmed via commit signatures
```

The pattern: **task-appropriate model selection**. Smaller models for focused work, larger models for architecture and content. Eventually, Claude Code becomes the daily driver.

---

## The Editor Story

**Zed** from December 4th onward. Clean, fast, with built-in AI support. No evidence of switching back.

---

## What Was Built

A mesh optimization SaaS that:

- Accepts GLB, GLTF, OBJ, FBX, and ZIP files up to 5GB
- Offers two modes: fast Decimate and high-quality Remesh (via Blender)
- Exports GLB and USDZ for web and Apple AR
- Has credit-based billing with Stripe integration
- Provides full API access with curl/Python/PowerShell generators
- Includes free tools (GLB Inspector, calculators)
- Has a blog with SEO-optimized content
- Meets WCAG 2.1 AA accessibility standards
- Runs in Docker with automated backups and health monitoring

All in 29 days. With AI assistance. In Zed.

---

*Generated with Claude Code on December 30, 2025*
