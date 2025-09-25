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

## [Installation](https://cubimedrub.github.io/macworp/latest/installation/)
* Want to test MAcWorP and get an idea what it is doing? Try the [quickstart guide](https://cubimedrub.github.io/macworp/latest/installation/#quickstart). 
    You just need Docker and `Make`. Will work on Linux, MacOS (Docker Desktop) and Windows with (WSL). 
    This is not ment to be a fully reliable, long-term installation as everything is setup on one machine which ment a workflow run can eat up all ressources for the web components etc.
* You like it? Ask your local admin to set it up on your hardware using the [full install guide](https://cubimedrub.github.io/macworp/latest/installation/#recommended-full-install)

## Contribution
If you want to contribute to MAcWorP please check our [contribution document](CONTRIBUTING.md) first.

### Development
Follow the [development setup](https://cubimedrub.github.io/macworp/latest/development/) to start hacking.

### Version 2.0
We aim to replace the frontend written in VueJS 2, which hit EOL already, with NiceGUI. 
This will reduce some technical dept and provide a nearly complete Python environment.

The new version will als incorporate some significant changes to the backend:
* Replacement of Flask with FastAPI
* Introduction of access control to projects (including tests to make sure access is only granted to the right persons)
* API key for easy access using your favorit http-client/library.

A very early prototype is located in the [`dev-v2`-branch](https://github.com/cubimedrub/macworp/tree/dev-v2).

## For workflow developers
Follow the [workflow intregration guide](https://cubimedrub.github.io/macworp/latest/workflow_integration/). 

General recommendations
* Use public Docker container to manage process dependencies
* Do not use any experimental features in your workflows

We provided a simple genomics example workflow in [Nextflow](https://github.com/cubimedrub/macworp-genomics-demo-workflow-nextflow) and [Snakemake](https://github.com/cubimedrub/macworp-genomics-demo-workflow-snakemake) to show how it is done.
A well tested real work example is [McQuaC](https://github.com/mpc-bioinformatics/McQuaC), quality control workflow for mass spectrometry proteomic data.

