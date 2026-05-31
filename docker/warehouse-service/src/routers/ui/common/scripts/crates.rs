use quench_srv::prelude::with_base_path;
use quench_web::prelude::{Script, js};

pub fn crates_script() -> Script {
    let yank_base = with_base_path("/api/v1/crates");
    let js_code = r#"
// ---- yank ----
function handleYankClick(event) {
    const button = event.currentTarget;
    const crateName = button.getAttribute('data-crate');
    const version = button.getAttribute('data-version');

    if (!crateName || !version) {
        console.error('Missing crate name or version');
        return;
    }

    fetch(`__CRATES_API_BASE__/${crateName}/${version}/yank`, {
        method: 'DELETE',
        headers: {
            'Content-Type': 'application/json'
        }
    })
    .then(response => {
        if (response.ok) {
            location.reload();
        } else {
            console.error('Failed to yank crate version');
        }
    })
    .catch(error => {
        console.error('Error yanking crate version:', error);
    });
}

// ---- unyank ----
function handleUnyankClick(event) {
    const button = event.currentTarget;
    const crateName = button.getAttribute('data-crate');
    const version = button.getAttribute('data-version');

    if (!crateName || !version) {
        console.error('Missing crate name or version');
        return;
    }

    fetch(`__CRATES_API_BASE__/${crateName}/${version}/unyank`, {
        method: 'PUT',
        headers: {
            'Content-Type': 'application/json'
        }
    })
    .then(response => {
        if (response.ok) {
            location.reload();
        } else {
            console.error('Failed to unyank crate version');
        }
    })
    .catch(error => {
        console.error('Error unyanking crate version:', error);
    });
}
    "#
    .replace("__CRATES_API_BASE__", &yank_base);

    js!(js_code)
}
