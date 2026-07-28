#!/usr/bin/env fish
#
# Local development environment for the Forge estate.
#
#   ./run.fish                  start postgres, initialise the database, run every service
#   ./run.fish start conveyor   start only conveyor and what it needs
#   ./run.fish repl             pick services interactively, then start them
#   ./run.fish stop             stop the services (postgres keeps running)
#   ./run.fish stop conveyor    stop one of them
#   ./run.fish stop all         stop the services and postgres
#   ./run.fish status           what is up, and on which port
#   ./run.fish logs sage        follow one service's log
#   ./run.fish db               postgres + foundry only, no services
#   ./run.fish reset            drop every schema foundry owns and reinstall (dev only)
#   ./run.fish test             run every BDD suite
#   ./run.fish test sage        run one suite (or pass cucumber flags: --tags, --name)
#
# Services are built with cargo and then launched from target/debug directly,
# rather than through `anvil run`. `anvil run` is the right thing when you are
# working on one service interactively; here the script needs a real PID per
# service so `stop` can actually stop it - killing a `cargo run` parent leaves
# the service running and holding its port.
#
# Naming services starts a subset of the estate: only those packages are built,
# and foundry installs only their schemas. Working on conveyor does not need
# sage's model launch or switchboard's GPU. What a service cannot start without
# comes with it - see the `needs` column below - so a subset is never a
# half-wired estate.

# Absolute: services are launched with their own working directory, so every
# path this script hands them has to survive that change.
set -g repo_root (realpath (dirname (status --current-filename)))
cd $repo_root

set -g run_dir $repo_root/.run
set -g log_dir $run_dir/logs

# name | package | port | base path | needs
#
# `needs` is what the service cannot start without, and it is also the start
# order: gatehouse owns the realm everything else authenticates against, and
# sage checks switchboard at startup. Selecting a service selects these too.
set -g services \
    "gatehouse|gatehouse-service|5443|/gatehouse|-" \
    "warehouse|warehouse-service|6443|/warehouse|gatehouse" \
    "switchboard|switchboard-service|7443|/switchboard|gatehouse" \
    "sage|sage-service|8443|/sage|gatehouse,switchboard" \
    "conveyor|conveyor-service|9443|/conveyor|gatehouse"

set -g pg_container postgres
set -g pg_image pgvector/pgvector:pg18
set -g redis_container forge-redis
set -g redis_image redis:7-alpine
set -g redis_url "redis://localhost:6379"
set -g database_url "postgres://postgres:postgres@localhost:5432/postgres"

# Values are read out of the existing service .env files rather than hardcoded,
# so the realm cannot quietly diverge from what the services already expect.
function env_value -a file key fallback
    set -l line (grep -h "^$key=" $repo_root/docker/$file/.env 2>/dev/null | head -1)
    if test -n "$line"
        echo $line | string replace -r "^$key=" '' | string trim -c '"'
    else
        echo $fallback
    end
end

# The realm shares one signing key; without it a token minted by gatehouse is
# rejected everywhere else.
function realm_secret
    env_value sage-service JWT_SECRET forge-local-dev-secret
end

# The interactive account.
set -g admin_user (env_value switchboard-service SERVICE_USERNAME admin)
set -g admin_password (env_value switchboard-service SERVICE_PASSWORD password)

# The machine-to-machine account. sage presents these to switchboard over Basic
# auth; switchboard used to create the account itself, but identity is realm-wide
# now, so gatehouse has to seed it or sage gets a 401.
set -g tech_user (env_value sage-service SWITCHBOARD_TECH_USERNAME service)
set -g tech_password (env_value sage-service SWITCHBOARD_TECH_PASSWORD password)

# ---------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------

function say -a tone label message
    switch $tone
        case ok
            set_color green
        case warn
            set_color yellow
        case err
            set_color red
        case '*'
            set_color cyan
    end
    printf "%-12s" $label
    set_color normal
    echo " $message"
end

function die -a message
    say err error $message
    exit 1
end

# ---------------------------------------------------------------------------
# Selecting services
# ---------------------------------------------------------------------------

function service_names
    for entry in $services
        string split -f1 "|" $entry
    end
end

