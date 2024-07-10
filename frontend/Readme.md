# Nextflow cloud frontend
The frontend is written with NuxtJS

## Dependencies
* [NodeJS](https://nodejs.org/en) >= 18.x
* [Yarn](https://yarnpkg.com/) >=4.x

## Configuration
The frontend is configured environment variables:
| variable | default | description |
| --- | --- | --- |
| NF_CLOUD_BACKEND_BASE_URL | `http://localhost:3000` | Base URL for the backend (no trailing slash, must be accessible for the internet browser) |
| NF_CLOUD_FRONTEND_INTERFACE | `127.0.0.1` | IP for the frontend |
| NF_CLOUD_FRONTEND_PORT | `5001` | Port of the frontend |
| NF_CLOUD_UPLOAD_MAX_FILE_SITE | 5368709120 | Maximum file size which can be uploaded in bytes | 
