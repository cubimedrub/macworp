/// Worker configuration
mod configuration;
/// Workflow executor, for building and running the workflow using an external tool
mod executors;
/// Proxy for catching workflow engine logs and sending them to the backend for further processing
mod log_proxy;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
}
