use crate::routers::with_base_path;

pub fn ensure_files_js() {
    let js = files_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/files.js", js);
}

fn files_js() -> String {
    let files_api_base = with_base_path("/api/v1/files");
    r#"
function currentDir() {
    const el = document.getElementById('current-path');
    return el ? (el.value || '') : '';
}

function handleRowSelect(event, url) {
    if (!url) return;
    if (event && event.target && event.target.closest('a, input, button, i, label')) {
        return;
    }
    window.location.assign(url);
}

async function uploadFiles(storageName) {
    const input = document.getElementById('upload-input');
    if (!input || !input.files || input.files.length === 0) {
        return;
    }

    const basePath = currentDir();
    for (const file of input.files) {
        const target = basePath ? `${basePath}/${file.name}` : file.name;
        const endpoint = `__FILES_API_BASE__/${storageName}/file?path=${encodeURIComponent(target)}`;
        const response = await fetch(endpoint, {
            method: 'PUT',
            body: file
        });
        if (!response.ok) {
            console.error(`upload failed for ${file.name}`);
            return;
        }
    }
    location.reload();
}

function selectedPaths() {
    const values = [];
    document.querySelectorAll('.bulk-path:checked').forEach((el) => {
        const value = el.getAttribute('data-path');
        if (value) values.push(value);
    });
    return values;
}

async function bulkDelete(storageName) {
    const paths = selectedPaths();
    if (paths.length === 0) return;

    const response = await fetch(`__FILES_API_BASE__/${storageName}/bulk`, {
        method: 'DELETE',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ paths })
    });

    if (response.ok) location.reload();
}

async function createFolder(storageName, currentPath) {
    const name = window.prompt('Folder name');
    if (!name) return;

    const path = currentPath ? `${currentPath}/${name}` : name;
    const response = await fetch(`__FILES_API_BASE__/${storageName}/folder?path=${encodeURIComponent(path)}`, {
        method: 'POST'
    });
    if (response.ok) location.reload();
}

async function deletePath(storageName, path, isDir) {
    const endpoint = isDir ? 'folder' : 'file';
    const response = await fetch(`__FILES_API_BASE__/${storageName}/${endpoint}?path=${encodeURIComponent(path)}`, {
        method: 'DELETE'
    });
    if (response.ok) location.reload();
}

function previewPath(storageName, path) {
    const url = `__FILES_API_BASE__/${storageName}/preview?path=${encodeURIComponent(path)}`;
    window.open(url, '_blank');
}

function downloadPath(storageName, path) {
    const url = `__FILES_API_BASE__/${storageName}/download?path=${encodeURIComponent(path)}`;
    window.location.assign(url);
}

async function bulkDownload(storageName) {
    const paths = selectedPaths();
    if (paths.length === 0) return;

    const response = await fetch(`__FILES_API_BASE__/${storageName}/bulk-download`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ paths })
    });

    if (!response.ok) return;

    const blob = await response.blob();
    const url = window.URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${storageName}-bulk.zip`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    window.URL.revokeObjectURL(url);
}
"#
    .replace("__FILES_API_BASE__", &files_api_base)
}
