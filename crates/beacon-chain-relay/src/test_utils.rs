pub fn read_json_file_from_data_dir(file_name: &str) -> std::string::String {
    let mut json_file_path = std::env::current_exe().unwrap();

    json_file_path.pop();
    json_file_path.push("../../../beacon-chain-relay/data");
    json_file_path.push(file_name);
    println!("json_file_path: {:?}", json_file_path);
    std::fs::read_to_string(json_file_path).expect("Unable to read file")
}
