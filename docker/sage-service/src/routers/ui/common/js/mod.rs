pub mod chat;

pub fn ensure_sage_js(chat_api_path: &str) {
    let js = chat::chat_js(chat_api_path);

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/sage.js", js);
}
