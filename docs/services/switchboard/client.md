---
title: Client
parent: Switchboard
grand_parent: Services
nav_order: 5
---

The browser client is `switchboard-app` compiled with `switchboard-web` to WebAssembly, served at `{base}/ui/assets/app.wasm`. Pattern: [The browser-client pattern](../../guides/browser-client.md).

## Routes (owned by the app)

| Path | Screen | On entry and while open |
|---|---|---|
| `{base}/ui`, `{base}/ui/`, `{base}/ui/home`, `{base}/ui/home/` | Home | nothing (capabilities were asked at start) |
| `{base}/ui/models/dashboard` (also with trailing `/`) | Models registry | `Gpu`, `Models(filters)`; `Gpu` every 1 s |
| `{base}/ui/vllm/manage` (also with trailing `/`) | vLLM management | `Gpu`, `Instances`, `Catalog`; `Gpu` every 1 s, `Instances` every 2 s |
| anything else | not found (`ui_not_found`) | |

The service serves page routes for the same six paths, plus `/ui` and `/ui/`. `Startup` is asked once when the app starts. The sidebar shows Home always, "Models" only when `caps.models_dashboard`, "vLLM" only when `caps.vllm_management` (`nav-home`, `nav-models`, `nav-vllm`, icons house, layers, server).

## Request and Msg

