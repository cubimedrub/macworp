# MAcWorP backend
 
MAcWorP's central componentent to manage workflow execution on any kind of data.

* `WorkflowIntegration`
  * What kind of workflow engine to use?
  * Which inputs are fixes, which can be changed by the user?
  * How the user inputs should be rendered?
  * Where is the workflow located? Remote or local?
* `Project`
  * A collection of files / folder containing data to analyze
  * Store for cached data
    * Last workflow 
    * Cloned workflow sources
  * Workflows are strictly confined into a project directory when executed

## Features
* `standalone` - Compiles for standalone deplyoment on a remote server.
* `integration` - Integration into desktop application, switches to SQLite database.

## Developments

### Structure

```
project/
├─ src/
│  ├─ main.rs     # Main entrypoint the the standalone binary (feature: standalone)
│  ├─ lib.rs
│  ├─ ...
├─ db/            # Database related files
│  ├─ migrations/ # Database migrations
│  │  ├─ ...
├─ Cargo.toml
```

### Running the backend

```bash
cargo run -r -- serve
```
