use quench_srv::prelude::with_base_path;

pub fn ensure_filters_js() {
    let js = models_dashboard_filters_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/models_dashboard_filters.js", js);
}

fn models_dashboard_filters_js() -> String {
    let models_api_base = with_base_path("/api/v1/models");
    let ui_base = with_base_path("/ui");

    let js = format!(
        r#"
function t(key) {{
    if (typeof TRANSLATIONS === 'undefined') return key;
    const locale = getLocale();
    const dict = TRANSLATIONS[locale];
    return dict ? (dict[key] || key) : key;
}}

let currentGpuInfo = null;
let currentModelSource = "hf";
let currentQuant = "ALL";
let currentContext = "0";
let currentVllmOnly = false;
let currentSearch = "";
let currentSort = "name_asc";
let estimatesModal = null;
let confirmDeleteModal = null;
let modelToDelete = null;
let isAdmin = false;

{dom_loaded}
{check_auth}
{apply_defaults}
{toggle_source}
{on_change_quant}
{on_change_context}
{on_change_vllm_only}
{on_change_sort}
{refresh_models}
{create_model_card}
{update_model_fits}
{update_card_fit}
{render_fit_summary}
{render_fit}
{render_no_fit}
{render_fit_item}
{render_separator}
{ensure_estimates_modal}
{open_estimates_modal}
{close_estimates_modal}
{render_estimate_row}
{quant_rank}
{context_value}
{find_best}
{find_min}

applySourceDefaults();
checkAuth().then(() => refreshModels());
    "#,
        dom_loaded = dom_loaded(),
        check_auth = check_auth(),
        apply_defaults = apply_source_defaults(),
        toggle_source = toggle_model_source(),
        on_change_quant = on_change_quant(),
        on_change_context = on_change_context(),
        on_change_vllm_only = on_change_vllm_only(),
        on_change_sort = on_change_sort(),
        refresh_models = refresh_models(),
        create_model_card = create_model_card(),
        update_model_fits = update_model_fits(),
        update_card_fit = update_card_fit(),
        render_fit_summary = render_fit_summary(),
        render_fit = render_fit(),
        render_no_fit = render_no_fit(),
        render_fit_item = render_fit_item(),
        render_separator = render_separator(),
        ensure_estimates_modal = ensure_estimates_modal(),
        open_estimates_modal = open_estimates_modal(),
        close_estimates_modal = close_estimates_modal(),
        render_estimate_row = render_estimate_row(),
        quant_rank = quant_rank(),
        context_value = context_value(),
        find_best = find_best_estimate(),
        find_min = find_minimum_estimate(),
    );

    js.replace("__MODELS_API_BASE__", &models_api_base)
        .replace("__UI_BASE__", &ui_base)
}

fn dom_loaded() -> String {
    r#"
window.addEventListener("DOMContentLoaded", () => {
    const quant = document.getElementById("quant");
    const context = document.getElementById("context");
    const vllmOnly = document.getElementById("vllm-only");
    const sort = document.getElementById("sort");
    if (quant) quant.value = currentQuant;
    if (context) context.value = String(currentContext);
    if (vllmOnly) vllmOnly.checked = currentVllmOnly;
    if (sort) sort.value = currentSort;

    const search = document.getElementById("search");
    if (search) {
        search.addEventListener("input", (e) => {
            currentSearch = e.target.value;
            refreshModels();
        });
    }

    window.addEventListener("gpu-update", (event) => {
        currentGpuInfo = event.detail;
        updateModelFits();
    });
});
"#
    .to_string()
}

fn check_auth() -> String {
    r#"
async function checkAuth() {
    try {
        const response = await fetch("__UI_BASE__/status");
        if (response.ok) {
            const status = await response.json();
            isAdmin = status.roles.includes("admin");
        }
    } catch (e) {
        console.error("Failed to check auth status", e);
    }
}
"#
    .to_string()
}

fn apply_source_defaults() -> String {
    r#"
function applySourceDefaults() {
    const quant = document.getElementById("quant");
    if (currentModelSource === "hf") {
        currentQuant = "ALL";
        document.querySelectorAll(".quant-hf").forEach(v => v.classList.remove("hidden"));
        document.querySelectorAll(".quant-gguf").forEach(v => v.classList.add("hidden"));
    } else {
        currentQuant = "ALL";
        document.querySelectorAll(".quant-hf").forEach(v => v.classList.add("hidden"));
        document.querySelectorAll(".quant-gguf").forEach(v => v.classList.remove("hidden"));
    }
    if (quant) quant.value = currentQuant;
}
"#
    .to_string()
}

