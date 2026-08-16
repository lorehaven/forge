header_label = Gatehouse
ui_home_button = Strona główna
ui_account_button = Moje konto
ui_logout = Wyloguj

ui_login_sign_in = Zaloguj się
ui_login_username = Nazwa użytkownika
ui_login_password = Hasło
ui_login_submit = Zaloguj
ui_login_invalid_credentials = Nieprawidłowe dane logowania
ui_login_forgot_password = Nie pamiętasz hasła?
ui_login_register = Utwórz konto
ui_login_registered_ok = Konto utworzone. Sprawdź e-mail, aby zweryfikować adres, a następnie się zaloguj.
ui_login_verified_ok = Twój adres e-mail został zweryfikowany. Zaloguj się, aby kontynuować.
ui_login_verify_invalid = Ten link weryfikacyjny jest nieprawidłowy lub wygasł.
ui_login_reset_requested_ok = Jeśli takie konto istnieje, link do resetowania hasła już wkrótce dotrze.
ui_login_reset_ok = Twoje hasło zostało zresetowane. Zaloguj się nowym hasłem.
ui_login_reset_invalid = Ten link do resetowania jest nieprawidłowy lub wygasł.
ui_login_account_disabled = To konto zostało dezaktywowane.
ui_login_account_locked = Zbyt wiele nieudanych prób. Spróbuj ponownie za kilka minut.
ui_login_mfa_title = Kod weryfikacyjny
ui_login_mfa_code = Kod
ui_login_mfa_hint = Wpisz 6-cyfrowy kod z aplikacji uwierzytelniającej.
ui_login_mfa_submit = Zweryfikuj
ui_login_mfa_invalid = Ten kod jest nieprawidłowy. Spróbuj ponownie.

ui_register_title = Utwórz konto
ui_register_email = E-mail
ui_register_submit = Utwórz konto
ui_register_have_account = Masz już konto? Zaloguj się
ui_register_error_email_invalid = Podaj prawidłowy adres e-mail.

ui_forgot_password_title = Zresetuj hasło
ui_forgot_password_hint = Wyślemy link do resetowania na adres e-mail przypisany do konta, jeśli taki istnieje.
ui_forgot_password_submit = Wyślij link resetujący

ui_reset_title = Wybierz nowe hasło
ui_reset_new_password = Nowe hasło
ui_reset_submit = Ustaw nowe hasło
ui_reset_error_password_empty = Hasło jest wymagane.

ui_home_title = Usługi
ui_home_subtitle = Jedno logowanie do wszystkiego poniżej.
ui_home_group_services = Dostępne usługi
ui_home_no_services = Żadne usługi nie są obecnie włączone.

ui_service_conveyor_title = Conveyor
ui_service_conveyor_desc = Potoki, kompilacje i wdrożenia.
ui_service_sage_title = Sage
ui_service_sage_desc = Środowisko AI: rozmowy, projekty i pliki.
ui_service_switchboard_title = Switchboard
ui_service_switchboard_desc = Orkiestracja modeli i instancji GPU.
ui_service_warehouse_title = Warehouse
ui_service_warehouse_desc = Rejestry crates, obrazów i plików.

ui_home_group_realm = Domena

ui_admin_title = Użytkownicy
ui_admin_users_title = Użytkownicy
ui_admin_users_desc = Konta, role i zakres dostępu każdego z nich.
ui_admin_no_users = Domena nie ma użytkowników.
ui_admin_you = ty
ui_admin_edit = Edytuj
ui_admin_back = Powrót do użytkowników
ui_admin_grants_all = wszystkie usługi
ui_admin_grants_none = brak dostępu

ui_admin_create_title = Dodaj użytkownika
ui_admin_new_username = Nazwa użytkownika
ui_admin_new_password = Hasło
ui_admin_new_hint = Nowy użytkownik nie ma żadnego dostępu. Nadasz go na następnym ekranie.
ui_admin_create = Utwórz

ui_admin_role = Rola
ui_admin_role_user = Użytkownik
ui_admin_role_admin = Administrator
ui_admin_role_service = Konto usługi

