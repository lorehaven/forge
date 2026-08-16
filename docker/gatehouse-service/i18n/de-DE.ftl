header_label = Gatehouse
ui_home_button = Startseite
ui_account_button = Mein Konto
ui_logout = Abmelden

ui_login_sign_in = Anmelden
ui_login_username = Benutzername
ui_login_password = Passwort
ui_login_submit = Anmelden
ui_login_invalid_credentials = Ungültige Anmeldedaten
ui_login_forgot_password = Passwort vergessen?
ui_login_register = Konto erstellen
ui_login_registered_ok = Konto erstellt. Bestätigen Sie Ihre E-Mail-Adresse und melden Sie sich dann an.
ui_login_verified_ok = Ihre E-Mail-Adresse wurde bestätigt. Melden Sie sich an, um fortzufahren.
ui_login_verify_invalid = Dieser Bestätigungslink ist ungültig oder abgelaufen.
ui_login_reset_requested_ok = Falls dieses Konto existiert, ist ein Link zum Zurücksetzen des Passworts unterwegs.
ui_login_reset_ok = Ihr Passwort wurde zurückgesetzt. Melden Sie sich mit dem neuen Passwort an.
ui_login_reset_invalid = Dieser Link zum Zurücksetzen ist ungültig oder abgelaufen.
ui_login_account_disabled = Dieses Konto wurde deaktiviert.
ui_login_account_locked = Zu viele fehlgeschlagene Versuche. Versuchen Sie es in ein paar Minuten erneut.
ui_login_mfa_title = Bestätigungscode
ui_login_mfa_code = Code
ui_login_mfa_hint = Geben Sie den 6-stelligen Code aus Ihrer Authenticator-App ein.
ui_login_mfa_submit = Bestätigen
ui_login_mfa_invalid = Der Code stimmt nicht überein. Versuchen Sie es erneut.

ui_register_title = Konto erstellen
ui_register_email = E-Mail
ui_register_submit = Konto erstellen
ui_register_have_account = Bereits ein Konto? Anmelden
ui_register_error_email_invalid = Geben Sie eine gültige E-Mail-Adresse ein.

ui_forgot_password_title = Passwort zurücksetzen
ui_forgot_password_hint = Falls eine E-Mail-Adresse hinterlegt ist, senden wir einen Link zum Zurücksetzen.
ui_forgot_password_submit = Link senden

ui_reset_title = Neues Passwort wählen
ui_reset_new_password = Neues Passwort
ui_reset_submit = Neues Passwort speichern
ui_reset_error_password_empty = Ein Passwort ist erforderlich.

ui_home_title = Dienste
ui_home_subtitle = Eine Anmeldung für alles Folgende.
ui_home_group_services = Verfügbare Dienste
ui_home_no_services = Derzeit sind keine Dienste aktiviert.

ui_service_conveyor_title = Conveyor
ui_service_conveyor_desc = Pipelines, Builds und Deployments.
ui_service_sage_title = Sage
ui_service_sage_desc = KI-Arbeitsbereich: Unterhaltungen, Projekte und Dateien.
ui_service_switchboard_title = Switchboard
ui_service_switchboard_desc = Modellorchestrierung und GPU-Instanzen.
ui_service_warehouse_title = Warehouse
ui_service_warehouse_desc = Registries für Crates, Images und Dateien.

ui_home_group_realm = Realm

ui_admin_title = Benutzer
ui_admin_users_title = Benutzer
ui_admin_users_desc = Konten, Rollen und wer worauf zugreifen darf.
ui_admin_no_users = Der Realm hat keine Benutzer.
ui_admin_you = Sie
ui_admin_edit = Bearbeiten
ui_admin_back = Zurück zu den Benutzern
ui_admin_grants_all = alle Dienste
ui_admin_grants_none = kein Zugriff

ui_admin_create_title = Benutzer hinzufügen
ui_admin_new_username = Benutzername
ui_admin_new_password = Passwort
ui_admin_new_hint = Ein neuer Benutzer hat zunächst keinen Zugriff. Diesen erteilen Sie im nächsten Schritt.
ui_admin_create = Erstellen

ui_admin_role = Rolle
ui_admin_role_user = Benutzer
ui_admin_role_admin = Administrator
ui_admin_role_service = Dienstkonto

