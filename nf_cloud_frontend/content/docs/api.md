---
title: API
description: Learn how the API works
---
# Default values
## Request
* method: `GET`
* content type: `application/json`

### Endpoints require authentication
* headers
    * x-access-token: <JWT TOKEN>

## Response
* content type: `application/json`



# Projects
## List projects
* url: `/api/projects"
### Output
```json
{
    "projects": [
        {
            "id": <int>,
            "name": <string>,
            "workflow_arguments": <dict>,
            "nextflow_project": <string>,
            "submitted_processes": <int>,
            "completed_processes": <int>,
            "is_scheduled": <boolean>
        },
        ...
    ]
}
```
## Get a specific project
* url: `/api/projects/<int:id>"
### Output
```json
{
    "project": {
        "id": <int>,
        "name": <string>,
        "workflow_arguments": <dict>,
        "nextflow_project": <string>,
        "submitted_processes": <int>,
        "completed_processes": <int>,
        "is_scheduled": <boolean>
    }
}
```

## Create a project
* url: `/api/projects/create", 
* methods: `POST`
### Request body
```json
{
    "name": "<string>"
}
```
### Output
```json
{
    "project": {
        "id": <int>,
        "name": <string>,
        "workflow_arguments": <dict>,
        "nextflow_project": <string>,
        "submitted_processes": <int>,
        "completed_processes": <int>,
        "is_scheduled": <boolean>
    }
}
```

## Update a project
* url: `/api/projects/<int:id>/update", 
* methods: `POST`
### Request body
```json
{
    "nextflow_project": "<string>",
    "nextflow_argumenty: <dict>
}
```
### Output
```json
{}
```

## Delete a project
* url: `/api/projects/<int:id>/delete`
* methods: `POST`
### Output
```json
{}
```

## Get project count
* url: `/api/projects/count"
### Output
```json
{
    "count": <int>
}
```

## List project files
* url: `/api/projects/<int:id>/files"
### Output
```json
{
    "folders": <string array>,
    "files": <string array>
}
```

## Upload a new file
* url: `/api/projects/<int:id>/upload-file", 
* methods: `POST`
### Request body (`multipart/form-data`)
* file: `<binary file>`
* directory: `<string>`
### Output
```json
{
    "directory": <string>,
    "file": <string>
}
```

## Delete file/folder from project
* url: `/api/projects/<int:id>/delete-path", 
* methods: `POST`
### Request body
```json
{
    "path": <string>
}
```
### Output
```
""
```

## Create a new folder within a project
* url: `/api/projects/<int:id>/create-folder", 
* methods: `POST`
### Request body
```json
{
    "path": <string>
}
```
### Output
```
""
```

## Schedule a project for execution
* url: `/api/projects/<int:id>/schedule", 
* methods: `POST`
### Output
```json
{
    "is_scheduled": <boolean>
}
```

## Finalize execution
Should only by used by worker.    
Signals that the execution is finished.
* url: `/api/projects/<int:id>/finished", 
* methods: `POST`
### Output
```
""
```

## Nextflow weblog endpoint
Should only by nextflow for reporting traces.
* url: `/api/projects/<int:id>/nextflow-log", 
* methods: `POST`
## Request body
See: https://www.nextflow.io/docs/latest/tracing.html#weblog-via-http
## Output
```
""
```

# Nextflow projects
## List available nextflow projects
* url: `/api/nextflow-projects`
### Output
```json
{
    "nextflow_projects": [
        <string>,
        <string>
    ]
}
```

## Get dynamic arguments for a nextflow project
Returns the dynamic arguments as defined in the configuration.
* url: `/api/nextflow-projects/<string:nextflow_project>/arguments`
### Output
```json
{
    "arguments": <dict>
}
```

# Users

## get login provider
* url: `/api/users/login-providers`
### Output
```json
{
  "<provider_type>": {
    "<provider_name>": "<description>"
  }
}
```
Provider type is something like `openid`, `oauth`, ...
## Login per openid
* url: `/api/users/openid/<string:provider>/login`
### Output
Redirect (status code 302) to the OpenID provider

## Callback for OpenID provider
* url: `/api/users/openid/<string:provider>/callback`
### Output
Redirects (status code 302) to frontend. The `Location`-header contains a URL which got the JWT token as query parameter token.


## Logout the current user
* url: `/api/users/logout`

### Output
* Empty (status code 200)
* Redirects (status code 302) to frontend. The `Location`-header contains a URL which got the JWT token as query parameter token.
* Unauthorized (status code 401)

