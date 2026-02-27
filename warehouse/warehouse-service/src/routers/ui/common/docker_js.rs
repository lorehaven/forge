pub fn ensure_docker_js() {
    let js = docker_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/docker.js", js);
}

fn docker_js() -> String {
    let service = envmnt::get_or("REGISTRY_SERVICE", "warehouse");

    format!(
        r#"
async function handleDeleteImageClick(event) {{
    const button = event.currentTarget;
    const repository = button.getAttribute('data-repository');
    const digest = button.getAttribute('data-digest');

    if (!repository || !digest) {{
        console.error('Missing repository name or digest');
        return;
    }}

    try {{
        // 1. Request JWT using session cookie
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

        // 2. Call registry with Bearer token
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
    )
}
