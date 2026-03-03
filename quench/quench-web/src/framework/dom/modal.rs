pub fn toggle_modal(overlay_class: &str, panel_class: &str, show_class: &str) -> String {
    format!(
        "const overlay = document.getElementsByClassName('{overlay_class}');\
const panel = document.getElementsByClassName('{panel_class}');\
if (overlay.length === 1 && panel.length) {{\
overlay[0].classList.toggle('{show_class}');\
panel[0].classList.toggle('{show_class}');\
}}"
    )
}
