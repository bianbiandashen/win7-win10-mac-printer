use serde_json::Value;
use std::fs;

#[tauri::command(rename_all = "snake_case")]
pub fn get_version_from_config() -> Result<String, String> {
    let config_path = "src-tauri/tauri.conf.json";
    let config_content = 
        fs::read_to_string(config_path).map_err(|err| err.to_string())?;
    let config_json: Value =
        serde_json::from_str(&config_content).map_err(|err| err.to_string())?;
    
    config_json["package"]["version"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Version not found in config".to_string())
}
