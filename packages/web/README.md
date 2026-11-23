# MAcWorP Web
Web frontend to run your favorite workflow on a remote platform. Needs MAcWorP's backend to be deployed separatly. 

## Development

Install [dioxus-cli](https://crates.io/crates/dioxus-cli)

### Structure

```
project/
├─ assets/ # Any assets specific for the web application
├─ src/
│  ├─ main.rs # The entrypoint for the app. It also defines the routes for the app.
│  ├─ components/ # Web specific reusable compontents
│  │  ├─ compontent1.rs 
│  ├─ pages/ # Web specific page for the web application which show a single ressource or multiple ressources of the same type (lists/indexes) 
│  │  ├─ page1.rs
├─ Cargo.toml
```

Shared code between the web and web application should be put into the ui_core-crate.

### Running the app

```bash
dx serve --web
```
