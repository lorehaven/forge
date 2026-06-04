use actix_web::{HttpResponse, Responder, get, http::header::ContentType, web};
pub use quench_srv::actix::routers::ui::{
    is_ui_authenticated, ui_asset_path, ui_login_redirect, ui_path,
};
use quench_srv::prelude::with_base_path;
use quench_web::prelude::*;
use std::sync::LazyLock;

mod css;

static UI_SHELL_HOME: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    let chat_api_path = with_base_path("/api/v1/chat");

    AppShellBuilder::new()
        .title("Sage")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(Some("ui_header_home"), true, true))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/sage.css"),
        )])
        .scripts(vec![Script::inline(format!(
            r#"
            document.addEventListener('input', function (e) {{
                if (e.target.classList.contains('chat-input')) {{
                    e.target.style.height = 'auto';
                    e.target.style.height = (e.target.scrollHeight) + 'px';
                }}
            }}, false);

            let messageCount = 1;

            window.copyCode = async function(btn) {{
                const code = btn.closest('.code-block').querySelector('code').textContent;
                try {{
                    await navigator.clipboard.writeText(code);
                    const original = btn.innerHTML;
                    btn.innerHTML = '<i class="fas fa-check"></i> Copied!';
                    btn.classList.add('success');
                    setTimeout(() => {{
                        btn.innerHTML = original;
                        btn.classList.remove('success');
                    }}, 2000);
                }} catch (err) {{
                    console.error('Failed to copy', err);
                    btn.textContent = 'Error';
                    setTimeout(() => btn.textContent = 'Copy', 2000);
                }}
            }};

            function formatMessage(text) {{
                const escape = (t) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
                
                function renderTable(rows) {{
                    if (rows.length < 2) return rows.join('\n');
                    let html = '<table><thead><tr>';
                    const headers = rows[0].split('|').filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
                    headers.forEach(h => html += `<th>${{formatInline(h.trim())}}</th>`);
                    html += '</tr></thead><tbody>';
                    
                    for (let i = 2; i < rows.length; i++) {{
                        html += '<tr>';
                        const cols = rows[i].split('|').filter((_, idx, arr) => idx > 0 && idx < arr.length - 1);
                        cols.forEach(c => html += `<td>${{formatInline(c.trim())}}</td>`);
                        html += '</tr>';
                    }}
                    html += '</tbody></table>';
                    return html;
                }}

                function formatInline(t) {{
                    return t.replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>');
                }}

                const parts = text.split('```');
                let html = '';
                for (let i = 0; i < parts.length; i++) {{
                    if (i % 2 === 0) {{
                        let textPart = escape(parts[i]);
                        const lines = textPart.split('\n');
                        let result = [];
                        let tableRows = [];
                        let inTable = false;

                        for (let j = 0; j < lines.length; j++) {{
                            const line = lines[j].trim();
                            if (line.startsWith('|') && line.endsWith('|')) {{
                                inTable = true;
                                tableRows.push(lines[j]);
                            }} else {{
                                if (inTable) {{
                                    result.push(renderTable(tableRows));
                                    tableRows = [];
                                    inTable = false;
                                }}
                                result.push(lines[j]);
                            }}
                        }}
                        if (inTable) result.push(renderTable(tableRows));
                        
                        html += result.map(l => l.startsWith('<table') ? l : formatInline(l)).join('\n');
                    }} else {{
                        const block = parts[i];
                        const lines = block.split('\n');
                        let lang = 'code';
                        let code = block;
                        
                        if (lines.length > 0 && lines[0].trim().length > 0 && !lines[0].includes(' ')) {{
                            lang = lines[0].trim();
                            code = lines.slice(1).join('\n');
                        }}

                        html += `<div class="code-block">
                                    <div class="code-header">
                                        <span class="code-lang">${{escape(lang)}}</span>
                                        <button class="copy-btn" onclick="copyCode(this)">
                                            <i class="far fa-copy"></i>
                                            <span>Copy</span>
                                        </button>
                                    </div>
                                    <pre><code>${{escape(code.trim())}}</code></pre>
                                 </div>`;
                    }}
                }}
                return html;
            }}

            function addNavDot(msgId, preview) {{
                const nav = document.querySelector('.chat-navigation');
                if (!nav) return;

                // Remove active class from others
                nav.querySelectorAll('.nav-dot').forEach(d => d.classList.remove('active'));

                const dot = document.createElement('div');
                dot.className = 'nav-dot active';
                dot.dataset.msgId = msgId;
                dot.onclick = () => document.getElementById(msgId).scrollIntoView({{behavior: 'smooth'}});
                
                const tooltip = document.createElement('div');
                tooltip.className = 'nav-tooltip';
                tooltip.textContent = preview;
                dot.appendChild(tooltip);

                nav.appendChild(dot);
            }}

            async function sendMessage() {{
                const input = document.getElementById('chat-input');
                const modelSelect = document.getElementById('model-select');
                const history = document.querySelector('.chat-history');
                const message = input.value.trim();
                const instanceId = modelSelect.value;

                if (!message || !instanceId) return;

                const preview = message.split(' ').slice(0, 5).join(' ') + (message.split(' ').length > 5 ? '...' : '');

                // Clear input
                input.value = '';
                input.style.height = 'auto';

                // Add user message
                const userMsgId = 'msg-' + (messageCount++);
                const userMsg = document.createElement('div');
                userMsg.className = 'chat-message message-user';
                userMsg.id = userMsgId;
                const userInner = document.createElement('div');
                userInner.className = 'message-inner';
                const userContent = document.createElement('div');
                userContent.className = 'message-content';
                userContent.innerHTML = formatMessage(message);
                userInner.appendChild(userContent);
                userMsg.appendChild(userInner);
                history.appendChild(userMsg);
                addNavDot(userMsgId, preview);
                history.scrollTop = history.scrollHeight;

                // Add AI message placeholder
                const aiMsgId = 'msg-' + (messageCount++);
                const aiMsg = document.createElement('div');
                aiMsg.className = 'chat-message message-ai';
                aiMsg.id = aiMsgId;
                const aiInner = document.createElement('div');
                aiInner.className = 'message-inner';
                const aiContent = document.createElement('div');
                aiContent.className = 'message-content';
                aiInner.appendChild(aiContent);
                aiMsg.appendChild(aiInner);
                history.appendChild(aiMsg);
                // AI preview will be updated once content starts streaming, but we add dot now
                addNavDot(aiMsgId, 'Sage is thinking...');

                try {{
                    const response = await fetch('{}', {{
                        method: 'POST',
                        headers: {{ 'Content-Type': 'application/json' }},
                        body: JSON.stringify({{ instance_id: instanceId, message }})
                    }});

                    const reader = response.body.getReader();
                    const decoder = new TextDecoder();
                    let fullContent = '';

                    while (true) {{
                        const {{ done, value }} = await reader.read();
                        if (done) break;

                        const chunk = decoder.decode(value);
                        const lines = chunk.split('\n');

                        for (const line of lines) {{
                            if (line.startsWith('data: ')) {{
                                try {{
                                    const data = JSON.parse(line.substring(6));
                                    if (data.content) {{
                                        fullContent += data.content;
                                        aiContent.innerHTML = formatMessage(fullContent);
                                        
                                        // Update AI tooltip
                                        const aiDotTooltip = document.querySelector(`.nav-dot[data-msg-id="${{aiMsgId}}"] .nav-tooltip`);
                                        if (aiDotTooltip && fullContent.length > 0) {{
                                            const aiPreview = fullContent.split(' ').slice(0, 5).join(' ') + (fullContent.split(' ').length > 5 ? '...' : '');
                                            aiDotTooltip.textContent = aiPreview;
                                        }}

                                        history.scrollTop = history.scrollHeight;
                                    }} else if (data.error) {{
                                        aiContent.innerHTML = 'Error: ' + data.error;
                                    }}
                                }} catch (e) {{
                                    console.error('Failed to parse SSE data', e);
                                }}
                            }}
                        }}
                    }}
                }} catch (e) {{
                    aiContent.innerHTML = 'Error: ' + e.message;
                }}
            }}

            document.addEventListener('click', e => {{
                if (e.target.closest('.chat-send-btn')) {{
                    sendMessage();
                }}
            }});

            document.addEventListener('keydown', e => {{
                if (e.target.id === 'chat-input' && e.key === 'Enter' && !e.shiftKey) {{
                    e.preventDefault();
                    sendMessage();
                }}
            }});

            const historyContainer = document.querySelector('.chat-history');
            if (historyContainer) {{
                historyContainer.addEventListener('scroll', () => {{
                    const messages = document.querySelectorAll('.chat-message');
                    const dots = document.querySelectorAll('.nav-dot');
                    let activeIndex = 0;
                    
                    const containerRect = historyContainer.getBoundingClientRect();
                    const threshold = containerRect.top + (containerRect.height / 2);

                    // If we're at the very bottom, highlight the last dot
                    if (Math.abs(historyContainer.scrollHeight - historyContainer.scrollTop - historyContainer.clientHeight) < 10) {{
                        activeIndex = messages.length - 1;
                    }} else {{
                        messages.forEach((msg, i) => {{
                            const rect = msg.getBoundingClientRect();
                            if (rect.top < threshold) {{
                                activeIndex = i;
                            }}
                        }});
                    }}

                    dots.forEach((dot, i) => {{
                        if (i === activeIndex) {{
                            dot.classList.add('active');
                        }} else {{
                            dot.classList.remove('active');
                        }}
                    }});
                }}, {{ passive: true }});
            }}
            "#,
            chat_api_path
        ))])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

