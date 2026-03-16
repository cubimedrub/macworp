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

## Installation

### Desktop

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

## For workflow developers
Follow the [workflow intregration guide](https://cubimedrub.github.io/macworp/latest/workflow_integration/). 

General recommendations
* Use public Docker container to manage process dependencies
* Do not use any experimental features in your workflows

We provided a simple genomics example workflow in [Nextflow](https://github.com/cubimedrub/macworp-genomics-demo-workflow-nextflow) and [Snakemake](https://github.com/cubimedrub/macworp-genomics-demo-workflow-snakemake) to show how it is done.
A well tested real work example is [McQuaC](https://github.com/mpc-bioinformatics/McQuaC), quality control workflow for mass spectrometry proteomic data.
