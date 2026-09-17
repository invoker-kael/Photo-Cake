use photo_core::Batch;

#[tauri::command]
fn batch_contract_demo() -> Batch {
    Batch::new(
        "Portrait Session",
        vec![
            "DSC_1042.ARW".to_string(),
            "DSC_1043.ARW".to_string(),
            "DSC_1044.ARW".to_string(),
        ],
    )
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![batch_contract_demo])
        .run(tauri::generate_context!())
        .expect("error while running Photo-Cake");
}
