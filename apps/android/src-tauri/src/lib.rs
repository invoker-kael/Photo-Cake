use photo_core::Batch;

#[tauri::command]
fn batch_contract_demo() -> Batch {
    Batch::new(
        "Portrait Session",
        vec![
            "content://photo-cake/demo/1".to_string(),
            "content://photo-cake/demo/2".to_string(),
            "content://photo-cake/demo/3".to_string(),
        ],
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![batch_contract_demo])
        .run(tauri::generate_context!())
        .expect("error while running Photo-Cake Android");
}