function service_field -a name index
    for entry in $services
        set -l parts (string split "|" $entry)
        if test $parts[1] = $name
            echo $parts[$index]
            return 0
        end
    end
    return 1
end

# Everything downstream iterates the table rather than the argument list, so a
# selection is always in start order however it was typed or clicked.
function in_table_order
    for entry in $services
        set -l name (string split -f1 "|" $entry)
        contains -- $name $argv; and echo $name
    end
end

# A service is no use without the ones it authenticates against, so asking for
# sage and getting a sage that 401s on every request would be the wrong kind of
# obedience. Pulling the dependencies in is quiet: they are printed as part of
# the selection, so what actually started is never a surprise.
function with_dependencies
    set -l wanted
    set -l pending $argv

    while test (count $pending) -gt 0
        set -l name $pending[1]
        set -e pending[1]
        contains -- $name $wanted; and continue
        set -a wanted $name

        set -l needs (service_field $name 5)
        if test -n "$needs"; and test "$needs" != -
            set -a pending (string split "," $needs)
        end
    end

    in_table_order $wanted
end

# Validates whatever was on the command line and puts it in table order. No
# names means the whole estate, which is what `./run.fish` has always done.
function resolve_names
    if test (count $argv) -eq 0
        service_names
        return 0
    end

    set -l known (service_names)
    for name in $argv
        contains -- $name $known
        or die "unknown service '$name' (known: "(string join ', ' $known)")"
    end

    in_table_order $argv
end

# What to start: the names asked for, plus what they cannot start without.
function resolve_selection
    with_dependencies (resolve_names $argv)
end

# ---------------------------------------------------------------------------
# Postgres
# ---------------------------------------------------------------------------

# Sessions live in Redis: expiry is its TTL, revocation is a delete, and every
# service reads the same store so a logout is immediate everywhere.
function start_redis
    command -q docker; or die "docker not found on PATH"

    set -l existing (docker ps -q -f "name=^$redis_container\$")
    if test -n "$existing"
        say ok redis "already running"
        return 0
    end

    say info redis "starting $redis_image"
    docker run --rm -d --name $redis_container -p 6379:6379 $redis_image >/dev/null
    or die "failed to start redis"

    for i in (seq 30)
        if docker exec $redis_container redis-cli ping >/dev/null 2>&1
            say ok redis "ready on localhost:6379"
            return 0
        end
        sleep 0.5
    end
    die "redis did not become ready"
end

function start_postgres
    command -q docker; or die "docker not found on PATH"

    set -l existing (docker ps -q -f "name=^$pg_container\$")
    if test -n "$existing"
        say ok postgres "already running"
    else
        say info postgres "starting $pg_image"
        docker run --rm -d \
            --name $pg_container \
            -e POSTGRES_USER=postgres \
            -e POSTGRES_PASSWORD=postgres \
            -e POSTGRES_DB=postgres \
            -p 5432:5432 \
            $pg_image >/dev/null
        or die "failed to start postgres"
    end

    say info postgres "waiting for connections"
    for i in (seq 60)
        if docker exec $pg_container pg_isready -U postgres >/dev/null 2>&1
            say ok postgres "ready on localhost:5432"
            return 0
        end
        sleep 0.5
    end
    die "postgres did not become ready"
end

# ---------------------------------------------------------------------------
# Database initialisation
# ---------------------------------------------------------------------------

# Takes the services being started. Each service's catalog module is named
# after it, so a subset becomes `--install`; shared modules (auth, quench-core,
# pgvector) resolve from the catalog and do not need listing. With no names the
# install config is used as-is, which is the whole estate.
#
# Scoping matters more than it looks: starting a service later re-runs this with
# the wider selection, so the schemas arrive when the service that needs them
# does rather than all at once at the first boot.
function init_database
    set -l scope
    if test (count $argv) -gt 0; and test (count $argv) -lt (count (service_names))
        # One flag per module: `--install` is repeatable but takes a single spec,
        # and only FOUNDRY_INSTALL splits on commas.
        for name in $argv
            set -a scope --install $name
        end
        say info foundry "applying "(string join ', ' $argv)
    else
        say info foundry "applying the migration catalog"
    end

    cargo build -q --package foundry-service
    or die "foundry-service build failed"

    # Run from the service directory so the catalog and install config resolve
    # exactly as they do inside the image.
    env DATABASE_URL=$database_url \
        $repo_root/target/debug/foundry-service \
        --catalog $repo_root/docker/foundry-service/migrations \
        --config $repo_root/docker/foundry-service/config/install.toml \
        $scope \
        apply
    or die "database initialisation failed"

    say ok foundry "database ready"