ui_admin_permissions = Dostęp
ui_admin_wildcard_note = Ta rola daje już dostęp do wszystkich usług; wybory powyżej są ustalone.
ui_admin_new_password_optional = Nowe hasło (pozostaw puste, aby zachować obecne)
ui_admin_save = Zapisz

ui_admin_template_title = Zastosuj szablon
ui_admin_template = Szablon
ui_admin_template_hint = Zastępuje każde uprawnienie poniżej tymi z szablonu.
ui_admin_apply_template = Zastosuj

ui_admin_delete_title = Usuń
ui_admin_delete_hint = Usunięcie konta natychmiast kończy jego sesje.
ui_admin_delete = Usuń tego użytkownika

ui_admin_status_title = Status
ui_admin_status_created = Utworzono
ui_admin_status_last_login = Ostatnie logowanie
ui_admin_status_never = Nigdy
ui_admin_status_disabled = Dezaktywowane
ui_admin_status_locked = Zablokowane
ui_admin_status_mfa = Uwierzytelnianie dwuskładnikowe
ui_admin_status_yes = Tak
ui_admin_status_no = Nie
ui_admin_action_disable = Dezaktywuj
ui_admin_action_enable = Aktywuj
ui_admin_action_unlock = Odblokuj
ui_admin_action_mfa_disable = Wymuś wyłączenie

ui_admin_ok_created = Użytkownik utworzony.
ui_admin_ok_saved = Zmiany zapisane.
ui_admin_ok_deleted = Użytkownik usunięty.

ui_admin_forbidden_title = Brak uprawnień
ui_admin_forbidden = Zarządzanie użytkownikami wymaga roli administratora.

ui_admin_error_not_found = Nie ma takiego użytkownika.
ui_admin_error_username_empty = Nazwa użytkownika jest wymagana.
ui_admin_error_password_empty = Hasło jest wymagane.
ui_admin_error_exists = Ta nazwa użytkownika jest już zajęta.
ui_admin_error_unknown_service = Ta usługa nie należy do tego wdrożenia.
ui_admin_error_last_admin = W domenie musi pozostać co najmniej jeden administrator.
ui_admin_error_self_demote = Nie możesz odebrać sobie roli administratora.
ui_admin_error_self_delete = Nie możesz usunąć własnego konta.
ui_admin_error_self_disable = Nie możesz dezaktywować własnego konta.
ui_admin_error_unknown_template = Nie ma takiego szablonu uprawnień.
ui_admin_error_roles_require_admin = Tylko administrator może przypisać rolę admin lub service.
ui_admin_error_mfa_code_invalid = Ten kod jest nieprawidłowy - spróbuj ponownie.
ui_admin_error_internal = Nie udało się zapisać zmiany.

ui_account_title = Moje konto
ui_account_profile_title = Profil
ui_account_display_name = Wyświetlana nazwa
ui_account_avatar_url = URL awatara
ui_account_title_field = Tytuł
ui_account_timezone = Strefa czasowa
ui_account_preferred_locale = Preferowany język
ui_account_new_password = Nowe hasło
ui_account_password_hint = Zostaw puste, aby zachować obecne hasło.
ui_account_save = Zapisz zmiany
ui_account_ok_saved = Twoje konto zostało zaktualizowane.
ui_account_ok_mfa_enabled = Uwierzytelnianie dwuskładnikowe jest teraz włączone.
ui_account_ok_mfa_disabled = Uwierzytelnianie dwuskładnikowe zostało wyłączone.

ui_account_mfa_title = Uwierzytelnianie dwuskładnikowe
ui_account_mfa_enabled = Uwierzytelnianie dwuskładnikowe jest włączone na Twoim koncie.
ui_account_mfa_disabled = Uwierzytelnianie dwuskładnikowe nie jest włączone.
ui_account_mfa_enable = Skonfiguruj uwierzytelnianie dwuskładnikowe
ui_account_mfa_disable = Wyłącz uwierzytelnianie dwuskładnikowe

ui_account_mfa_enroll_title = Skonfiguruj uwierzytelnianie dwuskładnikowe
ui_account_mfa_enroll_hint = Zeskanuj to aplikacją uwierzytelniającą lub wpisz poniższy sekret ręcznie.
ui_account_mfa_secret = Sekret
ui_account_mfa_code = Kod
ui_account_mfa_verify = Zweryfikuj i włącz
