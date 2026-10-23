fn main() -> std::process::ExitCode {
    storage_analyzer::app::main_with(env!("CARGO_BIN_NAME"))
}