end

# ---------------------------------------------------------------------------
# Services
# ---------------------------------------------------------------------------

# gatehouse and conveyor ship no dev certificate of their own; borrow
# warehouse's so the whole estate speaks HTTPS and the Secure realm cookie
# behaves the same everywhere.
function ensure_cert -a name
    set -l dir $repo_root/docker/$name-service
    if test -f $dir/cert.pem; and test -f $dir/key.pem
        return 0
    end
    if test -f $repo_root/docker/warehouse-service/cert.pem
        ln -sf $repo_root/docker/warehouse-service/cert.pem $dir/cert.pem
        ln -sf $repo_root/docker/warehouse-service/key.pem $dir/key.pem
        say info $name "linked warehouse's dev certificate"
    else
        say warn $name "no dev certificate; will serve plain HTTP"
    end
end

function start_service -a name package port base_path
    set -l secret (realm_secret)
    set -l pid_file $run_dir/$name.pid
    set -l log_file $log_dir/$name.log

    if is_running $name
        say ok $name "already running on $port"
        return 0
    end

    say info $name "building"
    cargo build -q --all-features --package $package
    or die "$package build failed"

    # Shared realm settings. These override the per-service .env files, which
    # dotenvy will not do the other way round - values already in the
    # environment win.
    set -l shared \
        DATABASE_URL=$database_url \
        JWT_SECRET=$secret \
        SERVICE_AUTH_ENABLED=true \
        AUTH_DB_SCHEMA=auth \
        SERVER_ADDR=0.0.0.0:$port \
        BASE_PATH=$base_path \
        SERVICE_NAME=$name \
        REDIS_URL=$redis_url \
        RUST_LOG=info

    set -l extra
    if test $name = gatehouse
        # Gatehouse owns the realm: it is the only service that seeds users and
        # the only one that mints tokens for the whole estate.
        set extra \
            AUTH_BOOTSTRAP=true \
            SERVICE_USERNAME=$admin_user \
            SERVICE_PASSWORD=$admin_password \
            SERVICE_TECH_USERNAME=$tech_user \
            SERVICE_TECH_PASSWORD=$tech_password \
            SERVICE_AUDIENCES=sage,switchboard,warehouse,gatehouse,conveyor \
            AUTH_REDIRECT_HOSTS=https://localhost:6443,https://localhost:7443,https://localhost:8443,https://localhost:9443 \
            SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:5080 \
            CONVEYOR_UI_URL=https://localhost:9443/conveyor/ui/home \
            SAGE_UI_URL=https://localhost:8443/sage/ui/home \
            SWITCHBOARD_UI_URL=https://localhost:7443/switchboard/ui/home \
            WAREHOUSE_UI_URL=https://localhost:6443/warehouse/ui/home
    else
        # Relying parties send a browser to gatehouse when there is no session.
        set extra GATEHOUSE_URL=https://localhost:5443/gatehouse
    end

    # sage asks switchboard to launch its SAGE_DEFAULT_MODELS at startup, which
    # is what puts the initializing screen in front of the home page. That is
    # the normal flow, so it stays on; FORGE_SKIP_MODELS=1 turns it off when you
    # want the estate up without spending the GPU.
    if test $name = sage; and set -q FORGE_SKIP_MODELS
        set extra $extra "SAGE_DEFAULT_MODELS=[]"
        say info sage "FORGE_SKIP_MODELS set - not launching default models"
    end

    # Run from the service's own directory: that is where its .env, TLS
    # certificate, i18n bundle and static assets live, and every one of those
    # paths is resolved relative to the working directory.
    env -C $repo_root/docker/$package $shared $extra \
        $repo_root/target/debug/$package >$log_file 2>&1 &
    set -l pid $last_pid
    echo $pid >$pid_file
    disown $pid 2>/dev/null

    for i in (seq 60)
        if not kill -0 $pid 2>/dev/null
            say err $name "exited during startup"
            tail -n 5 $log_file | sed 's/^/             /'
            rm -f $pid_file
            return 1
        end
        if curl -sk -o /dev/null --max-time 1 "https://localhost:$port$base_path/health"
            say ok $name "https://localhost:$port$base_path (pid $pid)"
            return 0
        end
        sleep 0.5
    end

    say warn $name "started (pid $pid) but /health did not answer - see $log_file"
    return 0
