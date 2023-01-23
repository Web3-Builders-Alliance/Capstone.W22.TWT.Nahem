use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, Response, WasmMsg,
};
use cosmwasm_std::{Empty, Uint128};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw721::NumTokensResponse;
use cw721_base::Extension;
use cw721_base::MintMsg;

use crate::msg::ReceiveMsg;
use crate::{
    state::{Config, Subscriptions, CONFIG, SUBSCRIPTIONS},
    ContractError,
};

// NOTE: this was ExecuteMsg & QueryMsg before, might have to change it back
pub type Cw721ExecuteMsg = cw721_base::ExecuteMsg<Extension, Empty>;
pub type Cw721QueryMsg = cw721_base::QueryMsg<Empty>;

pub fn subscribe(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    subscriber: Addr,
    bounty_pct: u16,
    min_bounty: Option<u128>,
) -> Result<Response, ContractError> {
    // Protocol owner, DAO or auiting firm can subscribe to the contract
    if bounty_pct > 100 {
        return Err(ContractError::InvalidBountyPercentage {});
    }
    let subscriptions = Subscriptions {
        subscriber: subscriber.clone(),
        bounty_pct,
        min_bounty,
    };
    SUBSCRIPTIONS.save(deps.storage, subscriber, &subscriptions)?;

    Ok(Response::new().add_attribute("action", "subscribe"))
}

pub fn unsubscribe(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    SUBSCRIPTIONS.remove(deps.storage, info.sender);

    Ok(Response::new().add_attribute("action", "unsubscribe"))
}

pub fn handle_receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    // validate that cw20 contract is sending this message
    let hacker_addr = deps.api.addr_validate(&cw20_msg.sender)?;
    let cw20_addr = info.sender.clone();
    let msg: ReceiveMsg = from_binary(&cw20_msg.msg)?;

    match msg {
        ReceiveMsg::DepositCw20 { subscriber } => deposit_cw20(
            deps,
            env,
            subscriber,
            hacker_addr,
            cw20_addr,
            cw20_msg.amount,
        ),
    }
}

pub fn deposit_cw20(
    deps: DepsMut,
    _env: Env,
    subscriber: String,
    hacker_addr: Addr,
    cw20_addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let subscriber = deps.api.addr_validate(&subscriber)?;
    let subscriptions = SUBSCRIPTIONS.load(deps.storage, subscriber.clone())?;
    let bounty = subscriptions.bounty_pct as u128 * amount.u128() / 100;

    let mut messages = Vec::new();

    // transfer bounty to hacker as a message
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: hacker_addr.to_string(),
            amount: bounty.into(),
        })?,
        funds: vec![],
    }));

    // transfer remaining funds to suscriber as a message
    // TODO: check recipient address! &
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: subscriptions.subscriber.to_string(),
            amount: (amount.u128() - bounty).into(),
        })?,
        funds: vec![],
    }));

    let config = CONFIG.load(deps.storage)?;
    let num_tokens: NumTokensResponse = deps
        .querier
        .query_wasm_smart(config.cw721_contract_addr, &Cw721QueryMsg::NumTokens {})?;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cw20_addr.to_string(),
        msg: to_binary(&Cw721ExecuteMsg::Mint(MintMsg::<Extension> {
            token_id: (num_tokens.count + 1).to_string(),
            owner: hacker_addr.to_string(),
            token_uri: None,
            extension: None,
        }))?,
        funds: vec![],
    }));

    println!("### num_tokens: {:?}", num_tokens.count);

    Ok(Response::new()
        .add_attribute("action", "deposit_cw20")
        .add_messages(messages))
}

pub fn withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Option<u128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.contract_owner {
        return Err(ContractError::Unauthorized {});
    }

    let amount = amount.unwrap_or(0u128);

    Ok(Response::new()
        .add_attribute("action", "withdraw")
        .add_attribute("amount", amount.to_string()))
}

pub fn update_subscription(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_bounty_pct: Option<u16>,
    new_min_bounty: Option<u128>,
) -> Result<Response, ContractError> {
    let subscriptions = SUBSCRIPTIONS.load(deps.storage, info.sender.clone())?;

    if new_bounty_pct.is_none() && new_min_bounty == subscriptions.min_bounty {
        return Err(ContractError::NothingToUpdate {});
    }

    if new_bounty_pct > Some(100) {
        return Err(ContractError::InvalidBountyPercentage {});
    }

    let subscriptions = Subscriptions {
        subscriber: subscriptions.subscriber,
        bounty_pct: new_bounty_pct.unwrap_or(subscriptions.bounty_pct),
        min_bounty: new_min_bounty,
    };
    SUBSCRIPTIONS.save(deps.storage, info.sender, &subscriptions)?;

    Ok(Response::new().add_attribute("action", "update_subscription"))
}

pub fn update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    new_contract_owner: Option<String>,
    new_protocol_fee: Option<u16>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.contract_owner {
        return Err(ContractError::Unauthorized {});
    }

    if new_contract_owner.is_none() && new_protocol_fee.is_none() {
        return Err(ContractError::NothingToUpdate {});
    }

    if new_contract_owner == Some(config.contract_owner.to_string())
        && new_protocol_fee == Some(config.protocol_fee)
    {
        return Err(ContractError::NothingToUpdate {});
    }

    let val_new_contract_owner = deps
        .api
        .addr_validate(&new_contract_owner.unwrap_or(config.contract_owner.to_string()));

    let config = Config {
        contract_owner: val_new_contract_owner?,
        protocol_fee: new_protocol_fee.unwrap_or(config.protocol_fee),
        cw721_contract_addr: config.cw721_contract_addr,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}