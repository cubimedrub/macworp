# Backend
MAcWorP's backend is the central component of the the ecosystem, handling authentication, data up and downloads as well as workflow integration and scheduling.


## Dependencies
Most dependencies will be installed via Conda, however to avoid additional Conda-channels some dependencies will be installed natively in addition to some basic compiler  which should be installable on most platforms:

* [Conda](https://docs.conda.io/en/latest/)

## Installation
1. Clone the repository
2. `micromamba env create -f environment.yaml`
3. `micromamba activte macworp`
4. `python -m macworp_backend ...`


## Configuration
Create a new config using `python -m macworp_backend utility config print > macworp.local.config.yaml` and place the file in the directory where you want to start the backend.   
On startup the backend tries to load a file called `macworp.local.config.yaml` from the current directory and will replace the default config with every setting from the loaded file.