end

function is_running -a name
    set -l pid_file $run_dir/$name.pid
    test -f $pid_file; or return 1
    set -l pid (cat $pid_file)
    kill -0 $pid 2>/dev/null
end

# vLLM instances are switchboard's children, not ours: killing switchboard
# orphans them, and they keep the GPU and their ports. Ask switchboard to stop
# them first - best effort, since it may already be down.
function stop_vllm_instances
    is_running switchboard; or return 0

    set -l api "https://localhost:7443/switchboard/api/v1/vllm/instances"
    set -l ids (curl -sk --max-time 5 -u "$tech_user:$tech_password" $api 2>/dev/null \
        | grep -oP '"id"\s*:\s*"\K[^"]+')

    for id in $ids
        say info switchboard "stopping vLLM instance $id"
        curl -sk --max-time 30 -X DELETE -u "$tech_user:$tech_password" "$api/$id" >/dev/null 2>&1
    end

    # Give vLLM a moment to exit; it does not always go down with the request.
    if test -n "$ids"
        sleep 3
    end
end

# vLLM can outlive the stop request, and it holds both the GPU and its port.
# Report rather than kill: some of these may not be ours.
function warn_about_stray_vllm
    set -l strays (pgrep -a -f "vllm serve" 2>/dev/null | string split -f1 ' ')
    if test -n "$strays"
        say warn vllm "still running: $strays"
        say info vllm "these hold the GPU and port 8000+; kill them with: kill $strays"
    end
end

# Takes the services to stop, defaulting to all of them. Dependencies are not
# expanded here: stopping conveyor should not take gatehouse - and everything
# else authenticating against it - down with it.
function stop_services
    set -l selected $argv
    if test (count $selected) -eq 0
        set selected (service_names)
    end

    contains -- switchboard $selected; and stop_vllm_instances

    for name in $selected
        set -l pid_file $run_dir/$name.pid

        if is_running $name
            set -l pid (cat $pid_file)
            kill $pid 2>/dev/null
            for i in (seq 20)
                kill -0 $pid 2>/dev/null; or break
                sleep 0.2
            end
            kill -9 $pid 2>/dev/null
            say ok $name "stopped"
        else
            say info $name "not running"
        end
        rm -f $pid_file
    end
end

# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

function cmd_start
    set -l selected (resolve_selection $argv)

    if test (count $selected) -lt (count (service_names))
        say info selection (string join ', ' $selected)
    end

    mkdir -p $log_dir
    start_postgres
    start_redis
    init_database $selected

    # gatehouse and conveyor borrow warehouse's certificate; the others carry
    # their own.
    for name in gatehouse conveyor
        contains -- $name $selected; and ensure_cert $name
    end

    # The table's order is the dependency order, and `resolve_selection` keeps
    # it: gatehouse first because it owns the realm the others authenticate
    # against, sage after switchboard because it checks switchboard at startup.
    for name in $selected
        start_service $name (service_field $name 2) (service_field $name 3) (service_field $name 4)
    end

    echo
    if contains -- gatehouse $selected
        say ok ready "sign in at https://localhost:5443/gatehouse/ui/login"
        say info credentials "$admin_user / $admin_password (service account: $tech_user)"
    end
    say info logs "$log_dir"
    echo
    for name in $selected
        printf "  %-12s https://localhost:%s%s\n" $name (service_field $name 3) (service_field $name 4)
    end
    echo
    say info stop "./run.fish stop"
end

