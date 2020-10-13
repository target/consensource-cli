use crate::error::CliError;
use crate::key;
use crate::submit;
use crate::transaction::{create_batch, create_batch_list_from_one, create_transaction};

use clap::ArgMatches;
use common::addressing;
use common::proto::organization::Organization_Authorization_Role;
use common::proto::payload::{AuthorizeAgentAction, CreateAgentAction};
use common::proto::payload::{CertificateRegistryPayload, CertificateRegistryPayload_Action};
use sawtooth_sdk::messages::batch::BatchList;
use sawtooth_sdk::signing;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time};

pub fn run<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        ("authorize", Some(args)) => run_authorize_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    let name = args.value_of("name").unwrap();
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let ms_since_epoch = since_the_epoch.as_secs();

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = create_agent_payload(name, ms_since_epoch);
    let header_input = create_agent_transaction_addresses(&signer.get_public_key()?.as_hex());
    let header_output = header_input.clone();
    let txn = create_transaction(&payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let public_key = context.get_public_key(&private_key)?.as_hex();
    agent_status_handler(&public_key, "create", url, &batch_list)
}

fn run_authorize_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    let agent_to_be_authorized = args.value_of("authorize_agent").unwrap(); // Pub key of agent we want to authorize
    let org_id = args.value_of("org_id").unwrap();
    let role = args.value_of("role").unwrap();
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let key = args.value_of("key"); // Priv key file of the agent doing the authorizing

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let public_key = context.get_public_key(&private_key)?.as_hex();
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = authorize_agent_payload(agent_to_be_authorized, role);
    let addresses_input =
        authorize_agent_transaction_addresses_input(&public_key, &org_id, &agent_to_be_authorized);
    let addresses_output = vec![
        addressing::make_organization_address(&org_id),
        addressing::make_agent_address(&agent_to_be_authorized),
    ];

    let txn = create_transaction(&payload, &signer, addresses_input, addresses_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    agent_status_handler(&public_key, "authorize", url, &batch_list)
}

fn agent_status_handler(
    public_key: &str,
    action: &str,
    url: &str,
    batch_list: &BatchList,
) -> Result<(), CliError> {
    let mut agent_status = submit::submit_batch_list(url, batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match agent_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Agent {} has been {}d", public_key, action);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    agent_status.data[0]
                        .invalid_transactions
                        .get(0)
                        .expect("Expected a transaction status, but was not found")
                        .message
                        .clone(),
                ));
            }
            // "PENDING" case where we should recheck
            _ => {
                thread::sleep(time::Duration::from_millis(3000));
                agent_status = submit::wait_for_status(&url, &agent_status.link)?;
            }
        }
    }
}

/// Returns a payload for creating an Agent
pub fn create_agent_payload(name: &str, timestamp: u64) -> CertificateRegistryPayload {
    let mut agent = CreateAgentAction::new();
    agent.set_name(String::from(name));
    agent.set_timestamp(timestamp);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::CREATE_AGENT;
    payload.set_create_agent(agent);
    payload
}

/// Returns a payload for to authorize an Agent
fn authorize_agent_payload(pub_key: &str, role: &str) -> CertificateRegistryPayload {
    let mut agent = AuthorizeAgentAction::new();
    agent.set_public_key(String::from(pub_key));
    match role {
        "1" => agent.set_role(Organization_Authorization_Role::ADMIN),
        "2" => agent.set_role(Organization_Authorization_Role::TRANSACTOR),
        x => Err(CliError::UserError(format!(
            "Unexpected invalid role {:?}",
            x
        )))
        .unwrap(),
    }

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::AUTHORIZE_AGENT;
    payload.set_authorize_agent(agent);
    payload
}

pub fn create_agent_transaction_addresses(public_key: &str) -> Vec<String> {
    let agent_address = addressing::make_agent_address(public_key);
    vec![agent_address]
}

fn authorize_agent_transaction_addresses_input(
    authorizer_public_key: &str,
    org_id: &str,
    authee_pub_key: &str,
) -> Vec<String> {
    let authorizer_agent_address = addressing::make_agent_address(authorizer_public_key);
    let org_address = addressing::make_organization_address(org_id);
    let authee_agent_address = addressing::make_agent_address(authee_pub_key);
    vec![authorizer_agent_address, org_address, authee_agent_address]
}
