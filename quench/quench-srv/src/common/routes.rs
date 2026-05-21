use std::sync::LazyLock;

static BASE_PATH: LazyLock<String> =
    LazyLock::new(|| normalize_base_path(&envmnt::get_or("BASE_PATH", "/")));

pub fn normalize_base_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return String::new();
    }

    let without_trailing = trimmed.trim_end_matches('/');
    if without_trailing.is_empty() || without_trailing == "/" {
        String::new()
    } else if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

pub fn with_base_path(path: &str) -> String {
    if BASE_PATH.as_str() == "/" {
        return path.to_string();
    }

    match path {
        "" => BASE_PATH.clone(),
        "/" => format!("{}/", BASE_PATH.as_str()),
        _ => format!("{}{}", BASE_PATH.as_str(), path),
    }
}