function cmd_status
    set -l existing (docker ps -q -f "name=^$pg_container\$" 2>/dev/null)
    if test -n "$existing"
        say ok postgres "running on localhost:5432"
    else
        say warn postgres "not running"
    end

    for entry in $services
        set -l parts (string split "|" $entry)
        set -l name $parts[1]
        if is_running $name
            say ok $name "https://localhost:$parts[3]$parts[4] (pid "(cat $run_dir/$name.pid)")"
        else
            say warn $name "not running"
        end
    end
end

function cmd_logs -a name
    if test -z "$name"
        die "usage: ./run.fish logs <gatehouse|warehouse|switchboard|sage|conveyor>"
    end
    set -l log_file $log_dir/$name.log
    test -f $log_file; or die "no log for '$name' at $log_file"
    tail -f $log_file
end

function cmd_db
    mkdir -p $log_dir
    start_postgres
    init_database
end

# Schema lifecycle belongs to foundry, so wiping the database for a fresh start
# is its `reset` command rather than a flag on each service.
function cmd_reset
    stop_services
    start_postgres

    say warn reset "dropping every schema foundry owns in $database_url"
    cargo build -q --package foundry-service
    or die "foundry-service build failed"

    env DATABASE_URL=$database_url \
        $repo_root/target/debug/foundry-service \
        --catalog $repo_root/docker/foundry-service/migrations \
        --config $repo_root/docker/foundry-service/config/install.toml \
        reset --yes
    or die "reset failed"

    say ok reset "database rebuilt; ./run.fish starts the estate"
end

# The BDD suite starts its own copies of the services on the same ports as the
# dev environment, so the two cannot be up at once. Rather than fail with a
# confusing bind error, take the environment down first and say so.
function cmd_test
    set -l running
    for entry in $services
        set -l parts (string split "|" $entry)
        if is_running $parts[1]
            set -a running $parts[1]
        end
    end

    if test -n "$running"
        say warn test "dev environment is up ($running); stopping it - the suite needs the same ports"
        stop_services
        warn_about_stray_vllm
        sleep 2
    end

    # A bare service name is the common case; anything starting with `-` is
    # passed through to cucumber (--tags, --name, --service, ...).
    set -l args $argv
    if test (count $argv) -ge 1; and not string match -q -- '-*' $argv[1]
        set args --service $argv[1] $argv[2..-1]
    end

    if test -n "$args"
        say info test "cargo run -p forge-bdd -- $args"
    else
        say info test "running every suite"
    end

    cargo run --package forge-bdd -- $args
    set -l result $status

    if test $result -eq 0
        say ok test "suites passed"
    else
        say err test "suites failed (exit $result)"
    end
    say info test "the dev environment is down; ./run.fish brings it back"

    return $result
end

# ---------------------------------------------------------------------------
# The picker
# ---------------------------------------------------------------------------
#
# For the case the flags are clumsy at: deciding what you need by looking at
# what is already up, then starting it. `./run.fish start conveyor` is the same
# thing in one line once you know what you want.
#
# Selection is kept global because fish functions return status, not lists, and
# threading it through every handler as an argument would be worse to read than
# one variable named for what it is.
set -g picked

function repl_list
    echo
    set -l index 0
    for entry in $services
        set index (math $index + 1)
        set -l parts (string split "|" $entry)
        set -l name $parts[1]

        if contains -- $name $picked
            set_color green
            printf "  [x] "
            set_color normal
        else
            printf "  [ ] "
        end

        printf "%d) %-12s " $index $name
        if is_running $name
            set_color green
            printf "running on %s" $parts[3]
        else
            set_color brblack
            printf "stopped"
        end
        set_color normal
        echo
    end
    echo
end

function repl_help
    echo "  <n> | <name>   toggle one (several at a time is fine: 1 5, sage conveyor)"
    echo "  all | none     select every service, or clear the selection"
    echo "  running        select whatever is up right now"
    echo "  up             start the selection, with what it depends on"
    echo "  down           stop the selection"
    echo "  status         what is up, and on which port"
    echo "  logs <name>    follow one service's log until you interrupt it"
    echo "  db | reset     initialise the database, or drop it and rebuild"
    echo "  list           the services again"
    echo "  quit           leave; anything started keeps running"
    echo
