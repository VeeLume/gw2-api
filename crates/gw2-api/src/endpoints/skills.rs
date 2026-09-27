use gw2_api_macros::gw2_endpoint;
use serde::{Deserialize, Serialize};

#[gw2_endpoint(path="skills", id_type = u32, paged)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    id: SkillId,
}
