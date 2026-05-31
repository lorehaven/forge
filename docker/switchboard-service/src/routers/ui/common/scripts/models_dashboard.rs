use quench_srv::prelude::with_base_path;
use quench_web::prelude::{Script, js};

pub fn models_dashboard_script() -> Script {
    let models_api_base = with_base_path("/api/v1/models");

    let htmx = htmx_js(&models_api_base);
    let delete_modal = delete_modal_js(&models_api_base);
    let estimates_modal = estimates_modal_js();

    js!(format!(
        r#"
        {}
        {}
        {}
    "#,
        htmx, delete_modal, estimates_modal,
    ))
}

fn htmx_js(models_api_base: &str) -> String {
    format!(
        r##"
function model_tab_click(tab) {{
    const hfTab = document.getElementById('model-tab-hf');
    const ggufTab = document.getElementById('model-tab-gguf');

    if (tab === 'hf') {{
        hfTab.classList.add('active');
        ggufTab.classList.remove('active');
        document.querySelectorAll('.quant-hf').forEach(el => el.classList.remove('hidden'));
        document.querySelectorAll('.quant-gguf').forEach(el => el.classList.add('hidden'));
    }} else {{
        hfTab.classList.remove('active');
        ggufTab.classList.add('active');
        document.querySelectorAll('.quant-hf').forEach(el => el.classList.add('hidden'));
        document.querySelectorAll('.quant-gguf').forEach(el => el.classList.remove('hidden'));
    }}

    const quantEl = document.getElementById('quant');
    if (quantEl) quantEl.value = 'ALL';
    
    const sourceEl = document.getElementById('source');
    if (sourceEl) sourceEl.value = tab;
    
    htmx.trigger('#model-filters', 'change');
}}

document.addEventListener("DOMContentLoaded", () => {{
    htmx.ajax(
        "GET",
        "{models_api_base}/grid",
        {{
            target: "#models-grid",
            swap: "outerHTML",
            values: htmx.values(htmx.find('#model-filters'))
        }}
    );
}});

document.addEventListener("gpu-status", () => {{
    htmx.ajax(
        "GET",
        "{models_api_base}/grid",
        {{
            target: "#models-grid",
            swap: "outerHTML",
            values: htmx.values(htmx.find('#model-filters'))
        }}
    );
}});
"##,
        models_api_base = models_api_base
    )
}

fn delete_modal_js(models_api_base: &str) -> String {
    format!(
        r##"
let confirmDeleteModal = null;
let modelToDelete = null;

function ensureConfirmDeleteModal() {{
    if (!confirmDeleteModal) {{
        confirmDeleteModal = document.getElementById("confirm-delete-modal");
    }}
}}

function openConfirmDeleteModal(modelArg) {{
    ensureConfirmDeleteModal();
    let model;
    try {{
        if (modelArg instanceof HTMLElement) {{
            model = JSON.parse(modelArg.dataset.model);
        }} else {{
            model = typeof modelArg === 'string' ? JSON.parse(modelArg) : modelArg;
        }}
    }} catch (e) {{
        console.error("Failed to parse model data for delete modal", e, modelArg);
        return;
    }}
    modelToDelete = model;
    
    const nameEl = document.getElementById("model-to-delete-name");
    if (nameEl) nameEl.textContent = model.name;
    
    const title = document.getElementById("confirm-delete-modal-title");
    if (title) {{
        title.innerHTML = '';
        const mainTitle = document.createElement('span');
        mainTitle.dataset.i18n = "ui_models_modal_delete_title";
        mainTitle.textContent = "Confirm Delete";
        title.appendChild(mainTitle);
        if (window.qUpdateI18n) window.qUpdateI18n();
    }}
    
    confirmDeleteModal.classList.add("open");
}}

async function confirmDelete() {{
    if (!modelToDelete) return;

    const response = await fetch("{models_api_base}/delete", {{
        method: "POST",
        headers: {{ "Content-Type": "application/json" }},
        body: JSON.stringify({{ path: modelToDelete.path }}),
    }});

    if (response.ok) {{
        closeConfirmDeleteModal();
        htmx.trigger("#model-filters", "change");
    }} else {{
        const err = await response.text();
        alert("Failed to delete model: " + err);
    }}
}}

function closeConfirmDeleteModal() {{
    if (!confirmDeleteModal) return;
    confirmDeleteModal.classList.remove("open");
    modelToDelete = null;
}}
"##,
        models_api_base = models_api_base
    )
}

