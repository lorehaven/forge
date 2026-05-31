use quench_srv::prelude::with_base_path;
use quench_web::prelude::{Script, js};

pub fn vllm_management_script() -> Script {
    let vllm_api_base = with_base_path("/api/v1/vllm");
    let models_api_base = with_base_path("/api/v1/models");
    let ui_base = with_base_path("/ui");

    js!(format!(
        r#"
    {}
    {}
    {}
    {}
    {}
    "#,
        htmx_js(),
        auth_js(&ui_base),
        models_js(&models_api_base),
        launch_modal_js(&vllm_api_base),
        stop_modal_js(&vllm_api_base),
    ))
}

fn htmx_js() -> String {
    r#"
function getAvailableVram() {
    const el = document.querySelector('#gpu-status .gpu-free span:last-child');
    if (!el) return 0;
    const val = el.textContent.replace(/[^\d.]/g, '');
    return parseFloat(val) || 0;
}

function getTotalVram() {
    const el = document.querySelector('#gpu-status .gpu-total span:last-child');
    if (!el) return 0;
    const val = el.textContent.replace(/[^\d.]/g, '');
    return parseFloat(val) || 0;
}

window.addEventListener('gpu-update', (e) => {
    if (typeof updateFitNote === 'function') {
        updateFitNote();
    }
    // Dispatch gpu-status event to trigger HTMX refresh for vllm instances grid
    window.dispatchEvent(new CustomEvent("gpu-status"));
});

window.addEventListener('DOMContentLoaded', () => {
    if (typeof checkAuth === 'function') {
        checkAuth().then(() => {
            if (typeof fetchModels === 'function') {
                fetchModels();
            }
            if (typeof updateFitNote === 'function') {
                updateFitNote();
            }
        });
    }
});
"#
    .to_string()
}

fn auth_js(ui_base: &str) -> String {
    format!(
        r##"
let isAdmin = false;

async function checkAuth() {{
    try {{
        const response = await fetch("{ui_base}/status");
        if (response.ok) {{
            const status = await response.json();
            isAdmin = status.roles.includes("admin");
        }}
    }} catch (e) {{
        console.error("Failed to check auth status", e);
    }}

    updateAdminControls();
}}

function updateAdminControls() {{
    const launchAction = document.getElementById('launch-instance-action');
    if (launchAction) {{
        launchAction.style.display = isAdmin ? '' : 'none';
    }}
}}
"##,
        ui_base = ui_base
    )
}

fn models_js(models_api_base: &str) -> String {
    format!(
        r##"
let availableModels = [];

async function fetchModels() {{
    const res = await fetch("{models_api_base}/list", {{
        method: 'POST',
        headers: {{ 'Content-Type': 'application/json' }},
        body: JSON.stringify({{ type: 'HF', name: '', quant: 'ALL', context: 'ALL' }})
    }});
    availableModels = (await res.json()).filter(m => m.vllm_supported);
    populateModelSelect();
}}

function populateModelSelect() {{
    const select = document.getElementById('launch-model');
    if (!select) return;
    select.innerHTML = '<option value="">-- select model --</option>';
    availableModels.forEach(m => {{
        const opt = document.createElement('option');
        opt.value = m.name;
        opt.textContent = m.name;
        select.appendChild(opt);
    }});
}}
"##,
        models_api_base = models_api_base
    )
}

fn stop_modal_js(vllm_api_base: &str) -> String {
    format!(
        r##"
let confirmStopInstanceModal = null;
let instanceToStop = null;

function openStopInstanceModal(id, model) {{
    if (!isAdmin) return;
    if (!confirmStopInstanceModal) {{
        confirmStopInstanceModal = document.getElementById("confirm-stop-instance-modal");
    }}
    if (!confirmStopInstanceModal) return;

    instanceToStop = {{ id, model }};

    const nameEl = document.getElementById("instance-to-stop-name");
    if (nameEl) {{
        nameEl.textContent = model;
    }}

    confirmStopInstanceModal.classList.add("open");
}}

function closeStopInstanceModal() {{
    if (!confirmStopInstanceModal) return;
    confirmStopInstanceModal.classList.remove("open");
    instanceToStop = null;
}}

async function confirmStopInstance() {{
    if (!isAdmin) return;
    if (!instanceToStop) return;

    const response = await fetch(
        "{vllm_api_base}/instances/" + instanceToStop.id,
        {{
            method: "DELETE",
        }},
    );

    if (response.ok) {{
        closeStopInstanceModal();
        window.dispatchEvent(new CustomEvent("gpu-status"));
    }} else {{
        const err = await response.text();
        alert("Failed to stop instance: " + err);
    }}
}}
"##,
        vllm_api_base = vllm_api_base
    )
}

