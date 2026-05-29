use quench_srv::prelude::with_base_path;

pub fn ensure_vllm_js() {
    let js = vllm_management_js();

    let _ = std::fs::create_dir_all("dist/assets/js");
    let _ = std::fs::write("dist/assets/js/vllm_management.js", js);
}

fn vllm_management_js() -> String {
    let vllm_api_base = with_base_path("/api/v1/vllm");
    let models_api_base = with_base_path("/api/v1/models");
    let gpu_api_base = with_base_path("/api/v1/gpu");
    let ui_base = with_base_path("/ui");

    let js = r#"
const VLLM_API_BASE = "__VLLM_API_BASE__";
const MODELS_API_BASE = "__MODELS_API_BASE__";
const GPU_API_BASE = "__GPU_API_BASE__";
const UI_BASE = "__UI_BASE__";

function t(key) {
    if (typeof TRANSLATIONS === 'undefined') return key;
    const locale = getLocale();
    const dict = TRANSLATIONS[locale];
    return dict ? (dict[key] || key) : key;
}

let vllmInstances = [];
let availableModels = [];
let currentGpuStatus = { total_gb: 0, free_gb: 0 };
let confirmStopInstanceModal = null;
let instanceToStop = null;
let isAdmin = false;

async function checkAuth() {
    try {
        const response = await fetch(`${UI_BASE}/status`);
        if (response.ok) {
            const status = await response.json();
            isAdmin = status.roles.includes("admin");
        }
    } catch (e) {
        console.error("Failed to check auth status", e);
    }

    updateAdminControls();
}

function updateAdminControls() {
    const launchAction = document.getElementById('launch-instance-action');
    if (launchAction) {
        launchAction.style.display = isAdmin ? '' : 'none';
    }
}

async function fetchModels() {
    const res = await fetch(`${MODELS_API_BASE}/list`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ type: 'HF', name: '', quant: 'ALL', context: 'ALL' })
    });
    availableModels = (await res.json()).filter(m => m.vllm_supported);
    populateModelSelect();
}

function populateModelSelect() {
    const select = document.getElementById('launch-model');
    if (!select) return;
    select.innerHTML = '<option value="">-- select model --</option>';
    availableModels.forEach(m => {
        const opt = document.createElement('option');
        opt.value = m.name;
        opt.textContent = m.name;
        select.appendChild(opt);
    });
}

function renderInstances() {
    const grid = document.getElementById('vllm-instances-grid');
    if (!grid) return;
    grid.innerHTML = '';
    
    if (vllmInstances.length === 0) {
        const empty = cloneTemplateNode('vllm-empty-state-template');
        if (empty) grid.appendChild(empty);
        return;
    }

    vllmInstances.forEach(inst => {
        const card = cloneTemplateNode('vllm-instance-card-template');
        if (!card) return;

        const title = card.querySelector('.card-title');
        if (title) {
            title.textContent = inst.model;
            title.title = inst.model;
        }

        const stopButton = card.querySelector('.card-delete');
        if (stopButton) {
            stopButton.style.display = isAdmin && inst.status === 'running' ? '' : 'none';
            stopButton.addEventListener('click', () => openStopInstanceModal(inst.id, inst.model));
        }

        const id = card.querySelector('.instance-id');
        if (id) id.textContent = inst.id;

        const ns = card.querySelector('.instance-namespace');
        if (ns) ns.textContent = inst.namespace;

        const endpoint = card.querySelector('.instance-endpoint');
        if (endpoint) endpoint.textContent = `${inst.host}:${inst.port}`;

        const status = card.querySelector('.instance-status');
        if (status) {
            status.textContent = inst.status;
            status.className = `instance-status ${instanceStatusClass(inst.status)}`;
        }

        const started = card.querySelector('.instance-started');
        if (started) started.textContent = new Date(inst.started_at).toLocaleString();

        const fitLine = card.querySelector('.fit-line');
        if (fitLine) {
            fitLine.className = `fit-line ${instanceFitClass(inst.status)}`;
            fitLine.replaceChildren(...buildInstanceBadges(inst));
        }

        const diagnostics = card.querySelector('.instance-diagnostics');
        if (diagnostics) {
            populateInstanceDiagnostics(diagnostics, inst);
        }

        grid.appendChild(card);
    });
}

