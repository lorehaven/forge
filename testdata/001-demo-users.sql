-- Demo accounts for local development, loaded by the `seed` task in
-- foreman.toml - never applied outside `foreman`.
--
-- Passwords are Argon2id hashes of "password123" in the exact format
-- quench-auth's `User::hash_password` produces, so they verify through the
-- realm's normal login form like any other account.
--
-- Six accounts, six different permission shapes to exercise:
--   alice  - editor across every service
--   bob    - viewer across every service
--   carol  - resource-scoped to just the "forge-demo" workbench project
--            (see 002-demo-workbench.sql, which creates that project under
--            this same fixed id)
--   dave   - delegated user-manager (create/edit/delete users), not admin
--   erin   - freshly "registered", no grants yet
--   frank  - disabled, cannot authenticate

INSERT INTO auth.users
    (username, password, roles, permissions, email, display_name, title, disabled_at)
VALUES
    ('alice',
     '$argon2id$v=19$m=4096,t=3,p=1$clA3UENxZk00MnVjQkNjNQ$slGJ+P9tQ5OGtS1Lu1DzkR69hq5vZmvZXxfxYKTIPA0',
     '["user"]',
     '{"sage":["read","write"],"warehouse":["read","write"],"switchboard":["read","launch","stop"],"conveyor":["read","write"],"workbench":["read","write"]}',
     'alice@forge.dev', 'Alice Chen', 'Engineering Lead', NULL),

    ('bob',
     '$argon2id$v=19$m=4096,t=3,p=1$UGRHd1RnNXAwWFBtQUhKNA$+hzRFby/x//hNXncHRxwOlw1MSHIkj1uohjqO7WVlvQ',
     '["user"]',
     '{"sage":["read"],"warehouse":["read"],"switchboard":["read"],"conveyor":["read"],"workbench":["read"]}',
     'bob@forge.dev', 'Bob Nakamura', 'QA Engineer', NULL),

    ('carol',
     '$argon2id$v=19$m=4096,t=3,p=1$cmhXYk5KOHV1UU5FWWozVA$bcFEaEu5V5gN0fePypI7cFhNRfUiz7ucAq+fon2dyeQ',
     '["user"]',
     '{"workbench":["project:forge-demo:read","project:forge-demo:write"]}',
     'carol@forge.dev', 'Carol Ibáñez', 'Product Designer', NULL),

    ('dave',
     '$argon2id$v=19$m=4096,t=3,p=1$dUtKM0Z0Y1RRdzZieERZbg$l9yJ6xDufLU6xrQhR55xizn4V5Hmv7bvc2szI0F0IDc',
     '["user"]',
     '{"gatehouse":["read-users","create-user","edit-user","delete-user","manage-permissions"]}',
     'dave@forge.dev', 'Dave Okafor', 'Platform Admin', NULL),

    ('erin',
     '$argon2id$v=19$m=4096,t=3,p=1$Qjg0SEFMdEFoUkFVVkRKMA$ZXtORC79FOMwRmxe1oy9bjD1mfS8E9FZG1dSaFawRoo',
     '["user"]',
     '{}',
     'erin@forge.dev', 'Erin Kowalski', 'New Hire', NULL),

    ('frank',
     '$argon2id$v=19$m=4096,t=3,p=1$QTdpQW5NSXpTamNNMlFVTA$eTdYWlftmkYVLe1cz2bCHR1BTEpa8hJ3H6AfJ95PjCw',
     '["user"]',
     '{"sage":["read"],"warehouse":["read"],"switchboard":["read"],"conveyor":["read"],"workbench":["read"]}',
     'frank@forge.dev', 'Frank Delgado', 'Former Contractor', NOW())
ON CONFLICT (username) DO NOTHING;
