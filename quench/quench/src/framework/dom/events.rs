pub fn on_dom_ready(blocks: &[String]) -> String {
    let body = blocks.join("\n");
    format!("document.addEventListener(\"DOMContentLoaded\", () => {{\n{body}\n}});")
}
