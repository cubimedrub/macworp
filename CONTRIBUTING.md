# How to contribute

Contributions are always welcome!

## Bugs
1. Make sure it is a bug in MAcWorP an not the underlaying systems, like the database, workflow engine or the used workflow. If you are unsure proceed but mention it.
2. Check there is already an issue for the bug you found, if not create one
3. Please provide a descent report of how to replicate the bug.
    * Description of the bug.
    * What did you expect to actually happen.
    * MAcWorP version
    * What did you do when you encounter the bug?
    * ...
   
    Even if the bug is already reported, you might encounter it a different way or in another version.
4. You want to provide a bugfix? Great! Please do so on a separate fork and branch (checked out from the `dev`-branch) and create a PR to the dev-branch.


## New features
1. Before start hacking together a new feature, please create an issue and mark it as `use-case` or `feature-request` to discuss your idea with the main developers. 
    As we are happy about every contributions each feature need to be maintained in the feature and the risk is high it might get abondened and tansformed into a technical dept.
    Therefore we want to make sure every addition is in line with the idea of MAcWorP, maintainable and not bloating it for some very specific use cases.
2. Check if your idea does not violate the license.
2. Like bugfixes please develop the feature on a separate fork and branch and merge it via a PR.


## Continuous integration
`.github/workflows/ci.yml` runs on every pull request and on pushes to `dev`:
- `lint` (Linux): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo check -p macworp_web --target wasm32-unknown-unknown`, `cargo check -p macworp_backend` (standalone build).
- `test` (Linux, macOS, Windows): `cargo test --workspace`.
- `container-smoke` (Linux, Docker and Podman): `macworp_worker doctor` and the `Container` end-to-end test; the Nextflow end-to-end test is allowed to fail for now.
- `desktop-build` (all three systems, not on pull requests): `cargo build -p macworp_desktop --release`.

Run the `lint` and `test` commands locally before opening a PR. macOS and Windows runners have no container runtime, so runs there stay manual (`docs/manual-testing.md`).

Thanks and happy coding!

Your MAcWorP Team
