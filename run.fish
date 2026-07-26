#!/usr/bin/env fish
#
# Local development environment for the Forge estate.
#
#   ./run.fish              start postgres, initialise the database, run every service
#   ./run.fish stop         stop the services (postgres keeps running)
#   ./run.fish stop all     stop the services and postgres
#   ./run.fish status       what is up, and on which port
#   ./run.fish logs sage    follow one service's log
#   ./run.fish db           postgres + foundry only, no services
#   ./run.fish reset        drop every schema foundry owns and reinstall (dev only)
#   ./run.fish test         run every BDD suite
#   ./run.fish test sage    run one suite (or pass cucumber flags: --tags, --name)
#
# Services are built with cargo and then launched from target/debug directly,
# rather than through `anvil run`. `anvil run` is the right thing when you are
# working on one service interactively; here the script needs a real PID per
# service so `stop` can actually stop it - killing a `cargo run` parent leaves
# the service running and holding its port.

# Absolute: services are launched with their own working directory, so every
# path this script hands them has to survive that change.
set -g repo_root (realpath (dirname (status --current-filename)))
cd $repo_root

set -g run_dir $repo_root/.run
set -g log_dir $run_dir/logs

# name | package | port | base path
set -g services \
    "gatehouse|gatehouse-service|5443|/gatehouse" \
    "warehouse|warehouse-service|6443|/warehouse" \
    "switchboard|switchboard-service|7443|/switchboard" \
    "sage|sage-service|8443|/sage"

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

function init_database
    say info foundry "applying the migration catalog"

    cargo build -q --package foundry-service
    or die "foundry-service build failed"

    # Run from the service directory so the catalog and install config resolve
    # exactly as they do inside the image.
    env DATABASE_URL=$database_url \
        $repo_root/target/debug/foundry-service \
        --catalog $repo_root/docker/foundry-service/migrations \
        --config $repo_root/docker/foundry-service/config/install.toml \
        apply
    or die "database initialisation failed"

    say ok foundry "database ready"
end

# ---------------------------------------------------------------------------
# Services
# ---------------------------------------------------------------------------

# gatehouse ships no dev certificate of its own; borrow warehouse's so the whole
# estate speaks HTTPS and the Secure realm cookie behaves the same everywhere.
function ensure_gatehouse_cert
    set -l dir $repo_root/docker/gatehouse-service
    if test -f $dir/cert.pem; and test -f $dir/key.pem
        return 0
    end
    if test -f $repo_root/docker/warehouse-service/cert.pem
        ln -sf $repo_root/docker/warehouse-service/cert.pem $dir/cert.pem
        ln -sf $repo_root/docker/warehouse-service/key.pem $dir/key.pem
        say info gatehouse "linked warehouse's dev certificate"
    else
        say warn gatehouse "no dev certificate; will serve plain HTTP"
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
            SERVICE_AUDIENCES=sage,switchboard,warehouse,gatehouse \
            AUTH_REDIRECT_HOSTS=https://localhost:6443,https://localhost:7443,https://localhost:8443 \
            SERVER_HTTP_REDIRECT_ADDR=0.0.0.0:5080 \
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

function stop_services
    stop_vllm_instances

    for entry in $services
        set -l parts (string split "|" $entry)
        set -l name $parts[1]
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
    mkdir -p $log_dir
    start_postgres
    start_redis
    init_database
    ensure_gatehouse_cert

    # gatehouse first: it owns the realm the others authenticate against. sage
    # last, because it checks switchboard at startup.
    for entry in $services
        set -l parts (string split "|" $entry)
        start_service $parts[1] $parts[2] $parts[3] $parts[4]
    end

    echo
    say ok ready "sign in at https://localhost:5443/gatehouse/ui/login"
    say info credentials "$admin_user / $admin_password (service account: $tech_user)"
    say info logs "$log_dir"
    echo
    for entry in $services
        set -l parts (string split "|" $entry)
        printf "  %-12s https://localhost:%s%s\n" $parts[1] $parts[3] $parts[4]
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
        die "usage: ./run.fish logs <gatehouse|warehouse|switchboard|sage>"
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

function cmd_stop -a scope
    stop_services
    warn_about_stray_vllm
    if test "$scope" = all
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
        cmd_start
    case stop
        cmd_stop $argv[2]
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
        echo "usage: ./run.fish [start|stop [all]|status|logs <service>|db|reset|test [service|flags]]"
        exit 1
end
