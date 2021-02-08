use crate::error::CliError;
use crate::key;
use crate::submit;
use crate::transaction::{create_batch, create_batch_list_from_one, create_transaction};

use chrono::NaiveDateTime;
use clap::ArgMatches;
use common::addressing;
use common::proto::payload::CreateStandardAction;
use common::proto::payload::{CertificateRegistryPayload, CertificateRegistryPayload_Action};
use crypto::digest::Digest;
use crypto::sha2::Sha256;
use sawtooth_sdk::signing;
use std::{thread, time};

pub fn run(args: &ArgMatches) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command(args: &ArgMatches) -> Result<(), CliError> {
    let name = args.value_of("name").unwrap();
    let version = args.value_of("version").unwrap();
    let description = args.value_of("description").unwrap();
    let link = args.value_of("link").unwrap();
    let organization_id = args.value_of("organization_id").unwrap();
    let approval_date = args.value_of("approval_date").unwrap();
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    //check approval_date is valid timestamp
    if NaiveDateTime::parse_from_str(approval_date, "%s").is_err() {
        return Err(CliError::UserError(
            "Approval date is invalid. Please provide time in seconds since Unix epoch".to_string(),
        ));
    }

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = create_standard_payload(
        &name,
        &version,
        &description,
        &link,
        approval_date.parse::<u64>().unwrap(),
    );

    let (inputs, outputs) = create_standard_transaction_addresses(
        &signer,
        payload.get_create_standard().get_standard_id(),
        &organization_id,
    )?;

    let txn = create_transaction(&payload, &signer, inputs, outputs)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Standard {} {} has been created", name, version);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    status.data[0]
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
                status = submit::wait_for_status(url, &status.link)?;
            }
        }
    }
}

pub fn create_standard_payload(
    name: &str,
    version: &str,
    description: &str,
    link: &str,
    approval_date: u64,
) -> CertificateRegistryPayload {
    let mut standard = CreateStandardAction::new();

    let mut standard_id_sha = Sha256::new();
    standard_id_sha.input_str(name);
    standard.set_standard_id(standard_id_sha.result_str());
    standard.set_name(String::from(name));
    standard.set_version(String::from(version));
    standard.set_description(String::from(description));
    standard.set_link(String::from(link));
    standard.set_approval_date(approval_date);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::CREATE_STANDARD;
    payload.set_create_standard(standard);
    payload
}

pub fn create_standard_transaction_addresses(
    signer: &signing::Signer,
    standard_id: &str,
    organization_id: &str,
) -> Result<(Vec<String>, Vec<String>), CliError> {
    let standard_address = addressing::make_standard_address(standard_id);
    let agent_address = addressing::make_agent_address(&signer.get_public_key()?.as_hex());
    let organization_address = addressing::make_organization_address(&organization_id);
    Ok((
        vec![
            standard_address.clone(),
            agent_address,
            organization_address,
        ],
        vec![standard_address],
    ))
}
