# required
header_label = Conveyor
footer_label = © 2026 Paweł Walus — Order of Devs
locale_label = Idioma
theme_label = Tema

# ── Shell ────────────────────────────────────────────────────────────────────

ui_header_home = Conveyor
ui_home_button = Inicio
ui_logout = Cerrar sesión

# ── Home ─────────────────────────────────────────────────────────────────────

ui_home_title = Canalizaciones
ui_home_subtitle = Cada compilación, desde el commit que la pidió.

ui_repos_title = Repositorios
ui_repos_empty = Aún no hay repositorios registrados.

ui_runs_title = Ejecuciones recientes
ui_runs_empty = Todavía no se ha ejecutado nada.
ui_runs_view_all = Ver todas las canalizaciones

ui_project_subtitle = Repositorios y canalizaciones bajo este nodo.
ui_project_not_found = No existe ese proyecto.

# ── Pipelines ────────────────────────────────────────────────────────────────

ui_pipelines_title = Todas las canalizaciones
ui_pipelines_subtitle = Cada ejecución, la más reciente primero.
ui_pager_prev = Anterior
ui_pager_next = Siguiente
ui_pager_page = Página {$page} de {$total}

# ── Runs ─────────────────────────────────────────────────────────────────────

ui_col_status = Estado
ui_col_repository = Repositorio
ui_col_ref = Referencia
ui_col_commit = Commit
ui_col_trigger = Disparador
ui_col_when = Cuándo
ui_col_provider = Proveedor
ui_col_branch = Rama por defecto
ui_col_state = Estado
ui_col_checks = Comprobaciones
ui_col_actions = Acciones
ui_repo_enabled = Habilitado
ui_repo_disabled = Deshabilitado
ui_repo_run_now = Ejecutar ahora
ui_chip_not_run = Aún no ejecutado
ui_run_not_found = No existe esa ejecución.
ui_run_reason = Por qué terminó
ui_meta_trigger = Disparador
ui_meta_queued = En cola
ui_meta_duration = Duración
ui_meta_attempt = Intento
ui_artifacts_title = Artefactos
ui_log_loading = Cargando…

# ── Scan ─────────────────────────────────────────────────────────────────────

ui_scan_subtitle = Lint, dependencias sin usar y vulnerabilidades conocidas, de la ejecución más reciente.
ui_scan_lint_title = Lint
ui_scan_machete_title = Dependencias sin usar
ui_scan_audit_title = Vulnerabilidades
ui_scan_no_runs = Este repositorio aún no se ha ejecutado.
ui_scan_no_checks = La ejecución más reciente no ejecutó lint, machete ni audit. Añade `anvil lint`, `anvil machete` o `anvil audit` como pasos en .conveyor.toml para verlos aquí.
ui_scan_repo_not_found = No existe ese repositorio.
ui_scan_back = Volver al resumen
ui_scan_clean = No se encontró nada.

# ── Status ───────────────────────────────────────────────────────────────────

ui_status_queued = En cola
ui_status_running = En curso
ui_status_success = Correcta
ui_status_failed = Fallida
ui_status_cancelled = Cancelada
ui_status_skipped = Omitida