end

function repl_toggle -a token
    set -l name $token

    # A number is an index into the list as printed, which is the table order.
    if string match -qr '^[0-9]+$' -- $token
        set -l names (service_names)
        if test $token -lt 1 -o $token -gt (count $names)
            say warn picker "no service $token"
            return 1
        end
        set name $names[$token]
    end

    contains -- $name (service_names)
    or begin
        say warn picker "unknown service '$name'"
        return 1
    end

    if contains -- $name $picked
        set -g picked (string match -v -- $name $picked)
        say info picker "$name off"
    else
        set -g picked (in_table_order $picked $name)
        say ok picker "$name on"
    end
end

function cmd_repl
    # Seeded from the command line, so `./run.fish repl conveyor` opens with the
    # obvious thing already ticked.
    set -g picked (resolve_names $argv)
    if test (count $argv) -eq 0
        set -g picked
    end

    say info picker "pick services, then `up`. `help` for the rest, `quit` to leave."
    repl_list

    while true
        set -l prompt "forge> "
        if test (count $picked) -gt 0
            set prompt "forge ("(string join ',' $picked)")> "
        end

        read -l --prompt-str $prompt line
        or begin
            # Ctrl-D. Leaving without starting anything is a valid answer.
            echo
            break
        end

        set -l words (string split -n " " -- (string trim -- $line))
        test (count $words) -eq 0; and continue

        switch $words[1]
            case quit exit q
                break
            case help h '?'
                repl_help
            case list ls l
                repl_list
            case all
                set -g picked (service_names)
                repl_list
            case none clear
                set -g picked
                repl_list
            case running
                set -g picked
                for name in (service_names)
                    is_running $name; and set -a picked $name
                end
                repl_list
            case up start
                if test (count $picked) -eq 0
                    say warn picker "nothing selected - `all` for the whole estate"
                else
                    cmd_start $picked
                    # Dependencies may have joined the selection on the way in;
                    # showing them keeps `down` from being a surprise.
                    set -g picked (with_dependencies $picked)
                    repl_list
                end
            case down stop
                if test (count $picked) -eq 0
                    say warn picker "nothing selected"
                else
                    stop_services $picked
                    warn_about_stray_vllm
                    repl_list
                end
            case status
                cmd_status
                echo
            case logs
                if test (count $words) -lt 2
                    say warn picker "usage: logs <service>"
                else
                    # Interrupting `tail -f` is how you get back here, so the
                    # picker must not take SIGINT as a reason to exit.
                    cmd_logs $words[2]
                end
            case db
                cmd_db
            case reset
                cmd_reset
            case '*'
                for token in $words
                    repl_toggle $token
                end
                repl_list
        end
    end

    say info picker "left the picker; ./run.fish status shows what is up"
end

function cmd_stop
    # `all` reaches past the services to the containers under them; anything
    # else is a list of services, and no argument is every service.
    if test "$argv[1]" = all
        stop_services
    else
        stop_services (resolve_names $argv)
    end

    warn_about_stray_vllm

    if test "$argv[1]" = all
        docker stop $pg_container >/dev/null 2>&1
        and say ok postgres "stopped"
        or say info postgres "not running"
        docker stop $redis_container >/dev/null 2>&1
        and say ok redis "stopped"
        or say info redis "not running"
    end
end

switch "$argv[1]"
    case '' start
        cmd_start $argv[2..-1]
    case repl pick
        cmd_repl $argv[2..-1]
    case stop
        cmd_stop $argv[2..-1]
    case status
        cmd_status
    case logs
        cmd_logs $argv[2]
    case db
        cmd_db
    case reset
        cmd_reset
    case test
        cmd_test $argv[2..-1]
        exit $status
    case '*'
        # A bare service name is a start: `./run.fish conveyor` is what anyone
        # who has read the service list types first.
        if contains -- "$argv[1]" (service_names)
            cmd_start $argv
        else
            echo "usage: ./run.fish [start [service...]|repl|stop [all|service...]|status|logs <service>|db|reset|test [service|flags]]"
            echo "       services: "(string join ', ' (service_names))
            exit 1
        end
end