static UI_SHELL_AUTH: LazyLock<AppShell> = LazyLock::new(|| {
    css::ensure_sage_css();

    AppShellBuilder::new()
        .title("Sage")
        .supported_locales(vec!["en-US".to_string()])
        .default_theme(Theme::DefaultDark)
        .supported_themes(vec![Theme::DefaultDark])
        .header(ui_header(None, false, false))
        .links(vec![Link::new(
            "stylesheet",
            &ui_asset_path("/css/sage.css"),
        )])
        .with_nav(false)
        .resources_prefix(ui_path(""))
        .build()
});

fn ui_header(title_key: Option<&str>, show_home: bool, show_logout: bool) -> Element {
    let title = match title_key {
        Some(key) => h2().attr("data-i18n", key),
        None => h2().attr("data-i18n", "header_label"),
    };

    header()
        .child(div().class("left-panel").child(title))
        .child(
            div()
                .class("right-panel")
                .child_opt(show_home.then(|| {
                    a().attr("href", ui_path("/home"))
                        .class("button")
                        .attr("data-i18n", "ui_home_button")
                }))
                .child_opt(show_logout.then(|| {
                    a().attr("href", ui_path("/logout"))
                        .class("button")
                        .attr("data-i18n", "ui_logout")
                })),
        )
}

#[get("/assets/{path:.*}")]
pub async fn assets(path: web::Path<String>) -> impl Responder {
    quench_srv::actix::routers::ui::serve_assets(path, "dist/assets").await
}

pub(super) fn render_page(
    mut builder: actix_web::HttpResponseBuilder,
    content: Element,
    page_kind: UiPageKind,
) -> HttpResponse {
    let shell = match page_kind {
        UiPageKind::Home => &*UI_SHELL_HOME,
        UiPageKind::Auth => &*UI_SHELL_AUTH,
    };
    builder
        .content_type(ContentType::html())
        .body(shell.page(div().class("page").child(content)))
}

pub(super) enum UiPageKind {
    Home,
    Auth,
}
