use clap::ArgMatches;

use common::addressing;
use common::proto::payload::AccreditCertifyingBodyAction;

use common::proto::payload::{CertificateRegistryPayload, CertificateRegistryPayload_Action};
use error::CliError;
use transaction::{create_batch, create_batch_list_from_one, create_transaction};

use key;
use sawtooth_sdk::signing;
use submit;

use chrono::NaiveDateTime;
use std::{thread, time};

pub fn run<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    let certifying_body_id = args.value_of("certifying_body_id").unwrap();
    let standards_body_id = args.value_of("standards_body_id").unwrap();
    let standard_id = args.value_of("standard_id").unwrap();
    let valid_from = args.value_of("valid_from").unwrap();
    let valid_to = args.value_of("valid_to").unwrap();
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    //check valid_from is valid timestamp
    if NaiveDateTime::parse_from_str(valid_from, "%s").is_err() {
        return Err(CliError::UserError(
            "Valid from date is invalid. Please provide time in seconds since Unix epoch"
                .to_string(),
        ));
    }
    //check valid_to is valid timestamp
    if NaiveDateTime::parse_from_str(valid_to, "%s").is_err() {
        return Err(CliError::UserError(
            "Valid to date is invalid. Please provide time in seconds since Unix epoch".to_string(),
        ));
    }

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = create_accreditation_payload(
        standard_id,
        certifying_body_id,
        valid_from.parse::<u64>().unwrap(),
        valid_to.parse::<u64>().unwrap(),
    );

    let standard_address = addressing::make_standard_address(&standard_id);
    let agent_address = addressing::make_agent_address(&signer.get_public_key()?.as_hex());
    let certifying_body_address = addressing::make_organization_address(&certifying_body_id);
    let standards_body_address = addressing::make_organization_address(&standards_body_id);

    let txn = create_transaction(
        &payload,
        &signer,
        vec![
            standard_address.clone(),
            agent_address.clone(),
            certifying_body_address.clone(),
            standards_body_address.clone(),
        ],
        vec![certifying_body_address.clone()],
    )?;
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
            "COMMITTED" => break Ok(()),
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

fn create_accreditation_payload(
    standard_id: &str,
    certifying_body_id: &str,
    valid_from: u64,
    valid_to: u64,
) -> CertificateRegistryPayload {
    let mut accreditation = AccreditCertifyingBodyAction::new();
    accreditation.set_standard_id(String::from(standard_id));
    accreditation.set_certifying_body_id(String::from(certifying_body_id));
    accreditation.set_valid_from(valid_from);
    accreditation.set_valid_to(valid_to);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::ACCREDIT_CERTIFYING_BODY_ACTION;
    payload.set_accredit_certifying_body_action(accreditation);
    payload
}
