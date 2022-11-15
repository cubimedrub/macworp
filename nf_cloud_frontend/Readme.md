# Nextflow cloud frontend
The frontend is written with NuxtJS

## Dependencies
* [Conda](https://docs.conda.io/en/latest/)

## Configuration
The frontend is configured by 3 environment variables:
| variable | default | description |
| --- | --- | --- |
| NF_CLOUD_BACKEND_BASE_URL | `http://localhost:3000` | Base URL for the backend (no trailing slash, must be accessible for the internet browser) |
| NF_CLOUD_FRONTEND_INTERFACE | `127.0.0.1` | IP for the frontend |
| NF_CLOUD_FRONTEND_PORT | `5000` | Port of the frontend |