ui_admin_permissions = Zugriff
ui_admin_wildcard_note = Diese Rolle gewährt bereits jeden Dienst; die Auswahl oben ist festgelegt.
ui_admin_new_password_optional = Neues Passwort (leer lassen, um das bisherige zu behalten)
ui_admin_save = Speichern

ui_admin_template_title = Vorlage anwenden
ui_admin_template = Vorlage
ui_admin_template_hint = Ersetzt jede Berechtigung unten durch die der Vorlage.
ui_admin_apply_template = Anwenden

ui_admin_delete_title = Löschen
ui_admin_delete_hint = Mit dem Konto enden auch dessen Sitzungen sofort.
ui_admin_delete = Diesen Benutzer löschen

ui_admin_status_title = Status
ui_admin_status_created = Erstellt
ui_admin_status_last_login = Letzte Anmeldung
ui_admin_status_never = Nie
ui_admin_status_disabled = Deaktiviert
ui_admin_status_locked = Gesperrt
ui_admin_status_mfa = Zwei-Faktor-Authentifizierung
ui_admin_status_yes = Ja
ui_admin_status_no = Nein
ui_admin_action_disable = Deaktivieren
ui_admin_action_enable = Aktivieren
ui_admin_action_unlock = Entsperren
ui_admin_action_mfa_disable = Erzwungen deaktivieren

ui_admin_ok_created = Benutzer erstellt.
ui_admin_ok_saved = Änderungen gespeichert.
ui_admin_ok_deleted = Benutzer gelöscht.

ui_admin_forbidden_title = Nicht erlaubt
ui_admin_forbidden = Die Benutzerverwaltung erfordert die Administratorrolle.

ui_admin_error_not_found = Diesen Benutzer gibt es nicht.
ui_admin_error_username_empty = Ein Benutzername ist erforderlich.
ui_admin_error_password_empty = Ein Passwort ist erforderlich.
ui_admin_error_exists = Dieser Benutzername ist bereits vergeben.
ui_admin_error_unknown_service = Dieser Dienst gehört nicht zu dieser Installation.
ui_admin_error_last_admin = Im Realm muss mindestens ein Administrator bleiben.
ui_admin_error_self_demote = Sie können sich die Administratorrolle nicht selbst entziehen.
ui_admin_error_self_delete = Sie können Ihr eigenes Konto nicht löschen.
ui_admin_error_self_disable = Sie können Ihr eigenes Konto nicht deaktivieren.
ui_admin_error_unknown_template = Diese Berechtigungsvorlage gibt es nicht.
ui_admin_error_roles_require_admin = Nur ein Administrator darf die Rolle „admin“ oder „service“ vergeben.
ui_admin_error_mfa_code_invalid = Der Code stimmt nicht überein - versuchen Sie es erneut.
ui_admin_error_internal = Die Änderung konnte nicht gespeichert werden.

ui_account_title = Mein Konto
ui_account_profile_title = Profil
ui_account_display_name = Anzeigename
ui_account_avatar_url = Avatar-URL
ui_account_title_field = Titel
ui_account_timezone = Zeitzone
ui_account_preferred_locale = Bevorzugte Sprache
ui_account_new_password = Neues Passwort
ui_account_password_hint = Leer lassen, um Ihr aktuelles Passwort zu behalten.
ui_account_save = Änderungen speichern
ui_account_ok_saved = Ihr Konto wurde aktualisiert.
ui_account_ok_mfa_enabled = Zwei-Faktor-Authentifizierung ist jetzt aktiviert.
ui_account_ok_mfa_disabled = Zwei-Faktor-Authentifizierung wurde deaktiviert.

ui_account_mfa_title = Zwei-Faktor-Authentifizierung
ui_account_mfa_enabled = Zwei-Faktor-Authentifizierung ist für Ihr Konto aktiviert.
ui_account_mfa_disabled = Zwei-Faktor-Authentifizierung ist nicht aktiviert.
ui_account_mfa_enable = Zwei-Faktor-Authentifizierung einrichten
ui_account_mfa_disable = Zwei-Faktor-Authentifizierung deaktivieren

ui_account_mfa_enroll_title = Zwei-Faktor-Authentifizierung einrichten
ui_account_mfa_enroll_hint = Scannen Sie dies mit Ihrer Authenticator-App oder geben Sie das Geheimnis unten manuell ein.
ui_account_mfa_secret = Geheimnis
ui_account_mfa_code = Code
ui_account_mfa_verify = Verifizieren und aktivieren
