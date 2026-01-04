#!/usr/bin/env node
/**
 * build-help-index.js
 *
 * Parses index.html to extract searchable content from all modals.
 * Generates server/public/help-index.json for the in-app help search feature.
 *
 * Run: node scripts/build-help-index.js
 * Called automatically by deploy.sh before rsync.
 */

const fs = require('fs');
const path = require('path');

// Paths
const INDEX_HTML_PATH = path.join(__dirname, '..', 'server', 'public', 'index.html');
const OUTPUT_PATH = path.join(__dirname, '..', 'server', 'public', 'help-index.json');

// Modal ID to friendly name mapping
const MODAL_NAMES = {
    'helpModal': 'Help & Guide',
    'apiDocsModal': 'API Documentation',
    'apiCodeModal': 'API Code Generator',
    'batchScriptModal': 'Batch Script Generator',
    'mcpModal': 'MCP Integration',
    'purchaseModal': 'Buy Credits',
    'historyModal': 'Transaction History',
    'contactModal': 'Contact Us',
    'privacyModal': 'Privacy Policy',
    'tosModal': 'Terms of Service',
    'freeCreditsModal': 'Get Free Credits',
    'changelogModal': 'Changelog',
    'founderModal': "Founder's Story",
    'apiSettingsModal': 'API Settings'
};

// Read and parse HTML
function parseHTML(html) {
    const entries = [];

    // Find all modal divs
    const modalRegex = /<div\s+id="([^"]+Modal)"\s+class="modal-overlay"[^>]*>([\s\S]*?)<\/div>\s*(?=<div\s+(?:id="[^"]+Modal"|class="d-flex)|<script|$)/gi;

    // Simpler approach: find each modal by its opening tag and extract content
    const modalIds = Object.keys(MODAL_NAMES);

    for (const modalId of modalIds) {
        const modalName = MODAL_NAMES[modalId];

        // Find the modal content - look for the modal div and extract until we hit another modal or script
        const startPattern = new RegExp(`<div\\s+id="${modalId}"\\s+class="modal-overlay"`, 'i');
        const startMatch = html.match(startPattern);

        if (!startMatch) continue;

        const startIndex = html.indexOf(startMatch[0]);
        if (startIndex === -1) continue;

        // Find the end of this modal (next modal or script tag)
        let endIndex = html.length;
        for (const otherId of modalIds) {
            if (otherId === modalId) continue;
            const otherPattern = new RegExp(`<div\\s+id="${otherId}"\\s+class="modal-overlay"`, 'i');
            const otherMatch = html.substring(startIndex + 100).match(otherPattern);
            if (otherMatch) {
                const idx = html.indexOf(otherMatch[0], startIndex + 100);
                if (idx !== -1 && idx < endIndex) {
                    endIndex = idx;
                }
            }
        }

        // Also check for script tag
        const scriptIdx = html.indexOf('<script>', startIndex + 100);
        if (scriptIdx !== -1 && scriptIdx < endIndex) {
            endIndex = scriptIdx;
        }

        const modalContent = html.substring(startIndex, endIndex);

        // Extract sections with headings
        extractSections(modalContent, modalId, modalName, entries);
    }

    // Also add sidebar features (not in modals)
    addSidebarFeatures(html, entries);

    return entries;
}

// Extract sections from modal content
function extractSections(content, modalId, modalName, entries) {
    // Find all h3 and h4 headings with their following content
    const sectionRegex = /<h([34])[^>]*>([^<]+(?:<[^>]+>[^<]*<\/[^>]+>)?[^<]*)<\/h[34]>/gi;
    let match;

    const headings = [];
    while ((match = sectionRegex.exec(content)) !== null) {
        const level = match[1];
        const headingText = stripTags(match[2]).trim();
        const position = match.index;
        headings.push({ level, text: headingText, position });
    }

    // For each heading, extract the content until the next heading of same or higher level
    for (let i = 0; i < headings.length; i++) {
        const heading = headings[i];
        const nextHeading = headings[i + 1];

        const startPos = heading.position;
        const endPos = nextHeading ? nextHeading.position : content.length;

        const sectionContent = content.substring(startPos, endPos);

        // Extract text content from this section
        const textContent = extractTextContent(sectionContent);

        // Extract code keywords
        const codeKeywords = extractCodeKeywords(sectionContent);

        // Try to find a section ID
        const sectionId = findSectionId(content, heading.position);

        entries.push({
            text: heading.text,
            modal: modalId,
            modalName: modalName,
            section: sectionId,
            context: textContent.substring(0, 200).trim(),
            keywords: codeKeywords
        });
    }

    // If no headings found, just extract the whole modal content
    if (headings.length === 0) {
        const textContent = extractTextContent(content);
        entries.push({
            text: modalName,
            modal: modalId,
            modalName: modalName,
            section: null,
            context: textContent.substring(0, 300).trim(),
            keywords: extractCodeKeywords(content)
        });
    }
}

