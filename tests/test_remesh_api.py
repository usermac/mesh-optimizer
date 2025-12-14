import os
import sys
import time

import requests

API_URL = "http://localhost:3000"
API_KEY = "sk_test_123"  # Default test key from the server code


def test_remesh(file_path):
    if not os.path.exists(file_path):
        print(f"Error: File '{file_path}' not found.")
        return

    print(f"Uploading {file_path} for remeshing...")

    # Prepare parameters
    files = {"file": open(file_path, "rb")}
    data = {
        "mode": "remesh",
        "faces": "2000",
        "texture_size": "1024",
        "format": "both",  # Request JSON response
    }
    headers = {"Authorization": f"Bearer {API_KEY}"}

    try:
        # 1. Submit Job
        start_time = time.time()
        res = requests.post(
            f"{API_URL}/optimize", files=files, data=data, headers=headers
        )

        if res.status_code != 200:
            print(f"Upload failed: {res.status_code}")
            print(res.text)
            return

        json_resp = res.json()
        job_id = json_resp.get("jobId")
        print(f"Job submitted! ID: {job_id}")

        # 2. Poll Status
        while True:
            status_res = requests.get(f"{API_URL}/job/{job_id}", headers=headers)
            if status_res.status_code != 200:
                print(f"Polling failed: {status_res.status_code}")
                break

            status_data = status_res.json()
            status = status_data.get("status")

            if isinstance(status, dict):
                # Completed or Failed usually come as objects/dicts in Rust enums serialized to JSON
                if "Completed" in status:
                    print("\nSUCCESS!")
                    print(f"GLB URL: {status['Completed']['glb_url']}")
                    print(f"USDZ URL: {status['Completed']['usdz_url']}")
                    print(f"Total Time: {time.time() - start_time:.2f}s")
                    break
                elif "Failed" in status:
                    print("\nFAILED!")
                    print(f"Error: {status['Failed']['error']}")
                    break
            elif status == "Processing" or status == "Queued":
                sys.stdout.write(".")
                sys.stdout.flush()
                time.sleep(1)
            else:
                # Fallback for unexpected status structure
                print(f"\nStatus: {status}")
                if (
                    status == "Completed"
                ):  # Should be caught by dict check above if strictly following Rust serialization
                    break
                time.sleep(1)

    except Exception as e:
        print(f"An error occurred: {e}")
    finally:
        files["file"].close()


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python test_remesh_api.py <path_to_3d_file>")
        print("Example: python test_remesh_api.py ./my_scan.obj")
    else:
        test_remesh(sys.argv[1])
