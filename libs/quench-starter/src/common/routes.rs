use std::sync::LazyLock;

static BASE_PATH: LazyLock<String> =
    LazyLock::new(|| normalize_base_path(&envmnt::get_or("BASE_PATH", "/")));

pub fn normalize_base_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return "/".to_string();
    }

    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.is_empty() {
        "/".to_string()
    } else if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

pub fn with_base_path(path: &str) -> String {
    let base = BASE_PATH.as_str();
    if base == "/" {
        return path.to_string();
    }

    match path {
        "" => BASE_PATH.clone(),
        "/" => format!("{}/", base),
        _ => format!("{}{}", base, path),
    }
}
