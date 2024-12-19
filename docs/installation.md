# Installation

## Quickstart
Interested and want to try MAcWorP as fast as possible?   

1. A Unix-like operating system is mandatory: You are good to go with any popular Linux distribution, Windows Subsystem for Linux or macOS
2. Install [Docker](https://docs.docker.com/get-started/get-docker/)
    * On Linux make sure your user is allowed to access the Docker service by adding the user to the Docker group `usermod -aG docker <your-user>` and reboot
3. Install `make` & `git`
4. Clone the repository
5. `make production-test-up`

This will start the web interface on `https://<local-computer-name>:16160`, the complete URL is written to the file `PRODUCTION_TEST_URL`. It might change depending on your location (home, office, ...) as it includes the computers FQDN.


## Recommended full install
Coming soon


