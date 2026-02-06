#!/usr/bin/env python3
"""
Sketchfab Bulk Downloader
=========================
Downloads free, downloadable 3D models from Sketchfab for testing.

Setup:
1. Create a free Sketchfab account at https://sketchfab.com
2. Get your API token from: https://sketchfab.com/settings/password
3. Set environment variable: export SKETCHFAB_API_TOKEN="your_token_here"

Usage:
    python sketchfab_bulk_download.py --count 75 --output ./models
    python sketchfab_bulk_download.py --count 75 --output ./models --category architecture
    python sketchfab_bulk_download.py --count 75 --output ./models --search "photogrammetry scan"
"""

import os
import sys
import time
import json
import argparse
import requests
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor, as_completed

API_BASE = "https://api.sketchfab.com/v3"

# Categories that tend to have larger/more complex models
GOOD_CATEGORIES = [
    "architecture",
    "cultural-heritage-history",
    "nature-plants",
    "food-drink",
    "furniture-home",
    "science-technology",
    "vehicles-transportation",
]


def get_headers(token):
    return {"Authorization": f"Token {token}"}


def search_models(token, count=75, category=None, search_query=None):
    """Search for free, downloadable models."""
    headers = get_headers(token)
    models = []
    cursor = None

    print(f"Searching for {count} downloadable models...")

    while len(models) < count:
        params = {
            "downloadable": "true",
            "count": min(24, count - len(models)),  # API max is 24 per page
            "sort_by": "-likeCount",  # Popular models first
        }

        if cursor:
            params["cursor"] = cursor
        if category:
            params["categories"] = category
        if search_query:
            params["q"] = search_query

        response = requests.get(
            f"{API_BASE}/search",
            headers=headers,
            params=params,
            timeout=30
        )

        if response.status_code != 200:
            print(f"Error searching: {response.status_code} - {response.text}")
            break

        data = response.json()
        results = data.get("results", [])

        if not results:
            print("No more results found.")
            break

        for model in results:
            if len(models) >= count:
                break
            models.append({
                "uid": model["uid"],
                "name": model["name"],
                "url": model["viewerUrl"],
            })

        cursor = data.get("cursors", {}).get("next")
        if not cursor:
            break

        print(f"  Found {len(models)}/{count} models...")
        time.sleep(0.5)  # Be nice to the API

    return models


def get_download_url(token, model_uid):
    """Get the download URL for a specific model."""
    headers = get_headers(token)

    response = requests.get(
        f"{API_BASE}/models/{model_uid}/download",
        headers=headers,
        timeout=30
    )

    if response.status_code == 200:
        data = response.json()
        # Prefer glb, then gltf, then source
        for format_key in ["glb", "gltf", "source"]:
            if format_key in data:
                return data[format_key]["url"], format_key
    elif response.status_code == 401:
        return None, "unauthorized"
    elif response.status_code == 403:
        return None, "forbidden"
    elif response.status_code == 404:
        return None, "not_found"

    return None, f"error_{response.status_code}"


def download_file(url, output_path):
    """Download a file from URL."""
    response = requests.get(url, stream=True, timeout=120)
    response.raise_for_status()

    with open(output_path, "wb") as f:
        for chunk in response.iter_content(chunk_size=8192):
            f.write(chunk)

    return os.path.getsize(output_path)


def download_model(token, model, output_dir, index, total):
    """Download a single model."""
    uid = model["uid"]
    name = model["name"]
    safe_name = "".join(c if c.isalnum() or c in "._- " else "_" for c in name)[:50]

    try:
        url, format_type = get_download_url(token, uid)

        if not url:
            return {"status": "skip", "reason": format_type, "name": name}

        ext = "glb" if format_type == "glb" else "gltf" if format_type == "gltf" else "zip"
        filename = f"{index:03d}_{safe_name}.{ext}"
        output_path = output_dir / filename

        size = download_file(url, output_path)
        size_mb = size / (1024 * 1024)

        return {
            "status": "success",
            "name": name,
            "file": filename,
            "size_mb": round(size_mb, 2)
        }

    except Exception as e:
        return {"status": "error", "name": name, "error": str(e)}


def main():
    parser = argparse.ArgumentParser(description="Bulk download free 3D models from Sketchfab")
    parser.add_argument("--count", type=int, default=75, help="Number of models to download (default: 75)")
    parser.add_argument("--output", type=str, default="./sketchfab_models", help="Output directory")
    parser.add_argument("--category", type=str, help=f"Category filter. Options: {', '.join(GOOD_CATEGORIES)}")
    parser.add_argument("--search", type=str, help="Search query (e.g., 'photogrammetry scan')")
    parser.add_argument("--workers", type=int, default=3, help="Parallel download workers (default: 3)")
    args = parser.parse_args()

    # Get API token
    token = os.environ.get("SKETCHFAB_API_TOKEN")
    if not token:
        print("Error: SKETCHFAB_API_TOKEN environment variable not set.")
        print("\nTo get your token:")
        print("1. Log in to https://sketchfab.com")
        print("2. Go to https://sketchfab.com/settings/password")
        print("3. Copy your API Token")
        print("4. Run: export SKETCHFAB_API_TOKEN='your_token_here'")
        sys.exit(1)

    # Create output directory
    output_dir = Path(args.output)
    output_dir.mkdir(parents=True, exist_ok=True)

    # Search for models
    models = search_models(
        token,
        count=args.count,
        category=args.category,
        search_query=args.search
    )

    if not models:
        print("No downloadable models found. Try different search criteria.")
        sys.exit(1)

    print(f"\nFound {len(models)} models. Starting download...\n")

    # Download models with progress
    success_count = 0
    skip_count = 0
    error_count = 0
    total_size_mb = 0

    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {
            executor.submit(download_model, token, model, output_dir, i+1, len(models)): model
            for i, model in enumerate(models)
        }

        for future in as_completed(futures):
            result = future.result()

            if result["status"] == "success":
                success_count += 1
                total_size_mb += result["size_mb"]
                print(f"[{success_count}/{len(models)}] Downloaded: {result['file']} ({result['size_mb']:.1f} MB)")
            elif result["status"] == "skip":
                skip_count += 1
                print(f"[SKIP] {result['name']}: {result['reason']}")
            else:
                error_count += 1
                print(f"[ERROR] {result['name']}: {result['error']}")

    # Summary
    print(f"\n{'='*60}")
    print(f"Download Complete!")
    print(f"{'='*60}")
    print(f"  Successful: {success_count}")
    print(f"  Skipped:    {skip_count}")
    print(f"  Errors:     {error_count}")
    print(f"  Total Size: {total_size_mb:.1f} MB")
    print(f"  Location:   {output_dir.absolute()}")
    print(f"{'='*60}")

    # Save manifest
    manifest_path = output_dir / "manifest.json"
    with open(manifest_path, "w") as f:
        json.dump({
            "count": success_count,
            "total_size_mb": round(total_size_mb, 2),
            "models": models
        }, f, indent=2)
    print(f"\nManifest saved to: {manifest_path}")


if __name__ == "__main__":
    main()
