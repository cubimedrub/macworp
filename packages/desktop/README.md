# MAcWorP-Desktop
Cross platform desktop application to run complex workflows on your local computer.

## Development

Install [dioxus-cli](https://crates.io/crates/dioxus-cli)

### Structure

```
project/
├─ assets/ # Any assets specific for the desktop application
├─ src/
│  ├─ main.rs # The entrypoint for the app. It also defines the routes for the app.
│  ├─ components/ # Desktop specific reusable compontents
│  │  ├─ compontent1.rs 
│  ├─ pages/ # Desktop specific page for the desktop application which show a single ressource or multiple ressources of the same type (lists/indexes) 
│  │  ├─ page1.rs
├─ Cargo.toml
```

Shared code between the desktop and web application should be put into the ui_core-crate.

### Running the app

```bash
dx run --desktop`
```

For passing argument to the underlaying binary add `--args`, e.g for increasing thrlog level:

```bash
dx run --desktop --args=-vvvv`
```

or see the help:s

```bash
dx run --desktop --args=--help
```
