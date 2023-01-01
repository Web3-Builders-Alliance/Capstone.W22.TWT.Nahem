use cosmwasm_schema::{cw_serde, QueryResponses};

use crate::state::Action;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    Boilerplate {},
    Subscribe { commission: String, action: Action },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(BoilerplateResponse)]
    Boilerplate {},
}

#[cw_serde]
pub struct BoilerplateResponse {}

#[cw_serde]
pub enum MigrateMsg {}
