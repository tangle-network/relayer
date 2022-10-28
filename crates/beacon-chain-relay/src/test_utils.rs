use std::path::Path;

pub fn read_json_file_from_data_dir(file_name: &str) -> std::string::String {
    // Get git root directory
    let git_root_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let json_file_path = Path::new(&git_root_dir).join("data").join(file_name);
    println!("Reading file: {json_file_path:?}");
    std::fs::read_to_string(json_file_path).expect("Unable to read file")
}
