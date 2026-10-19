#[tokio::main]
async fn main() -> std::process::ExitCode {
    binus_auth_spike::cli::run().await
}
