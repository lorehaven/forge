use quench_web::prelude::{Script, js};

pub fn docker_script() -> Script {
    let service = envmnt::get_or("REGISTRY_SERVICE", "warehouse");
    let js_code = format!(
        r#"
document.addEventListener('DOMContentLoaded', () => {{
    restoreTreeState();
    setupTreeStateTracking();
}});

function setupTreeStateTracking() {{
    const tree = document.querySelector('.repo-tree');
    if (!tree) return;

    tree.addEventListener('toggle', (event) => {{
        if (event.target.tagName !== 'DETAILS') return;
        
        const path = event.target.getAttribute('data-path');
        if (!path) return;

        let openPaths = JSON.parse(sessionStorage.getItem('dockerTreeOpenPaths') || '[]');
        if (event.target.open) {{
            if (!openPaths.includes(path)) openPaths.push(path);
        }} else {{
            openPaths = openPaths.filter(p => p !== path);
        }}
        sessionStorage.setItem('dockerTreeOpenPaths', JSON.stringify(openPaths));
    }}, true);
}}

function restoreTreeState() {{
    const openPaths = JSON.parse(sessionStorage.getItem('dockerTreeOpenPaths') || '[]');
    openPaths.forEach(path => {{
        const details = document.querySelector(`details[data-path="${{path}}"]`);
        if (details) {{
            details.open = true;
        }}
    }});
}}

async function handleDeleteImageClick(event) {{
    const button = event.currentTarget;
    const repository = button.getAttribute('data-repository');
    const digest = button.getAttribute('data-digest');

    if (!repository || !digest) {{
        console.error('Missing repository name or digest');
        return;
    }}

    try {{
        const tokenResponse = await fetch(
            `/token?service={service}&scope=repository:${{repository}}:push`,
            {{
                credentials: 'include'
            }}
        );

        if (!tokenResponse.ok) {{
            console.error('Failed to obtain token');
            return;
        }}

        const tokenData = await tokenResponse.json();
        const token = tokenData.token;

        if (!token) {{
            console.error('Token missing in response');
            return;
        }}

        const deleteResponse = await fetch(
            `/v2/${{repository}}/manifests/${{digest}}`,
            {{
                method: 'DELETE',
                headers: {{
                    'Authorization': `Bearer ${{token}}`
                }}
            }}
        );

        if (deleteResponse.ok) {{
            location.reload();
        }} else {{
            console.error('Failed to delete docker image');
        }}

    }} catch (error) {{
        console.error('Error deleting docker image:', error);
    }}
}}
"#
    );

    js!(js_code)
}
