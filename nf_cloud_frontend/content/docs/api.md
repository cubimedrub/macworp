---
title: API
description: Learn how the API works
---
# Default values
## Request
* method: `GET`
* content type: `application/json`

## Response
* content type: `application/json`



# Workflows
## List workflows
* url: `/api/workflows"
### Output
```json
{
    "workflows": [
        {
            "id": <int>,
            "name": <string>,
            "nextflow_arguments": <dict>,
            "nextflow_workflow": <string>,
            "submitted_processes": <int>,
            "completed_processes": <int>,
            "is_scheduled": <boolean>
        },
        ...
    ]
}
```
## Get a specific workflow
* url: `/api/workflows/<int:id>"
### Output
```json
{
    "workflow": {
        "id": <int>,
        "name": <string>,
        "nextflow_arguments": <dict>,
        "nextflow_workflow": <string>,
        "submitted_processes": <int>,
        "completed_processes": <int>,
        "is_scheduled": <boolean>
    }
}
```

## Create a workflow
* url: `/api/workflows/create", 
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
    "workflow": {
        "id": <int>,
        "name": <string>,
        "nextflow_arguments": <dict>,
        "nextflow_workflow": <string>,
        "submitted_processes": <int>,
        "completed_processes": <int>,
        "is_scheduled": <boolean>
    }
}
```

## Update a workflow
* url: `/api/workflows/<int:id>/update", 
* methods: `POST`
### Request body
```json
{
    "nextflow_workflow": "<string>",
    "nextflow_argumenty: <dict>
}
```
### Output
```json
{}
```

## Delete a workflow
* url: `/api/workflows/<int:id>/delete`
* methods: `POST`
### Output
```json
{}
```

## Get workflow count
* url: `/api/workflows/count"
### Output
```json
{
    "count": <int>
}
```

## List workflow files
* url: `/api/workflows/<int:id>/files"
### Output
```json
{
    "folders": <string array>,
    "files": <string array>
}
```

## Upload a new file
* url: `/api/workflows/<int:id>/upload-file", 
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

## Delete file/folder from workflow
* url: `/api/workflows/<int:id>/delete-path", 
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

## Create a new folder within a workflow
* url: `/api/workflows/<int:id>/create-folder", 
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

## Schedule a workflow for execution
* url: `/api/workflows/<int:id>/schedule", 
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
* url: `/api/workflows/<int:id>/finished", 
* methods: `POST`
### Output
```
""
```

## Nextflow weblog endpoint
Should only by nextflow for reporting traces.
* url: `/api/workflows/<int:id>/nextflow-log", 
* methods: `POST`
## Request body
See: https://www.nextflow.io/docs/latest/tracing.html#weblog-via-http
## Output
```
""
```

# Nextflow workflows
## List available nextflow workflows
* url: `/api/nextflow-workflows`
### Output
```json
{
    "nextflow_workflows": [
        <string>,
        <string>
    ]
}
```

## Get dynamic arguments for a nextflow workflow
Returns the dynamic arguments as defined in the configuration.
* url: `/api/nextflow-workflows/<string:nextflow_workflow>/arguments`
### Output
```json
{
    "arguments": <dict>
}
```
