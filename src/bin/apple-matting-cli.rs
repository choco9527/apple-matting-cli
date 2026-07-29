#[tokio::main]
async fn main() {
    let exit_code = apple_matting_lib::cli::run(std::env::args().collect()).await;
    std::process::exit(exit_code);
}