fn launch_modal_js(vllm_api_base: &str) -> String {
    format!(
        r##"
function openLaunchModal() {{
    if (!isAdmin) return;
    updateFitNote();
    document.getElementById('launch-modal').style.display = 'flex';
}}

function closeLaunchModal() {{
    document.getElementById('launch-modal').style.display = 'none';
}}

function onLaunchModelChange() {{
    syncMinimumGpuUtil();
    updateFitNote();
}}

function updateFitNote() {{
    const modelPath = document.getElementById('launch-model').value;
    const gpuUtilInput = document.getElementById('launch-gpu-util');
    const gpuUtil = parseFloat(gpuUtilInput?.value || '');
    const quantization = document.getElementById('launch-quant')?.value || '';
    const maxModelLenInput = document.getElementById('launch-max-len');
    const requestedContext = parseLaunchContext(maxModelLenInput?.value);
    const noteEl = document.getElementById('launch-fit-note');
    const launchBtn = document.getElementById('confirm-launch-btn');
    
    if (!modelPath) {{
        setFitNote(noteEl, 'fit-line fit-warn', 'ui_vllm_fit_select_model', 'Select a model to estimate required VRAM.');
        launchBtn.disabled = true;
        return;
    }}

    const model = availableModels.find(m => m.name === modelPath);
    if (!model) {{
        setFitNote(noteEl, 'fit-line fit-no', 'ui_vllm_fit_model_not_available', 'Selected model is not available.');
        launchBtn.disabled = true;
        return;
    }}

    if (!Number.isFinite(gpuUtil) || gpuUtil <= 0) {{
        setFitNote(noteEl, 'fit-line fit-no', 'ui_vllm_fit_invalid_gpu_util', 'GPU memory utilization must be greater than 0.');
        launchBtn.disabled = true;
        return;
    }}

    const estimate = findLaunchEstimate(model, quantization, requestedContext);
    if (!estimate) {{
        setFitNote(
            noteEl,
            'fit-line fit-warn',
            'ui_vllm_fit_no_estimate',
            'No matching estimate available for the selected quantization or context.',
        );
        launchBtn.disabled = true;
        return;
    }}

    const kvGb = Number(estimate.kv_gb || 0);
    const weightsGb = Number(estimate.weights_gb || 0);
    const estimatedModelGb = weightsGb + (kvGb * gpuUtil);
    
    const totalGpuGb = getTotalVram();
    const totalBudgetGb = totalGpuGb * gpuUtil;
    const reservationGb = totalBudgetGb;
    const requiredGb = Math.max(estimatedModelGb, reservationGb);
    const freeGb = getAvailableVram();
    const remainingGb = freeGb - requiredGb;
        
    if (estimatedModelGb > totalBudgetGb) {{
         setFitNote(noteEl, 'fit-line fit-no', 'ui_vllm_fit_wont_fit_budget', "Won't fit: model needs ~" + estimatedModelGb.toFixed(2) + " GB for the selected max length, but gpu memory utilization allows only " + totalBudgetGb.toFixed(2) + " GB");
         launchBtn.disabled = true;
         return;
    }}

    if (requiredGb > freeGb) {{
         setFitNote(noteEl, 'fit-line fit-no', 'ui_vllm_fit_wont_fit_free', "Won't fit right now: vLLM will reserve ~" + requiredGb.toFixed(2) + " GB, but only " + freeGb.toFixed(2) + " GB is free");
         launchBtn.disabled = true;
         return;
    }}

    if (remainingGb < 2) {{
         setFitNote(noteEl, 'fit-line fit-warn', 'ui_vllm_fit_tight', "Tight fit: model needs ~" + estimatedModelGb.toFixed(2) + " GB and vLLM will reserve ~" + requiredGb.toFixed(2) + " GB, leaving " + remainingGb.toFixed(2) + " GB free");
         launchBtn.disabled = false;
         return;
    }}

    setFitNote(noteEl, 'fit-line fit-ok', 'ui_vllm_fit_ok', "Should fit: model needs ~" + estimatedModelGb.toFixed(2) + " GB and vLLM will reserve ~" + requiredGb.toFixed(2) + " GB");
    launchBtn.disabled = false;
}}

function parseLaunchContext(value) {{
    const parsed = parseInt(value || '', 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}}

function roundGpuUtilUp(value) {{
    return Math.ceil(value * 20) / 20;
}}

function normalizeEstimateQuant(quantization, model) {{
    if (!quantization) return String(model.quant);

    switch (quantization) {{
        case 'awq':
            return 'AWQ';
        case 'gptq':
        case 'gptq_marlin':
            return 'GPTQ';
        case 'awq_marlin':
            return 'AWQ';
        case 'fp8':
            return 'FP8';
        case 'bitsandbytes':
            return 'INT8';
        default:
            return null;
    }}
}}

function estimateContextValue(context) {{
    if (typeof context === 'number') return context;
    const parsed = parseInt(String(context), 10);
    return Number.isFinite(parsed) ? parsed : null;
}}

function findLaunchEstimate(model, quantization, requestedContext) {{
    const estimateQuant = normalizeEstimateQuant(quantization, model);
    let candidates = model.estimates || [];

    if (estimateQuant) {{
        const byQuant = candidates.filter(e => String(e.quant) === estimateQuant);
        if (byQuant.length > 0) {{
            candidates = byQuant;
        }}
    }}

    if (requestedContext) {{
        const withContext = candidates
            .map(e => ({{ estimate: e, context: estimateContextValue(e.context) }}))
            .filter(e => e.context !== null);

        const atLeastRequested = withContext
            .filter(e => e.context >= requestedContext)
            .sort((a, b) => a.context - b.context);

        if (atLeastRequested.length > 0) {{
            return atLeastRequested[0].estimate;
        }}

        const largestAvailable = withContext.sort((a, b) => b.context - a.context);
        if (largestAvailable.length > 0) {{
            return largestAvailable[0].estimate;
        }}
    }}

    return candidates[0] || null;
}}

function calculateMinimumGpuUtil(estimate, totalGpuGb) {{
    const kvGb = Number(estimate?.kv_gb || 0);
    const weightsGb = Number(estimate?.weights_gb || 0);
    const safetyMarginGb = 1.5;

    if (!Number.isFinite(totalGpuGb) || totalGpuGb <= 0) {{
        return null;
    }}

    const denominator = totalGpuGb - kvGb;
    if (denominator <= 0) {{
        return null;
    }}

    const raw = (weightsGb + safetyMarginGb) / denominator;
    if (!Number.isFinite(raw) || raw <= 0) {{
        return 0.2;
    }}

    return Math.min(1.0, Math.max(0.2, roundGpuUtilUp(raw)));
}}

function syncMinimumGpuUtil() {{
    const modelName = document.getElementById('launch-model')?.value;
    const quantization = document.getElementById('launch-quant')?.value || '';
    const requestedContext = parseLaunchContext(document.getElementById('launch-max-len')?.value);
    const gpuUtilInput = document.getElementById('launch-gpu-util');
    if (!gpuUtilInput || !modelName) return;

    const model = availableModels.find(m => m.name === modelName);
    if (!model) return;

    const estimate = findLaunchEstimate(model, quantization, requestedContext);
    if (!estimate) return;

    const minimumGpuUtil = calculateMinimumGpuUtil(estimate, getTotalVram());
    if (minimumGpuUtil === null) return;

    gpuUtilInput.value = minimumGpuUtil.toFixed(2);
}}

function setFitNote(noteEl, className, i18nKey, fallbackText) {{
    if (!noteEl) return;
    const line = document.createElement('div');
    line.className = className;
    const icon = document.createElement('i');
    icon.className = fitNoteIconClass(className);

    const copy = document.createElement('span');
    copy.dataset.i18n = i18nKey;
    copy.textContent = fallbackText;

    line.appendChild(icon);
    line.appendChild(copy);
    noteEl.replaceChildren(line);
    
    // Trigger i18n scan if global function exists
    if (window.qUpdateI18n) window.qUpdateI18n();
}}

function fitNoteIconClass(className) {{
    if (className.includes('fit-no')) {{
        return 'fa-solid fa-circle-xmark';
    }}
    if (className.includes('fit-warn')) {{
        return 'fa-solid fa-triangle-exclamation';
    }}
    return 'fa-solid fa-circle-check';
}}

async function launchVllmInstance() {{
    if (!isAdmin) return;
    const model = document.getElementById('launch-model').value;
    const host = document.getElementById('launch-host').value;
    const port = parseInt(document.getElementById('launch-port').value);
    const namespace = document.getElementById('launch-namespace').value;
    const quantization = document.getElementById('launch-quant').value;
    const max_model_len = parseInt(document.getElementById('launch-max-len').value) || null;
    const gpu_memory_utilization = parseFloat(document.getElementById('launch-gpu-util').value);
    const enable_prefix_caching = document.getElementById('launch-prefix-caching').checked;

    if (!model) {{
        alert('Please select a model');
        return;
    }}

    const req = {{
        model, host, port, 
        namespace: namespace || null,
        quantization: quantization || null,
        max_model_len,
        gpu_memory_utilization,
        enable_prefix_caching
    }};

    const res = await fetch("{vllm_api_base}/instances", {{
        method: 'POST',
        headers: {{ 'Content-Type': 'application/json' }},
        body: JSON.stringify(req)
    }});

    if (res.ok) {{
        closeLaunchModal();
        window.dispatchEvent(new CustomEvent("gpu-status"));
    }} else {{
        const err = await res.text();
        alert('Failed to launch: ' + err);
    }}
}}
"##,
        vllm_api_base = vllm_api_base
    )
}
