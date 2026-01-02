"""
Test the /history API endpoint response format.

This test prevents regression of the bug where:
1. Frontend expected a direct array but API returned { history: [...] }
2. Timestamps were wrong because milliseconds were treated as seconds

Run against local server: python tests/test_history_api.py
Run against prod: python tests/test_history_api.py --prod
"""

import argparse
import sys
from datetime import datetime

import requests

LOCAL_URL = "http://localhost:3000"
PROD_URL = "https://webdeliveryengine.com"


def test_history_response_format(api_url: str, api_key: str) -> bool:
    """Test that /history returns the expected response format."""
    print(f"Testing /history endpoint at {api_url}...")

    headers = {"Authorization": f"Bearer {api_key}"}
    res = requests.get(f"{api_url}/history", headers=headers)

    if res.status_code != 200:
        print(f"FAIL: Expected 200, got {res.status_code}")
        print(f"Response: {res.text}")
        return False

    data = res.json()

    # Test 1: Response must have 'history' key (not be a direct array)
    if "history" not in data:
        print("FAIL: Response missing 'history' key")
        print(f"Got: {list(data.keys()) if isinstance(data, dict) else type(data)}")
        print("This would break the frontend which expects data.history")
        return False
    print("PASS: Response has 'history' key")

    history = data["history"]

    if not isinstance(history, list):
        print(f"FAIL: 'history' should be a list, got {type(history)}")
        return False
    print(f"PASS: 'history' is a list with {len(history)} entries")

    if len(history) == 0:
        print("WARN: No history entries to validate (empty history)")
        return True

    # Test 2: Each entry must have required fields
    required_fields = ["id", "timestamp", "type", "credits"]
    entry = history[0]

    for field in required_fields:
        if field not in entry:
            print(f"FAIL: Entry missing required field '{field}'")
            print(f"Entry has: {list(entry.keys())}")
            return False
    print(f"PASS: Entry has required fields: {required_fields}")

    # Test 3: Timestamp must be valid ISO 8601 and have reasonable year
    timestamp = entry["timestamp"]
    try:
        # Parse ISO 8601 timestamp
        dt = datetime.fromisoformat(timestamp.replace("+00:00", "+0000").replace("Z", "+0000"))

        # Year should be between 2020 and 2100 (catches ms-as-seconds bug which gives year 57976)
        if dt.year < 2020 or dt.year > 2100:
            print(f"FAIL: Timestamp year {dt.year} is unreasonable")
            print(f"Timestamp: {timestamp}")
            print("This suggests milliseconds are being treated as seconds")
            return False
        print(f"PASS: Timestamp '{timestamp}' has valid year {dt.year}")

    except ValueError as e:
        print(f"FAIL: Could not parse timestamp '{timestamp}': {e}")
        return False

    # Test 4: credits should be an integer
    if not isinstance(entry["credits"], int):
        print(f"FAIL: 'credits' should be int, got {type(entry['credits'])}")
        return False
    print("PASS: 'credits' is an integer")

    # Test 5: type should be a known value
    known_types = ["optimize", "refund", "credit", "transfer", "other"]
    if entry["type"] not in known_types:
        print(f"WARN: Unknown type '{entry['type']}' (expected one of {known_types})")
    else:
        print(f"PASS: 'type' is valid: '{entry['type']}'")

    print("\nAll tests passed!")
    return True


def main():
    parser = argparse.ArgumentParser(description="Test /history API endpoint")
    parser.add_argument("--prod", action="store_true", help="Test against production")
    parser.add_argument("--key", type=str, help="API key to use")
    args = parser.parse_args()

    api_url = PROD_URL if args.prod else LOCAL_URL
    api_key = args.key or "sk_test_123"  # Default test key

    if args.prod and not args.key:
        print("ERROR: --key required when testing against production")
        sys.exit(1)

    success = test_history_response_format(api_url, api_key)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