fn toggle_model_source() -> String {
    r#"
function toggleModelSource(event) {
    const target = event.currentTarget;
    if (!target) return;
    const hf = document.getElementById("model-tab-hf");
    const gguf = document.getElementById("model-tab-gguf");
    if (!hf || !gguf) return;

    hf.classList.remove("active");
    gguf.classList.remove("active");
    target.classList.add("active");

    currentModelSource = target.id === "model-tab-hf" ? "hf" : "gguf";
    applySourceDefaults();
    refreshModels();
}
"#
    .to_string()
}

fn on_change_quant() -> String {
    r#"
function onChangeQuant() {
    currentQuant = event.currentTarget.value;
    refreshModels();
}
"#
    .to_string()
}

fn on_change_context() -> String {
    r#"
function onChangeContext() {
    currentContext = parseInt(event.currentTarget.value, 10);
    refreshModels();
}
"#
    .to_string()
}

fn on_change_vllm_only() -> String {
    r#"
function onChangeVllmOnly() {
    currentVllmOnly = event.currentTarget.checked;
    refreshModels();
}
"#
    .to_string()
}

fn on_change_sort() -> String {
    r#"
function onChangeSort() {
    currentSort = event.currentTarget.value;
    refreshModels();
}
"#
    .to_string()
}

fn refresh_models() -> String {
    r#"
async function refreshModels() {
    const response = await fetch("__MODELS_API_BASE__/list", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            type: currentModelSource.toUpperCase(),
            name: currentSearch,
            quant: currentQuant,
            context: String(currentContext),
        }),
    });
    const models = await response.json();
    
    let filteredModels = models;
    if (currentVllmOnly) {
        filteredModels = models.filter(m => m.vllm_supported);
    }
    
    const sortedModels = sortModels(filteredModels);

    const grid = document.getElementById("models-grid");
    if (!grid) return;

    grid.innerHTML = "";
    for (const model of sortedModels) {
        grid.appendChild(createModelCard(model));
    }
}

function sortModels(models) {
    return models.sort((a, b) => {
        const [field, direction] = currentSort.split("_");
        const isAsc = direction === "asc";
        
        let result = 0;
        if (field === "name") {
            result = a.name.localeCompare(b.name);
        } else if (field === "params") {
            result = (a.params_billion || 0) - (b.params_billion || 0);
        } else if (field === "vram") {
            const availableVram = currentGpuInfo?.free_gb || 0;
            const bestA = findBestEstimate(a.estimates, availableVram);
            const bestB = findBestEstimate(b.estimates, availableVram);
            const minA = findMinimumEstimate(a.estimates);
            const minB = findMinimumEstimate(b.estimates);
            
            const vramA = bestA ? bestA.total_gb : (minA ? minA.total_gb + 1000 : 2000);
            const vramB = bestB ? bestB.total_gb : (minB ? minB.total_gb + 1000 : 2000);
            result = vramA - vramB;
        }
        
        return isAsc ? result : -result;
    });
}
"#
    .to_string()
}

fn create_model_card() -> String {
    r#"
function createModelCard(model) {
    const card = document.createElement("div");
    card.className = "card";
    card.dataset.model = JSON.stringify(model);
    
    let deleteBtn = "";
    if (isAdmin) {
        deleteBtn = `<button class="card-delete" title="${t("ui_models_card_delete_tooltip")}"><i class="fa-solid fa-trash"></i></button>`;
    }

    let vllmBadge = "";
    if (model.vllm_supported) {
        vllmBadge = `<span class="vllm-badge" title="Supported by vLLM">vLLM</span>`;
    }

    card.innerHTML = `
        <div class="card-header">
            <div class="card-title">${vllmBadge} ${model.name}</div>
            ${deleteBtn}
        </div>
        <div class="card-meta">
            <div><strong data-i18n="ui_models_card_params">${t("ui_models_card_params")}</strong>: ${model.params_billion}B<br>
                <strong data-i18n="ui_models_card_quant">${t("ui_models_card_quant")}</strong>: ${model.quant}<br>
                <strong data-i18n="ui_models_card_context">${t("ui_models_card_context")}</strong>: ${model.context}</div>
            <div><strong data-i18n="ui_models_card_layers">${t("ui_models_card_layers")}</strong>: ${model.layers}<br>
                <strong data-i18n="ui_models_card_hidden">${t("ui_models_card_hidden")}</strong>: ${model.hidden_size}</div>
        </div>
        <div class="card-fit"></div>
        <div class="card-path">${model.path}</div>
    `;

    const fit = card.querySelector(".card-fit");
    if (fit) {
        fit.addEventListener("click", () => openEstimatesModal(model));
    }

    const del = card.querySelector(".card-delete");
    if (del) {
        del.addEventListener("click", (e) => {
            e.stopPropagation();
            openConfirmDeleteModal(model);
        });
    }

    updateCardFit(card);
    return card;
}
"#
    .to_string()
}

