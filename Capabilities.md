# Application purpose
The app, a 3D mesh optimizer, is developed on the macos using the Zed editor. It is then pushed to the prod server (see hardware) as a basically Rust application. It is to help online and api users optimize 3D assets for web and mobile platforms and other needs. 

The user uses the web ui to get their settings right and then should use the API to process their assets.

# Application Capabilities

This document outlines the current features and capabilities of the Mesh Optimizer application.

## 1. 3D Model Optimization & Conversion
The core function of the application is processing 3D assets to make them web-ready.
- **Input Formats:** Supports `.glb`, `.gltf`, `.obj`, `.fbx`, and `.zip` archives containing these files.
- **Optimization:**
  - **Decimation:** Simplifies mesh geometry to reduce file size.
  - **Remeshing:** Generates new, clean topology (QuadriFlow) for organic or scanned assets.
  - **Baking:** Transfers Normal and Diffuse details from high-poly source to low-poly result.
- **Output Formats:** Automatically generates both:
  - **GLB** (Binary glTF) - Standard for web 3D.
  - **USDZ** (Universal Scene Description) - Standard for Apple AR/Quick Look.
- **Integrity:** Calculates `SHA256` hashes of uploaded files to ensure data integrity.

## 2. Job Processing System
Handles long-running tasks asynchronously to ensure server responsiveness.
- **Status Tracking:** Jobs move through defined states: `Queued`, `Processing`, `Completed`, and `Failed`.
- **Concurrency Control:** Utilizes a semaphore to limit the number of concurrent worker processes, preventing server overload.
- **Polling:** Provides endpoints for clients to poll the status of specific optimization jobs.

## 3. Monetization & Access Control
Implements a credit-based system for API usage.
- **Authentication:** Middleware validates requests using API Keys.
- **Credit System:**
  - Users have a credit balance stored in the database.
  - Credits are deducted per successful optimization job.
  - **History:** Tracks usage history for users.
- **Stripe Integration:**
  - **Checkout Sessions:** Users can purchase credits via Stripe.
  - **Webhooks:** secure webhook handler updates user credit balances automatically upon successful payment (`checkout.session.completed`).

## 4. Data Persistence & Storage
- **Database:** Uses **SQLite** (via `sqlx`) for a lightweight, self-contained relational database handling:
  - User accounts and API Keys.
  - Credit balances.
  - Job history and transaction logs.
- **File Storage:** Stores uploaded and processed files locally in a structured `uploads/{batch_id}` directory.

## 5. Administration
Includes restricted endpoints for system management:
- **Key Generation:** Ability to manually create new API keys with initial credit balances.
- **Credit Management:** Ability to manually add credits to existing keys.

## 6. Technical Stack
- **Language:** Rust (2021 edition)
- **Web Framework:** Axum
- **Async Runtime:** Tokio
- **Database ORM/Query:** SQLx
- **Payments:** Async-Stripe

## 7. Capability Analysis vs Market Standards

Based on the feature set, the application fits into the following market categories:

*   **Simple Decimation (Polygon Reduction):** **Confirmed.**
    *   The application performs geometry simplification to reduce file size.
    *   *Market Note:* The current pricing of $2.00/credit is significantly higher than the expected market rate of $0.10 - $0.50 for this specific task.

*   **Complex Optimization (Remeshing + Baking):** **Confirmed.**
    *   The application now performs remeshing via Blender (QuadriFlow) and bakes high-fidelity Normal/Diffuse maps onto the optimized mesh.

*   **Photogrammetry (Images to Mesh):** **Negative.**
    *   The application does *not* support image-to-mesh conversion. It operates strictly on existing 3D model files (`obj`, `fbx`, `glb`, `gltf`).
