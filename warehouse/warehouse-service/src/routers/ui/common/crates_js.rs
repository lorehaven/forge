pub fn ensure_crates_js() {
    let js = crates_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/crates.js", js);
}

fn crates_js() -> String {
    r#"
// ---- yank ----
function handleYankClick(event) {{
    const button = event.currentTarget;
    const crateName = button.getAttribute('data-crate');
    const version = button.getAttribute('data-version');

    if (!crateName || !version) {{
        console.error('Missing crate name or version');
        return;
    }}

    fetch(`/api/v1/crates/${{crateName}}/${{version}}/yank`, {{
        method: 'DELETE',
        headers: {{
            'Content-Type': 'application/json'
        }}
    }})
    .then(response => {{
        if (response.ok) {{
            // Reload the page to show updated status
            location.reload();
        }} else {{
            console.error('Failed to yank crate version');
        }}
    }})
    .catch(error => {{
        console.error('Error yanking crate version:', error);
    }});
}}

// ---- unyank ----
function handleUnyankClick(event) {{
    const button = event.currentTarget;
    const crateName = button.getAttribute('data-crate');
    const version = button.getAttribute('data-version');

    if (!crateName || !version) {{
        console.error('Missing crate name or version');
        return;
    }}

    fetch(`/api/v1/crates/${{crateName}}/${{version}}/unyank`, {{
        method: 'PUT',
        headers: {{
            'Content-Type': 'application/json'
        }}
    }})
    .then(response => {{
        if (response.ok) {{
            // Reload the page to show updated status
            location.reload();
        }} else {{
            console.error('Failed to unyank crate version');
        }}
    }})
    .catch(error => {{
        console.error('Error unyanking crate version:', error);
    }});
}}
    "#
    .to_string()
}