| Request | Call | Messages |
|---|---|---|
| `Startup` | `GET /api/v1/vllm/capabilities` | `Capabilities` |
| `Gpu` | `GET /api/v1/gpu/status` | `Gpu` |
| `Models(filters)` | `POST /api/v1/models/list` with the filters | `Models(filters, list)` (only the answer for the latest filters is used) |
| `Catalog` | `POST /api/v1/models/list` with `{source: "all", search: "", sort: "name_asc", quant: "ALL", context: "0", vllm_only: false}` | `Catalog` (the launch form's list) |
| `DeleteModel(path)` | `POST /api/v1/models/delete` `{path}` | `ModelDeleted(path)`; the app asks `Models` and `Catalog` again |
| `Instances` | `GET /api/v1/vllm/instances` | `Instances` |
| `Launch(body)` | `POST /api/v1/vllm/instances` | `Launched`; the app closes the form and asks `Instances` |
| `Stop(id)` | `DELETE /api/v1/vllm/instances/{id}` (percent-encoded) | `Stopped(id)`; the app asks `Instances` |
| `Language(code)` | none (storage key `language`) | none |

Failures: reads: 401 `SignedOut`, 403 `Failed("this account may not see Switchboard: ask for access in gatehouse")`, other `Failed("the service answered <n>")`, offline `Failed("cannot reach the service: ...")`. Writes (delete, launch, stop): 401 `SignedOut`; 403 with an empty body `Rejected("this account may not do that: ask for access in gatehouse")`; 404 with an empty body `Rejected("it is gone: someone removed it")`; otherwise `Rejected` with the JSON `error` or the plain body when it is short (200 characters or less) and not HTML, else `the service answered <n>`. Because the service answers plain-text keys such as `api_error_vllm_launch_failed`, the screen translates any rejection starting with `api_error_` through its i18n file (`screens::reason`). A rejection during a launch shows in `p#launch-error` inside the form; otherwise in `p#notice`.

## Screens

* Home (`screens::home`): tiles `a#tile-models` (title `ui_service_models_dashboard_title`) and `a#tile-vllm` for the enabled features; `p#no-services` when neither is on; `p#waiting` before the capabilities arrive.
* Models (`screens::models`): the GPU box `div#gpu-status` (name, total, free GB, or `GPU: n/a`); filters `div#model-filters` (source tabs All, HF, GGUF; `input#search`; sort select; quantisation select of the source; context select; checkbox "vLLM only"); grid `#models-grid` of cards (name, params, context, quant, layers, hidden size, "Fits: YES/NO" line from the free memory with the best estimate's context, quantisation, total VRAM and margin, an Estimates button, a delete button when `can_delete_model`); `p#no-models` when empty. Estimates window (`#estimate-grid`): filters fit/context/quantisation and one row per estimate. Delete confirmation window (`#delete-cancel`, `#delete-confirm`) states "Are you sure you want to physically delete this model from drive?".
* vLLM (`screens::vllm`): toolbar with the GPU box and, if `can_launch`, `button#launch-instance-action`; grid `#vllm-instances-grid` of cards (model, a GPU or CPU badge, id, namespace, endpoint, status, started, GPU utilisation or device, a stop button `[data-stop=ID]` when `can_stop` and the instance is stoppable), `p#no-instances` when empty; stop confirmation (`#stop-cancel`, `#stop-confirm`); launch window `form#launch-form` with fields `launch-model` (select of the catalog), `launch-host` (default `0.0.0.0`), `launch-port` (`8000`), `launch-namespace` (pre-filled with the capability namespace), `launch-quant` (awq, gptq, awq_marlin, gptq_marlin, fp8, bitsandbytes), `launch-dtype` (float16, bfloat16, float32), `launch-device` (GPU or cpu), `launch-max-len`, `launch-limit-mm`, `launch-gpu-util` (default 0.90, disabled for cpu), checkboxes `launch-prefix-caching` and `launch-enable-tool-calling`, task select (auto, generate, embed), the fit note `#launch-fit-note`, `p#launch-error`, buttons `#launch-cancel` and `#confirm-launch-btn` (disabled while the fit check says it will not fit, or while a launch is in flight). Choosing a model, quantisation, length or device recomputes the suggested utilisation and the note ([Models and GPU sizing](gpu-sizing.md)). A blank model is "choose a model to launch". Port and length that do not parse fall back to 8000 and none; a CPU launch carries no `gpu_memory_utilization`. `max_num_seqs` is not offered.

## Languages and i18n keys

Locales `en-US`, `de-DE`, `es-ES`, `fr-FR`, `pl-PL` from `services/switchboard/service/i18n/<locale>.ftl`. Key groups in `en-US.ftl`: shell (`header_label`, `footer_label`, `locale_label`, `theme_label`, `ui_header_home`, `ui_home_button`, `ui_dashboard_button`, `ui_theme_*`, `ui_palette_*`, `ui_user_menu`, `ui_profile_edit`, `ui_logout`, `ui_menu_open`, `ui_menu_close`, `ui_close`, `ui_cancel`, `ui_common_cancel`); home (`ui_home_title`, `ui_home_no_services`, `ui_home_group_services`, `ui_service_models_dashboard_title|desc`, `ui_service_vllm_management_title|desc`); registry (`ui_header_dashboard`, `ui_header_models`, `ui_header_vllm`, `ui_models_search_placeholder`, `ui_models_gpu_total`, `ui_models_gpu_free`, `ui_models_sort_{name,params,vram}_{asc,desc}`, `ui_models_tab_all|hf|gguf`, `ui_models_filter_all_quants|all_contexts|vllm_only`, `ui_models_card_*`, `ui_models_modal_estimates_*`, `ui_models_modal_delete_*`, `ui_models_quant_*`, `ui_models_empty`, `ui_models_card_no_estimates`, `ui_gpu_unavailable`, `ui_gpu_label`); vLLM (`ui_vllm_launch_new`, `ui_vllm_running_instances`, `ui_vllm_launch_modal_title|subtitle`, `ui_vllm_form_*`, `ui_vllm_launch_confirm`, `ui_vllm_no_instances`, `ui_vllm_stop_*`, `ui_vllm_unknown_model`, `ui_vllm_meta_*`, `ui_vllm_device_gpu|cpu`, `ui_vllm_status_running|starting|pending|failed|terminating`, `ui_vllm_fit_select_model|no_estimate|wont_fit_budget|wont_fit_free|tight|ok`, `ui_vllm_fit_note_cpu`); API error codes (`api_error_model_name_empty`, `api_error_vllm_launch_failed`, `api_error_vllm_stop_failed`, `api_error_vllm_list_failed`, `api_error_instance_not_found`, `api_error_invalid_model_path`, `api_error_model_not_found`, `api_error_model_delete_failed`); client (`ui_loading`, `ui_not_found`, `ui_session_ended`, `ui_login_sign_in`). Fit messages take the placeholders `$model`, `$budget`, `$required`, `$free`, `$remaining`. Status labels exist only for running, starting, pending, failed, terminating: other statuses (`completed`, `interrupted`, `Failed`) are shown as the raw text.

## Styling hooks

Root `div#switchboard.fui-app.fui-client`, content `main.sb-main`. Classes: `sb-page`, `sb-toolbar`, `sb-grow`, `sb-gpu`, `sb-grid`, `sb-btn`, `sb-primary`, `sb-icon-btn`, `sb-current`, `sb-muted`, `fui-link-card*`, `badge-device`, `badge-cpu`, `badge-gpu`, `status-failed`, `estimate-grid`, `estimates-modal`, `launch-modal`.