fn estimates_modal_js() -> String {
    r#"
let estimatesModal = null;

function ensureEstimatesModal() {
    if (!estimatesModal) {
        estimatesModal = document.getElementById("estimates-modal");
    }
}

function getAvailableVram() {
    const el = document.querySelector('#gpu-status .gpu-free span:last-child');
    if (!el) return 0;
    const val = el.textContent.replace(/[^\d.]/g, '');
    return parseFloat(val) || 0;
}

function openEstimatesModal(modelArg) {
    let model;
    try {
        if (modelArg instanceof HTMLElement) {
            model = JSON.parse(modelArg.dataset.model);
        } else {
            model = typeof modelArg === 'string' ? JSON.parse(modelArg) : modelArg;
        }
    } catch (e) {
        console.error("Failed to parse model data for estimates modal", e, modelArg);
        return;
    }

    ensureEstimatesModal();
    if (!estimatesModal) return;

    const availableVram = getAvailableVram();
    const fitFilter = document.getElementById("estimate-fit-filter");
    const contextFilter = document.getElementById("estimate-context-filter");
    const quantFilter = document.getElementById("estimate-quant-filter");
    if (!fitFilter || !contextFilter || !quantFilter) return;

    fitFilter.value = "all";
    replaceOptions(contextFilter, [
        createOption("all", "All Contexts", "ui_models_modal_estimates_filter_all_contexts"),
        ...buildContextOptions(model.estimates),
    ]);
    replaceOptions(quantFilter, [
        createOption("all", "All Quants", "ui_models_modal_estimates_filter_all_quants"),
        ...buildQuantOptions(model.estimates),
    ]);

    const title = document.getElementById("estimates-modal-title");
    if (title) {
        title.innerHTML = '';
        const mainTitle = document.createElement('span');
        mainTitle.dataset.i18n = "ui_models_modal_estimates_title";
        mainTitle.textContent = "Estimations"; 
        
        const nameTitle = document.createElement('span');
        nameTitle.textContent = " — " + (model.name || "Unknown");
        
        title.appendChild(mainTitle);
        title.appendChild(nameTitle);
    }

    bindEstimateFilters(model);
    renderEstimateGrid(model);
    
    // Apply translations to everything in the modal
    if (window.qUpdateI18n) window.qUpdateI18n();
    
    estimatesModal.classList.add("open");
}

function bindEstimateFilters(model) {
    [
        "estimate-fit-filter",
        "estimate-context-filter",
        "estimate-quant-filter",
    ].forEach(id => {
        const el = document.getElementById(id);
        if (!el) return;

        // Use cloned element to remove old listeners
        const newEl = el.cloneNode(true);
        el.parentNode.replaceChild(newEl, el);
        newEl.addEventListener("change", () => { renderEstimateGrid(model); });
    });
}

function renderEstimateGrid(model) {
    const grid = document.getElementById("estimate-grid");
    if (!grid) return;

    const availableVram = getAvailableVram();
    const fitFilter = document.getElementById("estimate-fit-filter")?.value || "all";
    const contextFilter = document.getElementById("estimate-context-filter")?.value || "all";
    const quantFilter = document.getElementById("estimate-quant-filter")?.value || "all";

    const estimates = model.estimates.filter(e => {
        const fits = e.total_gb <= availableVram;
        if (fitFilter === "fit" && !fits) return false;
        if (fitFilter === "nofit" && fits) return false;
        if (contextFilter !== "all" && String(e.context) !== contextFilter) return false;
        if (quantFilter !== "all" && e.quant !== quantFilter) return false;
        return true;
    });

    grid.replaceChildren(...estimates.map(e => renderEstimateRow(e, availableVram)));
    if (window.qUpdateI18n) window.qUpdateI18n();
}

function renderEstimateRow(estimate, availableVram) {
    const margin = availableVram - estimate.total_gb;
    const fits = estimate.total_gb <= availableVram;
    const tight = fits && margin <= 2.0;
    let cls = "fit-no";
    if (fits) cls = tight ? "fit-warn" : "fit-ok";

    const row = document.createElement("div");
    row.className = `fit-line ${cls}`;
    row.appendChild(renderEstimateField("ui_models_card_context", estimate.context));
    row.appendChild(renderEstimateField("ui_models_card_quant", estimate.quant));
    row.appendChild(renderEstimateField("ui_models_card_vram", `${estimate.total_gb} GB`));
    row.appendChild(renderEstimateField("ui_models_card_margin", `${margin.toFixed(2)} GB`));
    return row;
}

function renderEstimateField(key, value) {
    const wrapper = document.createElement("div");
    wrapper.appendChild(renderFitItemNode(key, value));
    return wrapper;
}

function renderFitItemNode(key, value) {
    if (value === undefined) {
        const span = document.createElement("span");
        span.dataset.i18n = key;
        return span;
    }

    const span = document.createElement("span");
    const strong = document.createElement("strong");
    strong.dataset.i18n = key;
    span.appendChild(strong);
    span.append(`: ${value}`);
    return span;
}

function buildContextOptions(estimates) {
    const values = [...new Set(estimates.map(e => e.context))];
    values.sort((a, b) => contextValue(b) - contextValue(a));
    return values.map(v => createOption(String(v), String(v)));
}

function buildQuantOptions(estimates) {
    const values = [...new Set(estimates.map(e => e.quant))];
    return values.map(v => createOption(v, v));
}

function replaceOptions(select, options) {
    select.replaceChildren(...options);
    select.value = "all";
}

function createOption(value, text, i18nKey) {
    const option = document.createElement("option");
    option.value = value;
    if (i18nKey) {
        option.dataset.i18n = i18nKey;
    }
    if (text) {
        option.textContent = text;
    }
    return option;
}

function contextValue(context) {
    if (typeof context === "number") return context;
    if (typeof context !== "string") return 0;
    return parseInt(context.replace("Size", ""), 10) || 0;
}

function closeEstimatesModal() {
    ensureEstimatesModal();
    if (!estimatesModal) return;
    estimatesModal.classList.remove("open");
}
"#
    .to_string()
}