fn update_model_fits() -> String {
    r#"
function updateModelFits() {
    document.querySelectorAll(".card").forEach(updateCardFit);
}
"#
    .to_string()
}

fn update_card_fit() -> String {
    r#"
function updateCardFit(card) {
    const fit = card.querySelector(".card-fit");
    if (!fit) return;
    const model = JSON.parse(card.dataset.model);
    const availableVram = currentGpuInfo?.free_gb || 0;
    fit.innerHTML = renderFitSummary(model, availableVram);
}
"#
    .to_string()
}

fn render_fit_summary() -> String {
    r#"
function renderFitSummary(model, availableVram) {
    const best = findBestEstimate(model.estimates, availableVram);
    const minimum = findMinimumEstimate(model.estimates);
    if (!best) return renderNoFit(minimum, availableVram);
    return renderFit(best, availableVram);
}
"#
    .to_string()
}

fn render_fit() -> String {
    r#"
function renderFit(best, availableVram) {
    const margin = availableVram - best.total_gb;
    const tight = margin <= 2.0;
    const fitClass = tight ? "fit-warn" : "fit-ok";
    return `
        <div class="fit-line ${fitClass}">
            ${renderFitItem("ui_models_card_fits_yes", t("ui_models_card_fits_yes"))}
            ${renderSeparator()}
            ${renderFitItem("ui_models_card_best", t("ui_models_card_best"), `${best.context} / ${best.quant}`)}
            ${renderSeparator()}
            ${renderFitItem("ui_models_card_vram", t("ui_models_card_vram"), `${best.total_gb} GB`)}
            ${renderSeparator()}
            ${renderFitItem("ui_models_card_margin", t("ui_models_card_margin"), `${margin.toFixed(2)} GB`)}
        </div>
    `;
}
"#
    .to_string()
}

fn render_no_fit() -> String {
    r#"
function renderNoFit(minimum, availableVram) {
    return `
        <div class="fit-line fit-no">
            ${renderFitItem("ui_models_card_fits_no", t("ui_models_card_fits_no"))}
            ${renderSeparator()}
            ${renderFitItem("ui_models_card_minimum", t("ui_models_card_minimum"), minimum ? `${minimum.total_gb} GB` : "?")}
            ${renderSeparator()}
        </div>
    `;
}
"#
    .to_string()
}

fn render_fit_item() -> String {
    r#"
function renderFitItem(key, label, value) {
    if (value === undefined) {
        return `<span data-i18n="${key}">${label}</span>`;
    }
    return `<span><strong data-i18n="${key}">${label}</strong>: ${value}</span>`;
}
"#
    .to_string()
}

fn render_separator() -> String {
    r#"
function renderSeparator() {
    return `<span class="fit-separator"> | </span>`;
}
"#
    .to_string()
}

fn ensure_estimates_modal() -> String {
    r#"
function ensureEstimatesModal() {
    if (document.getElementById("estimates-modal")) return;
    const modal = document.createElement("div");
    modal.id = "estimates-modal";
    modal.innerHTML = `
        <div class="estimates-modal-backdrop"></div>
        <div class="estimates-modal-content">
            <div class="estimates-modal-header">
                <div class="estimates-modal-title" data-i18n="ui_models_modal_estimates_title">${t("ui_models_modal_estimates_title")}</div>
                <button class="estimates-modal-close" onclick="closeEstimatesModal()"><i class="fa-solid fa-xmark"></i></button>
            </div>
            <div class="estimates-modal-body" id="estimates-modal-body"></div>
        </div>
    `;
    document.body.appendChild(modal);
    estimatesModal = modal;
    modal.querySelector(".estimates-modal-backdrop")?.addEventListener("click", closeEstimatesModal);
}
"#.to_string()
}

