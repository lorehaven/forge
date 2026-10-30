use switchboard_core::fit::{
    CardFit, FitTone, best_estimate, card_fit, estimate_rows, filter_options, find_launch_estimate,
    launch_fit, launch_gpu_util, minimum_gpu_util, round_gpu_util_up,
};
use switchboard_core::{Context, Estimate, Gpu, Model};

fn est(quant: &str, context: u32, weights: f64, kv: f64) -> Estimate {
    Estimate {
        quant: quant.into(),
        context: Context(context),
        weights_gb: weights,
        kv_gb: kv,
        total_gb: weights + kv,
    }
}

fn model(quant: &str, estimates: Vec<Estimate>) -> Model {
    Model {
        source: "HF".into(),
        name: "m".into(),
        path: "/m".into(),
        architecture: None,
        vllm_supported: true,
        quant: quant.into(),
        context: Context(0),
        layers: 32,
        hidden_size: 4096,
        params_billion: 7.0,
        estimates,
    }
}

fn gpu(total: f64, free: f64) -> Gpu {
    Gpu {
        name: "card0".into(),
        total_gb: total,
        used_gb: total - free,
        free_gb: free,
    }
}

#[quench_test::test]
fn the_best_estimate_is_the_longest_context_that_fits() {
    let list = vec![
        est("FP16", 4096, 14.0, 1.0),
        est("Q4KM", 8192, 4.0, 2.0),
        est("FP16", 8192, 14.0, 2.0),
        est("BF16", 8192, 14.0, 2.0),
    ];
    assert_eq!(best_estimate(&list, 10.0).unwrap().quant, "Q4KM");
    assert_eq!(best_estimate(&list, 17.0).unwrap().quant, "FP16");
    assert_eq!(best_estimate(&list, 17.0).unwrap().context.0, 8192);
    assert!(best_estimate(&list, 1.0).is_none());
}

#[quench_test::test]
fn a_card_says_fits_tight_too_big_or_nothing() {
    let m = model("FP16", vec![est("FP16", 4096, 14.0, 1.0)]);
    match card_fit(&m, 30.0) {
        CardFit::Fits {
            tone,
            margin_gb,
            quant,
            context,
            ..
        } => {
            assert_eq!(tone, FitTone::Ok);
            assert!((margin_gb - 15.0).abs() < 1e-9);
            assert_eq!((quant.as_str(), context.as_str()), ("FP16", "4096"));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        card_fit(&m, 16.0),
        CardFit::Fits {
            tone: FitTone::Warn,
            ..
        }
    ));
    assert_eq!(card_fit(&m, 4.0), CardFit::TooBig { min_gb: 15.0 });
    assert_eq!(card_fit(&model("FP16", vec![]), 4.0), CardFit::NoEstimates);
}

#[quench_test::test]
fn the_estimates_table_filters() {
    let m = model(
        "FP16",
        vec![
            est("FP16", 4096, 14.0, 1.0),
            est("AWQ", 4096, 4.0, 1.0),
            est("AWQ", 8192, 4.0, 2.0),
        ],
    );
    assert_eq!(estimate_rows(&m, 10.0, "all", "all", "all").len(), 3);
    let fits = estimate_rows(&m, 10.0, "fit", "all", "all");
    assert_eq!(fits.len(), 2);
    assert!(fits.iter().all(|r| r.quant == "AWQ"));
    let nofit = estimate_rows(&m, 10.0, "nofit", "all", "all");
    assert_eq!((nofit.len(), nofit[0].tone), (1, FitTone::No));
    assert_eq!(estimate_rows(&m, 10.0, "all", "8192", "all").len(), 1);
    assert_eq!(estimate_rows(&m, 10.0, "all", "all", "FP16").len(), 1);
    let tight = estimate_rows(&m, 7.0, "fit", "all", "AWQ");
    assert_eq!(tight[0].tone, FitTone::Warn);
    let (contexts, quants) = filter_options(&m);
    assert_eq!(contexts, ["4096", "8192"]);
    assert_eq!(quants, ["AWQ", "FP16"]);
}

#[quench_test::test]
fn the_launch_estimate_follows_the_quant_override() {
    let m = model(
        "AWQ",
        vec![
            est("FP16", 4096, 14.0, 1.0),
            est("AWQ", 4096, 4.0, 1.0),
            est("AWQ", 8192, 4.0, 2.0),
            est("FP8", 4096, 7.0, 1.0),
        ],
    );
    // No override: the model's own quantisation.
    assert_eq!(find_launch_estimate(&m, "", None).unwrap().quant, "AWQ");
    assert_eq!(find_launch_estimate(&m, "fp8", None).unwrap().quant, "FP8");
    assert!(find_launch_estimate(&m, "gptq_marlin", None).is_none());
    // A length: the first at or above it, else the longest.
    assert_eq!(
        find_launch_estimate(&m, "", Some(5000)).unwrap().context.0,
        8192
    );
    assert_eq!(
        find_launch_estimate(&m, "", Some(1_000_000))
            .unwrap()
            .context
            .0,
        8192
    );
    // An override it does not know picks nothing out: the first row.
    assert_eq!(
        find_launch_estimate(&m, "other", None).unwrap().quant,
        "FP16"
    );
}

