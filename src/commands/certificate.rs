use crate::error::CliError;
use crate::key;
use crate::submit;
use crate::transaction::{create_batch, create_batch_list_from_one, create_transaction};

use clap::ArgMatches;
use common::addressing;
use common::proto::certificate::Certificate_CertificateData;
use common::proto::payload::{
    CertificateRegistryPayload, CertificateRegistryPayload_Action, IssueCertificateAction_Source,
};
use common::proto::payload::{IssueCertificateAction, UpdateCertificateAction};
use sawtooth_sdk::signing;
use std::{thread, time};

pub fn run(args: &ArgMatches) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        ("update", Some(args)) => run_update_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command(args: &ArgMatches) -> Result<(), CliError> {
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let cert_id = args.value_of("id").unwrap();
    let certifying_body_id = args.value_of("certifying_body_id").unwrap();
    let factory_id = args.value_of("factory_id").unwrap();
    let source = args.value_of("source").unwrap();
    let request_id = args.value_of("request_id");
    let standard_id = args.value_of("standard_id").unwrap();
    let valid_from = args.value_of("valid_from").unwrap();
    let valid_to = args.value_of("valid_to").unwrap();

    let cert_data: Result<Vec<Certificate_CertificateData>, CliError> = args
        .values_of("cert_data")
        .map(|values| {
            values
                .map(|cert_data| {
                    let cd: Vec<&str> = cert_data.split(':').collect();
                    match (cd.get(0), cd.get(1)) {
                        (Some(field), Some(data)) => {
                            let mut ccd: Certificate_CertificateData =
                                Certificate_CertificateData::new();
                            ccd.set_field(field.to_string());
                            ccd.set_data(data.to_string());
                            Ok(ccd)
                        }
                        _ => Err(CliError::InvalidInputError(String::from(
                            "Invalid format for cert_data",
                        ))),
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(vec![]));

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let public_key = context.get_public_key(&private_key)?.as_hex();
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = issue_certificate_payload(
        &cert_id,
        factory_id,
        source,
        request_id,
        standard_id,
        cert_data?,
        &valid_from,
        &valid_to,
    )?;

    let mut header_input =
        make_create_header_input(&public_key, &certifying_body_id, &cert_id, &factory_id);
    let mut header_output = vec![addressing::make_certificate_address(cert_id)];
    if payload.get_issue_certificate().get_source() == IssueCertificateAction_Source::FROM_REQUEST {
        let request_address = addressing::make_request_address(request_id.unwrap());
        header_input.push(request_address.clone());
        header_output.push(request_address);
    }
    let txn = create_transaction(&payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut batch_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(&url, &link))?;

    loop {
        match batch_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Certificate {} has been issued", cert_id);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    batch_status.data[0]
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
                batch_status = submit::wait_for_status(&url, &batch_status.link)?;
            }
        }
    }
}

fn run_update_command(args: &ArgMatches) -> Result<(), CliError> {
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let cert_id = args.value_of("id").unwrap();
    let certifying_body_id = args.value_of("certifying_body_id").unwrap();
    let valid_from = args.value_of("valid_from").unwrap();
    let valid_to = args.value_of("valid_to").unwrap();

    let cert_data: Result<Vec<Certificate_CertificateData>, CliError> = args
        .values_of("cert_data")
        .map(|values| {
            values
                .map(|cert_data| {
                    let cd: Vec<&str> = cert_data.split(':').collect();
                    match (cd.get(0), cd.get(1)) {
                        (Some(field), Some(data)) => {
                            let mut ccd: Certificate_CertificateData =
                                Certificate_CertificateData::new();
                            ccd.set_field(field.to_string());
                            ccd.set_data(data.to_string());
                            Ok(ccd)
                        }
                        _ => Err(CliError::InvalidInputError(String::from(
                            "Invalid format for cert_data",
                        ))),
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(vec![]));

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let public_key = context.get_public_key(&private_key)?.as_hex();
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = update_certificate_payload(&cert_id, cert_data?, &valid_from, &valid_to)?;

    let header_input = make_update_header_input(&public_key, &certifying_body_id, &cert_id);
    let header_output = vec![addressing::make_certificate_address(cert_id)];
    let txn = create_transaction(&payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut batch_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(&url, &link))?;

    loop {
        match batch_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Certificate {} has been updated", cert_id);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    batch_status.data[0]
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
                batch_status = submit::wait_for_status(&url, &batch_status.link)?;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn issue_certificate_payload(
    id: &str,
    factory_id: &str,
    source: &str,
    request_id: Option<&str>,
    standard_id: &str,
    cert_data: Vec<Certificate_CertificateData>,
    valid_from: &str,
    valid_to: &str,
) -> Result<CertificateRegistryPayload, CliError> {
    let mut certificate = IssueCertificateAction::new();
    certificate.set_id(id.to_string());
    let source_enum = match source {
        "1" => {
            if request_id.is_none() {
                return Err(CliError::InvalidInputError(
                    "request_id must be provided when source = 1 (FROM_REQUEST)".to_string(),
                ));
            }
            certificate.set_request_id(request_id.unwrap().to_string());
            Ok(IssueCertificateAction_Source::FROM_REQUEST)
        }
        "2" => {
            certificate.set_factory_id(factory_id.to_string());
            certificate.set_standard_id(standard_id.to_string());
            Ok(IssueCertificateAction_Source::INDEPENDENT)
        }
        _ => Err(CliError::InvalidInputError(
            "Invalid source. Pass 1 for FROM_REQUEST, and 2 for INDEPENDENT".to_string(),
        )),
    }?;
    certificate.set_source(source_enum);
    certificate.set_certificate_data(::protobuf::RepeatedField::from_vec(cert_data));
    certificate.set_valid_from(valid_from.parse().unwrap());
    certificate.set_valid_to(valid_to.parse().unwrap());

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::ISSUE_CERTIFICATE;
    payload.set_issue_certificate(certificate);
    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
fn update_certificate_payload(
    id: &str,
    cert_data: Vec<Certificate_CertificateData>,
    valid_from: &str,
    valid_to: &str,
) -> Result<CertificateRegistryPayload, CliError> {
    let mut certificate = UpdateCertificateAction::new();
    certificate.set_id(id.to_string());
    certificate.set_certificate_data(::protobuf::RepeatedField::from_vec(cert_data));
    certificate.set_valid_from(valid_from.parse().unwrap());
    certificate.set_valid_to(valid_to.parse().unwrap());

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::UPDATE_CERTIFICATE;
    payload.set_update_certificate(certificate);
    Ok(payload)
}

fn make_create_header_input(
    public_key: &str,
    certifying_body_id: &str,
    certificate_id: &str,
    factory_id: &str,
) -> Vec<String> {
    let agent_address = addressing::make_agent_address(public_key);
    let org_address = addressing::make_organization_address(certifying_body_id);
    let cert_address = addressing::make_certificate_address(certificate_id);
    let factory_address = addressing::make_organization_address(factory_id);
    vec![agent_address, org_address, cert_address, factory_address]
}

fn make_update_header_input(
    public_key: &str,
    certifying_body_id: &str,
    certificate_id: &str,
) -> Vec<String> {
    let agent_address = addressing::make_agent_address(public_key);
    let org_address = addressing::make_organization_address(certifying_body_id);
    let cert_address = addressing::make_certificate_address(certificate_id);
    vec![agent_address, org_address, cert_address]
}