fn open_estimates_modal() -> String {
    r#"
function openEstimatesModal(model) {
    ensureEstimatesModal();

    const body = document.getElementById("estimates-modal-body");
    if (!body) return;

    const availableVram = currentGpuInfo?.free_gb || 0;

    body.innerHTML = `
        <div class="estimate-filters">
            <select id="estimate-fit-filter">
                <option value="all" data-i18n="ui_models_modal_estimates_filter_all">${t("ui_models_modal_estimates_filter_all")}</option>
                <option value="fit" data-i18n="ui_models_modal_estimates_filter_fits">${t("ui_models_modal_estimates_filter_fits")}</option>
                <option value="nofit" data-i18n="ui_models_modal_estimates_filter_nofit">${t("ui_models_modal_estimates_filter_nofit")}</option>
            </select>

            <select id="estimate-context-filter">
                <option value="all" data-i18n="ui_models_modal_estimates_filter_all_contexts">${t("ui_models_modal_estimates_filter_all_contexts")}</option>
                ${buildContextOptions(model.estimates)}
            </select>

            <select id="estimate-quant-filter">
                <option value="all" data-i18n="ui_models_modal_estimates_filter_all_quants">${t("ui_models_modal_estimates_filter_all_quants")}</option>
                ${buildQuantOptions(model.estimates)}
            </select>
        </div>

        <div class="estimate-grid" id="estimate-grid"></div>
    `;

    const title = document.querySelector(".estimates-modal-title");
    if (title) { title.textContent = `${t("ui_models_modal_estimates_title")} — ${model.name}`; }

    bindEstimateFilters(model, availableVram);
    renderEstimateGrid(model, availableVram);
    estimatesModal.classList.add("open");
}

function bindEstimateFilters(model, availableVram) {
    [
        "estimate-fit-filter",
        "estimate-context-filter",
        "estimate-quant-filter",
    ].forEach(id => {
        const el = document.getElementById(id);
        if (!el) return;

        el.addEventListener("change", () => { renderEstimateGrid(model, availableVram); });
    });
}

function renderEstimateGrid(model, availableVram) {
    const grid =
        document.getElementById(
            "estimate-grid",
        );

    if (!grid) return;

    const fitFilter =
        document.getElementById(
            "estimate-fit-filter",
        )?.value || "all";

    const contextFilter =
        document.getElementById(
            "estimate-context-filter",
        )?.value || "all";

    const quantFilter =
        document.getElementById(
            "estimate-quant-filter",
        )?.value || "all";

    const estimates =
        model.estimates.filter(e => {
            const fits = e.total_gb <= availableVram;

            if (fitFilter === "fit" && !fits) {
                return false;
            }

            if (fitFilter === "nofit" && fits) {
                return false;
            }

            if (contextFilter !== "all" && String(e.context) !== contextFilter) {
                return false;
            }

            if (quantFilter !== "all" && e.quant !== quantFilter) {
                return false;
            }

            return true;
        });

    grid.innerHTML = estimates.map(e => renderEstimateRow(e, availableVram)).join("");
}

function buildContextOptions(estimates) {
    const values = [...new Set(estimates.map(e => e.context))];
    values.sort((a, b) => contextValue(b) - contextValue(a));

    return values
        .map(v => `<option value="${v}">${v}</option>`)
        .join("");
}

function buildQuantOptions(estimates) {
    const values = [...new Set(estimates.map(e => e.quant))];

    values.sort((a, b) => quantRank(b) - quantRank(a));

    return values
        .map(v => `<option value="${v}">${v}</option>`)
        .join("");
}
"#
    .to_string()
}

