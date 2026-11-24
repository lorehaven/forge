//! Gantry runs here on a cluster that does not exist (`GANTRY_CLUSTER=none`, `GANTRY_RUNNER=simulate`; see
//! `services::Fixture::start_gantry`), so the security suites can act on every route, the ones that delete, scale and exec
//! included, without a real workload anywhere in reach.
use crate::world::ForgeWorld;
use quench_test_bdd::given;

#[given("gantry API is available")]
const fn available(world: &mut ForgeWorld) {
    world.target = crate::world::Target::Gantry;
}