#[quench_test::test]
fn the_least_memory_share_rounds_up_by_twentieths() {
    assert!((round_gpu_util_up(0.31) - 0.35).abs() < 1e-9);
    let e = est("FP16", 4096, 14.0, 1.0);
    // (14 + 1.5) / (32 - 1) = 0.5 exactly.
    assert_eq!(minimum_gpu_util(&e, 32.0), Some(0.5));
    assert_eq!(minimum_gpu_util(&e, 0.0), None);
    assert_eq!(minimum_gpu_util(&e, 1.0), None);
    // Needs more than there is: capped at 1.0.
    assert_eq!(minimum_gpu_util(&e, 16.0), Some(1.0));
    // A tiny model is floored at 0.20.
    assert_eq!(
        minimum_gpu_util(&est("Q2K", 512, 0.1, 0.0), 100.0),
        Some(0.2)
    );
}

#[quench_test::test]
fn the_share_is_kept_unless_the_choice_changed() {
    let m = model("FP16", vec![est("FP16", 4096, 14.0, 1.0)]);
    let g = gpu(32.0, 30.0);
    assert!((launch_gpu_util(Some(&m), "", None, Some(0.7), false, &g) - 0.7).abs() < 1e-6);
    assert!((launch_gpu_util(Some(&m), "", None, Some(0.7), true, &g) - 0.5).abs() < 1e-6);
    assert!((launch_gpu_util(None, "", None, None, true, &g) - 0.9).abs() < 1e-6);
    assert!((launch_gpu_util(None, "", None, Some(0.4), true, &g) - 0.4).abs() < 1e-6);
    let bare = model("FP16", vec![]);
    assert!((launch_gpu_util(Some(&bare), "", None, None, true, &g) - 0.9).abs() < 1e-6);
}

#[quench_test::test]
fn the_launch_note_has_five_states() {
    let g = gpu(32.0, 30.0);
    let none = launch_fit(None, "", None, 0.9, &g);
    assert_eq!(none.key, "ui_vllm_fit_select_model");
    assert!(!none.launch_enabled);
    let bare = model("FP16", vec![]);
    assert_eq!(
        launch_fit(Some(&bare), "", None, 0.9, &g).key,
        "ui_vllm_fit_no_estimate"
    );

    let m = model("FP16", vec![est("FP16", 4096, 10.0, 2.0)]);
    let roomy = gpu(32.0, 32.0);
    let ok = launch_fit(Some(&m), "", None, 0.9, &roomy);
    assert_eq!(
        (ok.key, ok.tone, ok.launch_enabled),
        ("ui_vllm_fit_ok", FitTone::Ok, true)
    );
    let get = |k: &str| ok.args.iter().find(|(n, _)| *n == k).unwrap().1.clone();
    // 10 + 2 * 0.9 = 11.8 for the model; 32 * 0.9 = 28.8 is the budget and what vLLM reserves.
    assert_eq!(get("model"), "11.80");
    assert_eq!(get("budget"), "28.80");
    assert_eq!(get("required"), "28.80");
    assert_eq!(get("free"), "32.00");
    assert_eq!(get("remaining"), "3.20");

    let tight = launch_fit(Some(&m), "", None, 0.9, &g);
    assert_eq!(
        (tight.key, tight.tone, tight.launch_enabled),
        ("ui_vllm_fit_tight", FitTone::Warn, true)
    );
    let busy = launch_fit(Some(&m), "", None, 0.9, &gpu(32.0, 20.0));
    assert_eq!(
        (busy.key, busy.tone, busy.launch_enabled),
        ("ui_vllm_fit_wont_fit_free", FitTone::No, false)
    );
    let small = launch_fit(Some(&m), "", None, 0.3, &roomy);
    assert_eq!(
        (small.key, small.launch_enabled),
        ("ui_vllm_fit_wont_fit_budget", false)
    );
    assert!(launch_fit(Some(&m), "", None, 0.9, &roomy).launch_enabled);
    assert_eq!(
        switchboard_core::fit::LaunchFit::cpu().key,
        "ui_vllm_fit_note_cpu"
    );
}