fn close_estimates_modal() -> String {
    r#"
function closeEstimatesModal() {
    if (!estimatesModal) return;
    estimatesModal.classList.remove("open");
}

function ensureConfirmDeleteModal() {
    if (document.getElementById("confirm-delete-modal")) return;
    const modal = document.createElement("div");
    modal.id = "confirm-delete-modal";
    modal.className = "estimates-modal"; // Reusing class for layout
    modal.innerHTML = `
        <div class="estimates-modal-backdrop"></div>
        <div class="estimates-modal-content small">
            <div class="estimates-modal-header">
                <div class="estimates-modal-title" data-i18n="ui_models_modal_delete_title">${t("ui_models_modal_delete_title")}</div>
                <button class="estimates-modal-close" onclick="closeConfirmDeleteModal()"><i class="fa-solid fa-xmark"></i></button>
            </div>
            <div class="estimates-modal-body">
                <p data-i18n="ui_models_modal_delete_text">${t("ui_models_modal_delete_text")}</p>
                <div class="model-to-delete-name" id="model-to-delete-name"></div>
                <div class="confirm-actions">
                    <button class="button cancel" onclick="closeConfirmDeleteModal()" data-i18n="ui_models_modal_delete_cancel">${t("ui_models_modal_delete_cancel")}</button>
                    <button class="button delete" onclick="confirmDelete()" data-i18n="ui_models_modal_delete_confirm">${t("ui_models_modal_delete_confirm")}</button>
                </div>
            </div>
        </div>
    `;
    document.body.appendChild(modal);
    confirmDeleteModal = modal;
    modal.querySelector(".estimates-modal-backdrop")?.addEventListener("click", closeConfirmDeleteModal);
}

function openConfirmDeleteModal(model) {
    ensureConfirmDeleteModal();
    modelToDelete = model;
    const nameEl = document.getElementById("model-to-delete-name");
    if (nameEl) nameEl.textContent = model.name;
    confirmDeleteModal.classList.add("open");
}

function closeConfirmDeleteModal() {
    if (!confirmDeleteModal) return;
    confirmDeleteModal.classList.remove("open");
    modelToDelete = null;
}

async function confirmDelete() {
    if (!modelToDelete) return;
    
    const response = await fetch("__MODELS_API_BASE__/delete", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: modelToDelete.path }),
    });

    if (response.ok) {
        closeConfirmDeleteModal();
        refreshModels();
    } else {
        const err = await response.text();
        alert("Failed to delete model: " + err);
    }
}
"#
    .to_string()
}

fn render_estimate_row() -> String {
    r#"
function renderEstimateRow(estimate, availableVram) {
    const margin = availableVram - estimate.total_gb;
    const fits = estimate.total_gb <= availableVram;
    const tight = fits && margin <= 2.0;
    let cls = "fit-no";
    if (fits) cls = tight ? "fit-warn" : "fit-ok";

    return `
        <div class="fit-line ${cls}">
            <div>${renderFitItem("ui_models_card_context", t("ui_models_card_context"), estimate.context)}</div>
            <div>${renderFitItem("ui_models_card_quant", t("ui_models_card_quant"), estimate.quant)}</div>
            <div>${renderFitItem("ui_models_card_vram", t("ui_models_card_vram"), `${estimate.total_gb} GB`)}</div>
            <div>${renderFitItem("ui_models_card_margin", t("ui_models_card_margin"), `${margin.toFixed(2)} GB`)}</div>
        </div>
    `;
}
"#
    .to_string()
}

fn quant_rank() -> String {
    r#"
function quantRank(quant) {
    switch (quant) {
        case "FP16": return 100;
        case "BF16": return 95;
        case "FP8": return 90;
        case "INT8": return 80;
        case "Q80": return 70;
        case "Q6K": return 60;
        case "Q5KM": return 50;
        case "Q50": return 45;
        case "Q4KM": return 40;
        case "Q40": return 35;
        case "Q3KM": return 30;
        case "Q2K": return 20;
        case "AWQ": return 55;
        case "GPTQ": return 50;
        default: return 0;
    }
}
"#
    .to_string()
}

fn context_value() -> String {
    r#"
function contextValue(context) {
    if (typeof context === "number") return context;
    if (typeof context !== "string") return 0;
    return parseInt(context.replace("Size", ""), 10) || 0;
}
"#
    .to_string()
}

fn find_best_estimate() -> String {
    r#"
function findBestEstimate(estimates, availableVram) {
    const fitting = estimates.filter(e => e.total_gb <= availableVram);
    if (fitting.length === 0) return null;
    return fitting.reduce((best, current) => {
        const bestCtx = contextValue(best.context);
        const curCtx = contextValue(current.context);
        if (curCtx > bestCtx || (curCtx === bestCtx && quantRank(current.quant) > quantRank(best.quant))) {
            return current;
        }
        return best;
    });
}
"#.to_string()
}

fn find_minimum_estimate() -> String {
    r#"
function findMinimumEstimate(estimates) {
    if (!estimates || estimates.length === 0) return null;
    return estimates.reduce((min, cur) => cur.total_gb < min.total_gb ? cur : min);
}
"#
    .to_string()
}
