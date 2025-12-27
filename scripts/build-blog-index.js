#!/usr/bin/env node
/**
 * build-blog-index.js
 *
 * Parses blog HTML files to extract searchable content.
 * Generates server/public/blog/blog-index.json for the blog search feature.
 *
 * Run: node scripts/build-blog-index.js
 * Called automatically by deploy.sh before rsync.
 */

const fs = require('fs');
const path = require('path');

// Paths
const BLOG_DIR = path.join(__dirname, '..', 'server', 'public', 'blog');
const OUTPUT_PATH = path.join(BLOG_DIR, 'blog-index.json');

// Parse a single blog article
function parseArticle(filePath) {
    const html = fs.readFileSync(filePath, 'utf8');
    const fileName = path.basename(filePath);
    const entries = [];

    // Skip index.html - it's the listing page, not an article
    if (fileName === 'index.html') {
        return entries;
    }

    // Extract article metadata
    const titleMatch = html.match(/<title>([^|<]+)/);
    const descMatch = html.match(/<meta\s+name="description"\s+content="([^"]+)"/);
    const categoryMatch = html.match(/<span\s+class="category">([^<]+)<\/span>/);
    const dateMatch = html.match(/<span>([A-Za-z]+\s+\d{4})<\/span>/);

    const articleTitle = titleMatch ? titleMatch[1].trim() : fileName.replace('.html', '');
    const description = descMatch ? descMatch[1] : '';
    const category = categoryMatch ? categoryMatch[1] : 'Article';
    const date = dateMatch ? dateMatch[1] : '';
    const url = `/blog/${fileName}`;

    // Add entry for the article itself
    entries.push({
        text: articleTitle,
        url: url,
        category: category,
        date: date,
        context: description,
        keywords: extractKeywords(html)
    });

    // Extract sections (h2 headings)
    const h2Regex = /<h2>([^<]+)<\/h2>/gi;
    let h2Match;
    const h2Positions = [];

    while ((h2Match = h2Regex.exec(html)) !== null) {
        h2Positions.push({
            text: h2Match[1].trim(),
            position: h2Match.index
        });
    }

    // For each h2, extract content until next h2 or end of article
    for (let i = 0; i < h2Positions.length; i++) {
        const section = h2Positions[i];
        const nextSection = h2Positions[i + 1];

        const startPos = section.position;
        const endPos = nextSection ? nextSection.position : html.indexOf('</article>');

        if (endPos > startPos) {
            const sectionContent = html.substring(startPos, endPos);
            const contextText = extractTextContent(sectionContent).substring(0, 200).trim();

            // Create anchor ID from heading text
            const anchorId = section.text
                .toLowerCase()
                .replace(/[^a-z0-9\s]/g, '')
                .replace(/\s+/g, '-');

            entries.push({
                text: section.text,
                url: `${url}#${anchorId}`,
                category: category,
                date: date,
                context: contextText,
                parentArticle: articleTitle,
                keywords: extractCodeKeywords(sectionContent)
            });
        }
    }

    // Also extract h3 headings for more granular search
    const h3Regex = /<h3>([^<]+)<\/h3>/gi;
    let h3Match;

    while ((h3Match = h3Regex.exec(html)) !== null) {
        const headingText = h3Match[1].trim();
        const position = h3Match.index;

        // Find the parent h2 for context
        let parentH2 = null;
        for (const h2 of h2Positions) {
            if (h2.position < position) {
                parentH2 = h2.text;
            } else {
                break;
            }
        }

        // Extract a snippet of content after the h3
        const snippetEnd = Math.min(position + 500, html.length);
        const snippet = html.substring(position, snippetEnd);
        const contextText = extractTextContent(snippet).substring(0, 150).trim();

        const anchorId = headingText
            .toLowerCase()
            .replace(/[^a-z0-9\s]/g, '')
            .replace(/\s+/g, '-');

        entries.push({
            text: headingText,
            url: `${url}#${anchorId}`,
            category: category,
            date: date,
            context: contextText,
            parentArticle: articleTitle,
            parentSection: parentH2,
            keywords: extractCodeKeywords(snippet)
        });
    }

    return entries;
}

// Extract general keywords from HTML
function extractKeywords(html) {
    const keywords = new Set();

    // Extract from meta keywords
    const keywordsMatch = html.match(/<meta\s+name="keywords"\s+content="([^"]+)"/);
    if (keywordsMatch) {
        keywordsMatch[1].split(',').forEach(kw => {
            const trimmed = kw.trim().toLowerCase();
            if (trimmed.length > 2) {
                keywords.add(trimmed);
            }
        });
    }

    // Extract code keywords
    extractCodeKeywords(html).forEach(kw => keywords.add(kw));

    return Array.from(keywords);
}

// Extract keywords from code blocks and strong tags
function extractCodeKeywords(html) {
    const keywords = new Set();

    // Extract from <code> tags
    const codeRegex = /<code[^>]*>([^<]+)<\/code>/gi;
    let match;
    while ((match = codeRegex.exec(html)) !== null) {
        const code = match[1].trim().toLowerCase();
        // Only add short, meaningful keywords
        if (code.length > 1 && code.length < 30 && !code.includes('\n')) {
            keywords.add(code);
        }
    }

    // Extract from <strong> tags (single words only)
    const strongRegex = /<strong>([^<]+)<\/strong>/gi;
    while ((match = strongRegex.exec(html)) !== null) {
        const text = match[1].trim().toLowerCase();
        if (text.length > 2 && text.length < 25 && !text.includes(' ')) {
            keywords.add(text);
        }
    }

    return Array.from(keywords);
}

// Strip HTML tags
function stripTags(html) {
    return html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
}

// Extract text content from HTML
function extractTextContent(html) {
    // Remove script and style tags
    let text = html.replace(/<script[^>]*>[\s\S]*?<\/script>/gi, '');
    text = text.replace(/<style[^>]*>[\s\S]*?<\/style>/gi, '');
    text = text.replace(/<pre[^>]*>[\s\S]*?<\/pre>/gi, ' [code] ');

    // Convert list items to text
    text = text.replace(/<li[^>]*>/gi, ' - ');

    // Add space for block elements
    text = text.replace(/<\/(p|div|li|h[1-6]|tr|td|th)>/gi, ' ');

    // Strip remaining tags
    text = stripTags(text);

    // Clean up whitespace and HTML entities
    text = text.replace(/&[a-z]+;/gi, ' ');
    text = text.replace(/\s+/g, ' ').trim();

    return text;
}

// Main
function main() {
    console.log('Building blog search index...');

    if (!fs.existsSync(BLOG_DIR)) {
        console.error(`Error: ${BLOG_DIR} not found`);
        process.exit(1);
    }

    // Get all HTML files in blog directory
    const files = fs.readdirSync(BLOG_DIR)
        .filter(f => f.endsWith('.html'))
        .map(f => path.join(BLOG_DIR, f));

    console.log(`Found ${files.length} blog files`);

    // Parse all articles
    const allEntries = [];
    for (const file of files) {
        const entries = parseArticle(file);
        allEntries.push(...entries);
    }

    // Deduplicate entries
    const seen = new Set();
    const cleanedEntries = allEntries.filter(entry => {
        const key = `${entry.url}:${entry.text}`;
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