// Add sidebar features that aren't in modals
function addSidebarFeatures(html, entries) {
    // Find the sidebar content
    const sidebarMatch = html.match(/<aside[^>]*class="sidebar"[^>]*>([\s\S]*?)<\/aside>/i);
    if (!sidebarMatch) return;

    const sidebar = sidebarMatch[1];

    // Add key sidebar features
    entries.push({
        text: 'Upload 3D File',
        modal: null,
        modalName: 'Main Page',
        section: 'dropzone',
        context: 'Drag and drop your 3D file here. Supports GLB, OBJ, FBX, GLTF, and ZIP files.',
        keywords: ['upload', 'drag', 'drop', 'glb', 'obj', 'fbx', 'gltf', 'zip', 'file']
    });

    entries.push({
        text: 'Optimization Mode',
        modal: null,
        modalName: 'Main Page',
        section: 'modeSelect',
        context: 'Choose between Decimate (reduce polygon count) or Remesh + Bake (retopology with texture baking).',
        keywords: ['decimate', 'remesh', 'bake', 'retopology', 'polygon', 'reduce', 'mode']
    });

    entries.push({
        text: 'Target Quality Slider',
        modal: null,
        modalName: 'Main Page',
        section: 'ratio',
        context: 'Adjust the quality ratio for decimation. Lower means less detail, higher means more detail.',
        keywords: ['quality', 'ratio', 'slider', 'percentage', 'detail']
    });

    entries.push({
        text: 'Target Faces',
        modal: null,
        modalName: 'Main Page',
        section: 'targetFaces',
        context: 'Set the target number of faces for remesh mode.',
        keywords: ['faces', 'target', 'count', 'polygon', 'remesh']
    });

    entries.push({
        text: 'Texture Size',
        modal: null,
        modalName: 'Main Page',
        section: 'textureSize',
        context: 'Select texture resolution for remesh: 1024, 2048, or 4096 pixels.',
        keywords: ['texture', 'size', 'resolution', 'pixels', '1024', '2048', '4096']
    });

    entries.push({
        text: 'API Key',
        modal: null,
        modalName: 'Main Page',
        section: 'apiKey',
        context: 'Enter your API key to authenticate. Keys start with sk_ or fr_.',
        keywords: ['api', 'key', 'authentication', 'sk_', 'fr_', 'login']
    });

    entries.push({
        text: 'Material and Textures',
        modal: null,
        modalName: 'Main Page',
        section: 'objInputs',
        context: 'For OBJ files, add optional .mtl material file and texture images.',
        keywords: ['material', 'mtl', 'texture', 'obj', 'png', 'jpg', 'images']
    });
}

// Strip HTML tags from text
function stripTags(html) {
    return html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
}

// Extract text content, preserving some structure
function extractTextContent(html) {
    // Remove script and style tags
    let text = html.replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '');
    text = text.replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '');

    // Convert list items to text with bullets
    text = text.replace(/<li[^>]*>/gi, ' - ');

    // Add space for block elements
    text = text.replace(/<\/(p|div|li|h[1-6]|tr|td|th)>/gi, ' ');

    // Strip remaining tags
    text = stripTags(text);

    // Clean up whitespace
    text = text.replace(/\s+/g, ' ').trim();

    return text;
}

// Extract code/technical keywords
function extractCodeKeywords(html) {
    const keywords = new Set();

    // Extract content from <code> tags
    const codeRegex = /<code[^>]*>([^<]+)<\/code>/gi;
    let match;
    while ((match = codeRegex.exec(html)) !== null) {
        const code = match[1].trim().toLowerCase();
        if (code.length > 1 && code.length < 50) {
            keywords.add(code);
        }
    }

    // Extract strong/bold keywords
    const strongRegex = /<strong[^>]*>([^<]+)<\/strong>/gi;
    while ((match = strongRegex.exec(html)) !== null) {
        const text = match[1].trim().toLowerCase();
        // Only add short, single-word terms as keywords
        if (text.length > 2 && text.length < 30 && !text.includes(' ')) {
            keywords.add(text);
        }
    }

    return Array.from(keywords);
}

// Find section ID near a position
function findSectionId(content, position) {
    // Look backwards for a section with an ID
    const before = content.substring(Math.max(0, position - 500), position);
    const idMatch = before.match(/id="([^"]+)"\s*$/i) || before.match(/id="([^"]+)"[^>]*>\s*$/i);

    // Look for content-section class
    const sectionMatch = before.match(/<section[^>]*class="content-section"[^>]*>/gi);

    return null; // Most sections don't have IDs, so we'll scroll to them by search
}

// Main
function main() {
    console.log('Building help search index...');

    if (!fs.existsSync(INDEX_HTML_PATH)) {
        console.error(`Error: ${INDEX_HTML_PATH} not found`);
        process.exit(1);
    }

    const html = fs.readFileSync(INDEX_HTML_PATH, 'utf8');
    const entries = parseHTML(html);

    // Deduplicate and clean entries
    const seen = new Set();
    const cleanedEntries = entries.filter(entry => {
        const key = `${entry.modal}:${entry.text}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return entry.text && entry.text.length > 0;
    });

    // Write output
    const output = {
        generated: new Date().toISOString(),
        version: '1.0',
        entries: cleanedEntries
    };

    fs.writeFileSync(OUTPUT_PATH, JSON.stringify(output, null, 2));

    console.log(`Generated ${cleanedEntries.length} search entries`);
    console.log(`Output: ${OUTPUT_PATH}`);
}

main();
