#[path = "../core/remote_connection_error_codes.rs"]
mod remote_connection_error_codes;

fn main() {
    match remote_connection_error_codes::export_remote_connection_error_code_catalog_doc() {
        Ok(path) => {
            println!(
                "exported remote connection error codes to {}",
                path.display()
            );
        }
        Err(err) => {
            eprintln!("failed to export remote connection error codes: {err}");
            std::process::exit(1);
        }
    }
}
