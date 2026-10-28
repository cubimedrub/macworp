# MAcWorP - Massive aCcessible Workflow Platform

MaCWorP is a web application to give workflow developers a simple way to make their workflows easily accessible via a web browser for everyone. 

## Introduction
* What it **does** for you
    * Graphical user interface for
        * Data up- & download
        * Parameter input
        * Rendered results (interactive plots, PDFs, images, tables, ...)
    * Authentication
        * Simple file based authentication (easy to set up, recommended for smaller teams, labs or institutes)
        * OpenIDConnect (e.g. connect to ELIXIR's Life Science Login)
    * Distributed execution of a workflow on different workers
    * Coming soon(ish):
        * Support for multiple workflow engines (currently Nextflow is working)
        * Fine grained access control to projects and workflows including sharing with others.
* What it **does not** for you
    * Scaling - This is still the job of the workflow engine. E.g. you can configure Nextflow to use K8s or Slurm when started by MAcWorP workers but setting these executors up is still a separate job for an admin.

## Supported engines per container runtime
Engines run inside containers, so a container runtime (Podman or Docker) is required.

| Runtime | Nextflow | Snakemake | CWL | Container |
| --- | --- | --- | --- | --- |
| Docker | yes | yes | yes | yes |
| Podman | yes | no | yes | yes |
| WSL Containers (`wslc`, Windows) | no | no | no | yes |

Snakemake needs Docker. CWL support is still planned.

## Installation

### Desktop

Data is located in
| OS | Path |
| --- | --- |
| macOS | `~/Library/Application Support/de.ruhr-uni-bochum.CUBiMed.RUB.macworp_desktop` |
| Linux | `$XDG_DATA_HOME/macworp_desktop` (default `~/.local/share/macworp_desktop`) |
| Windows | `%APPDATA%\CUBiMed.RUB\macworp_desktop` (`C:\Users\<user>\AppData\Roaming\CUBiMed.RUB\macworp_desktop`) |

#### macOS
Comming soon

#### Linux
Comming soon

#### Windows
Comming soon

### Web
Comming soon

## Contribution
If you want to contribute to MAcWorP please check our [contribution document](CONTRIBUTING.md) first.

### Development
The ecosystem is build on top of [axum](https://crates.io/crates/axum) (backend) and [dioxus](https://crates.io/crates/dioxus) (web and desktop UI).
Code, shared between all components, can be found in the [core-crate](packages/core) shared UI code is maintained in the [ui_core-crate](packages/ui_core).  
MAcWorPs worker component lives in the [worker-crate](packages/worker) as a separate binary.

All styling is done via SASS which is transpiled and bundled via [manganis](https://crates.io/crates/manganis).


#### Requirements
* Rust: Follow [the install page](https://rust-lang.org/tools/install/) (use `rustup`)
  * x86_64 target: Depends on your operating system
  * WASM target: `rustup target add wasm32-unknown-unknown`
* Dioxus CLI: `curl -sSL https://dioxus.dev/install.sh | bash` or `cargo binstall dioxus-cli --force` or `cargo install dioxus-cli`
* On Linux or Mac, Procfile application: [Ultraman](https://github.com/yukihirop/ultraman) (Rust based) or use Procfile application like [foreman](https://github.com/ddollar/foreman) (Ruby basesd) or [honcho](https://github.com/nickstenning/honcho) (Python based)
* Container engine
  * Docker and docker-compose: Needed for PostgreSQL. Instruction depend on your operating system
  * Podman: Optional for testing and validation. Instruction depend on your operating system
  * WSL (Container): Optional for testing and validation. Instruction depend on your operating system
  
#### Running the desktop edition
`dx serve --package macworp_desktop --desktop --args='-vvv -a dev'`

To avoid collision between development and production `-a dev` adds a suffix `-dev` to the application directory, data is stored in:
| OS | Path |
| --- | --- |
| macOS | `~/Library/Application Support/de.ruhr-uni-bochum.CUBiMed.RUB.macworp_desktop-dev` |
| Linux | `$XDG_DATA_HOME/macworp_desktop-dev` (default `~/.local/share/macworp_desktop-dev`) |
| Windows | `%APPDATA%\CUBiMed.RUB\macworp_desktop-dev` (`C:\Users\<user>\AppData\Roaming\CUBiMed.RUB\macworp_desktop-dev`) |


#### Running the web edition

##### Linux / macOS
```
dev/setup.sh
ultraman start
```

##### Windows
```
# terminal 1
pwsh dev/setup.ps1
docker-compose up
```

```
# terminal 2
cargo run -p macworp_backend -- db-migrate dev/config.toml && cargo run -p macworp_backend -- serve dev/config.toml
```

```
# terminal 3
MACWORP_BASE_URL=http://127.0.0.1:3001 dx serve --package macworp_web --web --port 5001
```

##### Resetting all operating systems
Stop all the started processes from above and run `docker compose down --remove-orphans`


## For workflow developers
Follow the [workflow intregration guide](https://cubimedrub.github.io/macworp/latest/workflow_integration/). 

General recommendations
* Use public Docker container to manage process dependencies
* Do not use any experimental features in your workflows

We provided a simple genomics example workflow in [Nextflow](https://github.com/cubimedrub/macworp-genomics-demo-workflow-nextflow) and [Snakemake](https://github.com/cubimedrub/macworp-genomics-demo-workflow-snakemake) to show how it is done.
A well tested real work example is [McQuaC](https://github.com/mpc-bioinformatics/McQuaC), quality control workflow for mass spectrometry proteomic data.
