use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

#[gw2_endpoint(path="skills", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
// TODO: stub with only `id` (not even pub). The full shape (facts, traited_facts, slots,
//       chains, …) is in the 2026-09-27 wiki audit in Firefly.
pub struct Skill {
    id: SkillId,
}
