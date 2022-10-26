pub fn read_json_file_from_data_dir(file_name: &str) -> std::string::String {
    let mut json_file_path = std::env::current_exe().unwrap();

    while json_file_path.clone().into_os_string().into_string().unwrap().contains("target".to_string().as_str()) {
        json_file_path.pop();
    }

    json_file_path.push("./crates/beacon-chain-relay/data");
    json_file_path.push(file_name);
    std::fs::read_to_string(json_file_path).expect("Unable to read file")
}