function cloneTemplateNode(id) {
    const template = document.getElementById(id);
    return template?.firstElementChild?.cloneNode(true) || null;
}

function buildInstanceBadges(inst) {
    const badges = [];
    const model = availableModels.find(m => m.name === inst.model);
    const quantization = inst.quantization || normalizeInstanceQuant(model?.quant);
    const context = inst.max_model_len || normalizeInstanceContext(model?.context);

    if (quantization) {
        badges.push(createBadge(quantization));
    }

    if (context) {
        badges.push(createBadge(`ctx: ${context}`));
    }

    if (inst.gpu_memory_utilization) {
        badges.push(createBadge(`gpu: ${Math.round(inst.gpu_memory_utilization * 100)}%`));
    }

    if (inst.enable_prefix_caching) {
        badges.push(createBadge('prefix-caching'));
    }

    return badges;
}

function normalizeInstanceQuant(quant) {
    if (!quant) return null;
    return String(quant).toLowerCase();
}

function normalizeInstanceContext(context) {
    const parsed = parseInt(String(context || ''), 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function createBadge(text) {
    const badge = document.createElement('span');
    badge.className = 'badge';
    badge.textContent = text;
    return badge;
}

function instanceStatusClass(status) {
    if (status === 'failed') return 'status-failed';
    if (status === 'starting') return 'status-starting';
    return 'status-running';
}

function instanceFitClass(status) {
    if (status === 'failed') return 'fit-no';
    if (status === 'starting') return 'fit-warn';
    return 'fit-ok';
}

function populateInstanceDiagnostics(root, inst) {
    const lines = [];

    if (inst.last_error) {
        lines.push({ className: 'instance-error', text: inst.last_error });
    }

    if (inst.log_path) {
        lines.push({ className: 'instance-log-path', text: `log: ${inst.log_path}` });
    }

    if (lines.length === 0) {
        root.style.display = 'none';
        root.replaceChildren();
        return;
    }

    root.style.display = '';
    root.replaceChildren(
        ...lines.map(line => {
            const el = document.createElement('div');
            el.className = line.className;
            el.textContent = line.text;
            return el;
        }),
    );
}

function openStopInstanceModal(id, model) {
    if (!isAdmin) return;
    if (!confirmStopInstanceModal) {
        confirmStopInstanceModal = document.getElementById("confirm-stop-instance-modal");
    }
    if (!confirmStopInstanceModal) return;

    instanceToStop = { id, model };

    const nameEl =
        document.getElementById(
            "instance-to-stop-name",
        );

    if (nameEl) {
        nameEl.textContent = model;
    }

    confirmStopInstanceModal.classList.add("open");
}

function closeStopInstanceModal() {
    if (!confirmStopInstanceModal) return;

    confirmStopInstanceModal.classList.remove("open");

    instanceToStop = null;
}

async function confirmStopInstance() {
    if (!isAdmin) return;
    if (!instanceToStop) return;

    const response = await fetch(
        `${VLLM_API_BASE}/instances/${instanceToStop.id}`,
        {
            method: "DELETE",
        },
    );

    if (response.ok) {
        closeStopInstanceModal();
    } else {
        const err = await response.text();

        alert(
            "Failed to stop instance: " + err,
        );
    }
}

function openLaunchModal() {
    if (!isAdmin) return;
    updateFitNote();
    document.getElementById('launch-modal').style.display = 'flex';
}

function closeLaunchModal() {
    document.getElementById('launch-modal').style.display = 'none';
}

function onLaunchModelChange() {
    syncMinimumGpuUtil();
    updateFitNote();
}

function updateFitNote() {
    const modelPath = document.getElementById('launch-model').value;
    const gpuUtilInput = document.getElementById('launch-gpu-util');
    const gpuUtil = parseFloat(gpuUtilInput?.value || '');
    const quantization = document.getElementById('launch-quant')?.value || '';
    const maxModelLenInput = document.getElementById('launch-max-len');
    const requestedContext = parseLaunchContext(maxModelLenInput?.value);
    const noteEl = document.getElementById('launch-fit-note');
    const launchBtn = document.getElementById('confirm-launch-btn');
    
    if (!modelPath) {
        setFitNote(noteEl, 'fit-line fit-warn', 'Select a model to estimate required VRAM.');
        launchBtn.disabled = true;
        return;
    }

    const model = availableModels.find(m => m.name === modelPath);
    if (!model) {
        setFitNote(noteEl, 'fit-line fit-no', 'Selected model is not available.');
        launchBtn.disabled = true;
        return;
    }

    if (!Number.isFinite(gpuUtil) || gpuUtil <= 0) {
        setFitNote(noteEl, 'fit-line fit-no', 'GPU memory utilization must be greater than 0.');
        launchBtn.disabled = true;
        return;
    }

    const estimate = findLaunchEstimate(model, quantization, requestedContext);
    if (!estimate) {
        setFitNote(
            noteEl,
            'fit-line fit-warn',
            'No matching estimate available for the selected quantization or context.',
        );
        launchBtn.disabled = true;
        return;
    }

    const kvGb = Number(estimate.kv_gb || 0);
    const weightsGb = Number(estimate.weights_gb || 0);
    const estimatedModelGb = weightsGb + (kvGb * gpuUtil);
    const totalBudgetGb = currentGpuStatus.total_gb * gpuUtil;
    const reservationGb = totalBudgetGb;
    const requiredGb = Math.max(estimatedModelGb, reservationGb);
    const freeGb = currentGpuStatus.free_gb;
    const remainingGb = freeGb - requiredGb;
        
    if (estimatedModelGb > totalBudgetGb) {
         setFitNote(noteEl, 'fit-line fit-no', `Won't fit: model needs ~${estimatedModelGb.toFixed(2)} GB for the selected max length, but gpu memory utilization allows only ${totalBudgetGb.toFixed(2)} GB`);
         launchBtn.disabled = true;
         return;
    }

    if (requiredGb > freeGb) {
         setFitNote(noteEl, 'fit-line fit-no', `Won't fit right now: vLLM will reserve ~${requiredGb.toFixed(2)} GB, but only ${freeGb.toFixed(2)} GB is free`);
         launchBtn.disabled = true;
         return;
    }

    if (remainingGb < 2) {
         setFitNote(noteEl, 'fit-line fit-warn', `Tight fit: model needs ~${estimatedModelGb.toFixed(2)} GB and vLLM will reserve ~${requiredGb.toFixed(2)} GB, leaving ${remainingGb.toFixed(2)} GB free`);
         launchBtn.disabled = false;
         return;
    }

    setFitNote(noteEl, 'fit-line fit-ok', `Should fit: model needs ~${estimatedModelGb.toFixed(2)} GB and vLLM will reserve ~${requiredGb.toFixed(2)} GB`);
    launchBtn.disabled = false;
}

function parseLaunchContext(value) {
    const parsed = parseInt(value || '', 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function roundGpuUtilUp(value) {
    return Math.ceil(value * 20) / 20;
}

function normalizeEstimateQuant(quantization, model) {
    if (!quantization) return String(model.quant);

    switch (quantization) {
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
    }
}

function estimateContextValue(context) {
    if (typeof context === 'number') return context;
    const parsed = parseInt(String(context), 10);
    return Number.isFinite(parsed) ? parsed : null;
}

function findLaunchEstimate(model, quantization, requestedContext) {
    const estimateQuant = normalizeEstimateQuant(quantization, model);
    let candidates = model.estimates || [];

    if (estimateQuant) {
        const byQuant = candidates.filter(e => String(e.quant) === estimateQuant);
        if (byQuant.length > 0) {
            candidates = byQuant;
        }
    }

    if (requestedContext) {
        const withContext = candidates
            .map(e => ({ estimate: e, context: estimateContextValue(e.context) }))
            .filter(e => e.context !== null);

        const atLeastRequested = withContext
            .filter(e => e.context >= requestedContext)
            .sort((a, b) => a.context - b.context);

        if (atLeastRequested.length > 0) {
            return atLeastRequested[0].estimate;
        }

        const largestAvailable = withContext.sort((a, b) => b.context - a.context);
        if (largestAvailable.length > 0) {
            return largestAvailable[0].estimate;
        }
    }

    return candidates[0] || null;
}

function calculateMinimumGpuUtil(estimate, totalGpuGb) {
    const kvGb = Number(estimate?.kv_gb || 0);
    const weightsGb = Number(estimate?.weights_gb || 0);
    const safetyMarginGb = 1.5;

    if (!Number.isFinite(totalGpuGb) || totalGpuGb <= 0) {
        return null;
    }

    const denominator = totalGpuGb - kvGb;
    if (denominator <= 0) {
        return null;
    }

    const raw = (weightsGb + safetyMarginGb) / denominator;
    if (!Number.isFinite(raw) || raw <= 0) {
        return 0.2;
    }

    return Math.min(1.0, Math.max(0.2, roundGpuUtilUp(raw)));
}

function syncMinimumGpuUtil() {
    const modelName = document.getElementById('launch-model')?.value;
    const quantization = document.getElementById('launch-quant')?.value || '';
    const requestedContext = parseLaunchContext(document.getElementById('launch-max-len')?.value);
    const gpuUtilInput = document.getElementById('launch-gpu-util');
    if (!gpuUtilInput || !modelName) return;

    const model = availableModels.find(m => m.name === modelName);
    if (!model) return;

    const estimate = findLaunchEstimate(model, quantization, requestedContext);
    if (!estimate) return;

    const minimumGpuUtil = calculateMinimumGpuUtil(estimate, currentGpuStatus.total_gb);
    if (minimumGpuUtil === null) return;

    gpuUtilInput.value = minimumGpuUtil.toFixed(2);
}

function setFitNote(noteEl, className, text) {
    if (!noteEl) return;
    const line = document.createElement('div');
    line.className = className;
    const icon = document.createElement('i');
    icon.className = fitNoteIconClass(className);

    const copy = document.createElement('span');
    copy.textContent = text;

    line.appendChild(icon);
    line.appendChild(copy);
    noteEl.replaceChildren(line);
}

function fitNoteIconClass(className) {
    if (className.includes('fit-no')) {
        return 'fa-solid fa-circle-xmark';
    }
    if (className.includes('fit-warn')) {
        return 'fa-solid fa-triangle-exclamation';
    }
    return 'fa-solid fa-circle-check';
}

async function launchVllmInstance() {
    if (!isAdmin) return;
    const model = document.getElementById('launch-model').value;
    const host = document.getElementById('launch-host').value;
    const port = parseInt(document.getElementById('launch-port').value);
    const namespace = document.getElementById('launch-namespace').value;
    const quantization = document.getElementById('launch-quant').value;
    const max_model_len = parseInt(document.getElementById('launch-max-len').value) || null;
    const gpu_memory_utilization = parseFloat(document.getElementById('launch-gpu-util').value);
    const enable_prefix_caching = document.getElementById('launch-prefix-caching').checked;

    if (!model) {
        alert('Please select a model');
        return;
    }

    const req = {
        model, host, port, 
        namespace: namespace || null,
        quantization: quantization || null,
        max_model_len,
        gpu_memory_utilization,
        enable_prefix_caching
    };

    const res = await fetch(`${VLLM_API_BASE}/instances`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(req)
    });

    if (res.ok) {
        closeLaunchModal();
    } else {
        const err = await res.text();
        alert('Failed to launch: ' + err);
    }
}

window.addEventListener('gpu-update', (e) => {
    currentGpuStatus = e.detail;
    updateFitNote();
});

window.addEventListener('vllm-instances-update', (e) => {
    vllmInstances = e.detail;
    renderInstances();
});

window.addEventListener('DOMContentLoaded', () => {
    confirmStopInstanceModal = document.getElementById("confirm-stop-instance-modal");
    document.getElementById('launch-quant')?.addEventListener('change', () => {
        syncMinimumGpuUtil();
        updateFitNote();
    });
    document.getElementById('launch-gpu-util')?.addEventListener('input', updateFitNote);
    document.getElementById('launch-max-len')?.addEventListener('input', () => {
        syncMinimumGpuUtil();
        updateFitNote();
    });
    checkAuth().then(() => {
        fetchModels();
        updateFitNote();
    });
});
"#.to_string();

    js.replace("__VLLM_API_BASE__", &vllm_api_base)
        .replace("__MODELS_API_BASE__", &models_api_base)
        .replace("__GPU_API_BASE__", &gpu_api_base)
        .replace("__UI_BASE__", &ui_base)
}
