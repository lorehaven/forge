pub fn chat_js(_chat_api_path: &str) -> String {
    let mut js = String::new();
    js.push_str(&input_listener());
    js.push_str(&copy_code_fn());
    js.push_str(&click_listener());
    js.push_str(&sse_listener());
    js.push_str(&scroll_listener());
    js
}

fn input_listener() -> String {
    r#"
document.addEventListener('input', function (e) {
    if (e.target.classList.contains('chat-input')) {
        e.target.style.height = 'auto';
        e.target.style.height = (e.target.scrollHeight) + 'px';
    }
}, false);
"#
    .to_string()
}

fn copy_code_fn() -> String {
    r#"
async function copyCode(btn) {
    const code = btn.closest('.code-block').querySelector('code').textContent;
    try {
        await navigator.clipboard.writeText(code);
        const original = btn.innerHTML;
        btn.innerHTML = '<i class="fas fa-check"></i> Copied!';
        btn.classList.add('success');
        setTimeout(() => {
            btn.innerHTML = original;
            btn.classList.remove('success');
        }, 2000);
    } catch (err) {
        console.error('Failed to copy', err);
        btn.textContent = 'Error';
        setTimeout(() => btn.textContent = 'Copy', 2000);
    }
}
"#
    .to_string()
}

fn click_listener() -> String {
    r#"
document.addEventListener('click', e => {
    // Handle nav dots
    const navDot = e.target.closest('.nav-dot');
    if (navDot && navDot.dataset.msgId) {
        const target = document.getElementById(navDot.dataset.msgId);
        if (target) {
            target.scrollIntoView({behavior: 'smooth', block: 'start'});
        }
    }

    // Handle copy buttons
    const copyBtn = e.target.closest('.copy-btn');
    if (copyBtn) {
        copyCode(copyBtn);
    }
});
"#
    .to_string()
}

fn sse_listener() -> String {
    r#"
document.addEventListener('htmx:sseMessage', e => {
    const history = document.querySelector('.chat-history');
    if (history) {
        history.scrollTop = history.scrollHeight;
    }
});
"#
    .to_string()
}

fn scroll_listener() -> String {
    r#"
(function() {
    function updateActiveDot() {
        const historyContainer = document.querySelector('.chat-history');
        if (!historyContainer) return;

        const messages = document.querySelectorAll('.chat-message');
        const dots = document.querySelectorAll('.nav-dot');
        if (messages.length === 0 || dots.length === 0) return;

        let activeIndex = 0;
        const containerRect = historyContainer.getBoundingClientRect();
        const threshold = containerRect.top + (containerRect.height / 3);

        const atBottom = Math.abs(historyContainer.scrollHeight - historyContainer.scrollTop - historyContainer.clientHeight) < 50;
        
        if (atBottom) {
            activeIndex = messages.length - 1;
        } else {
            for (let i = 0; i < messages.length; i++) {
                const rect = messages[i].getBoundingClientRect();
                if (rect.top < threshold) {
                    activeIndex = i;
                } else {
                    break;
                }
            }
        }

        dots.forEach((dot, i) => {
            if (i === activeIndex) {
                dot.classList.add('active');
            } else {
                dot.classList.remove('active');
            }
        });
    }

    document.addEventListener('scroll', (e) => {
        if (e.target.classList && e.target.classList.contains('chat-history')) {
            updateActiveDot();
        }
    }, true);

    document.addEventListener('htmx:afterSwap', (e) => {
        updateActiveDot();
    });

    setTimeout(updateActiveDot, 100);
})();
"#
    .to_string()
}
