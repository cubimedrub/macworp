# Frontend
MAcWorP's frontend component provides the graphical user interface for the backend. 

## Dependencies
* [Conda](https://docs.conda.io/en/latest/)

## Install
1. Clone the repository
2. `micromamba env create -f environment.yaml`
3. `micromamba activte macworp`
4. `cd frontend`
5. `yarn install`
6. `yarn dev`

## Configuration
The frontend is configured using environment variables:

| variable | default | description |
| --- | --- | --- |
| MACWORP_FRONTEND_INTERFACE | `127.0.0.1` | IP for the frontend |
| MACWORP_FRONTEND_PORT | `5001` | Port of the frontend |
| MACWORP_BACKEND_BASE_URL | `http://localhost:3000` | Base URL for the backend (no trailing slash, must be accessible for the internet browser) |
| MACWORP_BACKEND_WS_URL | `http://localhost:3000` | URL for where the web socket endpoint is located, should be the same as the `MACWORP_BACKEND_BASE_URL` with the protocol `ws://`  |
| MACWORP_UPLOAD_MAX_FILE_SITE | 5368709120 | Maximum file size which can be uploaded in bytes |
| MACWORP_RENDER_MAX_FILE_SIZE | 1048576 | Maximum file size to render |
