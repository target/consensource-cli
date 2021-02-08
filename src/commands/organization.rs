use crate::error::CliError;
use crate::key;
use crate::submit;
use crate::transaction::{
    create_batch, create_batch_list, create_batch_list_from_one, create_batch_with_transactions,
    create_transaction,
};

use clap::ArgMatches;
use common::addressing;
use common::proto::payload::{CertificateRegistryPayload, CertificateRegistryPayload_Action};
use common::proto::payload::{CreateOrganizationAction, UpdateOrganizationAction};
use sawtooth_sdk::messages::transaction::Transaction;
use sawtooth_sdk::signing;
use std::fs::File;
use std::io::prelude::*;
use std::{thread, time};
use uuid::Uuid;

use common::proto::organization::Factory_Address;
use common::proto::organization::Organization_Contact;
use common::proto::organization::Organization_Type;

const SECP_256K1: &str = "secp256k1";

pub fn run(args: &ArgMatches) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        ("update", Some(args)) => run_update_command(args),
        ("batch_update", Some(args)) => run_batch_update_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command(args: &ArgMatches) -> Result<(), CliError> {
    let name = args.value_of("name").unwrap();
    let contact_name = args.value_of("contact_name").unwrap();
    let contact_phone_number = args.value_of("contact_phone_number").unwrap();
    let contact_language_code = args.value_of("contact_language_code").unwrap();
    let street = args.value_of("street_address");
    let city = args.value_of("city");
    let country = args.value_of("country");
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    let valid_org_types =
        "1 - CERTIFYING_BODY \n 2 - STANDARDS_BODY \n 3 - FACTORY \n 4 - INGESTION";

    let organization_type = match args.value_of("org_type").unwrap() {
        "1" => Ok(Organization_Type::CERTIFYING_BODY),
        "2" => Ok(Organization_Type::STANDARDS_BODY),
        "3" => Ok(Organization_Type::FACTORY),
        "4" => Ok(Organization_Type::INGESTION),
        other => Err(CliError::UserError(format!(
            "Invalid organization type: {:?}. Valid types are: \n {org_types}",
            other,
            org_types = valid_org_types
        ))),
    }?;

    if organization_type == Organization_Type::FACTORY {
        match street {
            None => Err(CliError::InvalidInputError(
                "A street address is required for a factory".to_string(),
            )),
            other => Ok(other),
        }?;
        match city {
            None => Err(CliError::InvalidInputError(
                "A city is required for a factory".to_string(),
            )),
            other => Ok(other),
        }?;
        match country {
            None => Err(CliError::InvalidInputError(
                "A country is required for a factory".to_string(),
            )),
            other => Ok(other),
        }?;
    }

    let org_id = Uuid::new_v4().to_string();

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = create_organization_payload(
        &org_id,
        &name,
        organization_type,
        contact_name,
        contact_phone_number,
        contact_language_code,
        street,
        city,
        country,
    );

    let header_input =
        create_organization_transaction_addresses(&signer.get_public_key()?.as_hex(), &org_id);
    let header_output = header_input.clone();

    let txn = create_transaction(&payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut org_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match org_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Organization {} has been created", org_id);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    org_status.data[0]
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
                org_status = submit::wait_for_status(url, &org_status.link)?;
            }
        }
    }
}

fn run_update_command(args: &ArgMatches) -> Result<(), CliError> {
    let org_id = args.value_of("id").unwrap();
    let name = args.value_of("name");
    let contact_name = args.value_of("contact_name");
    let contact_phone_number = args.value_of("contact_phone_number");
    let contact_language_code = args.value_of("contact_language_code");
    let street = args.value_of("street_address");
    let city = args.value_of("city");
    let country = args.value_of("country");
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context("secp256k1")?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let payload = update_organization_payload(
        &org_id,
        name,
        contact_name,
        contact_phone_number,
        contact_language_code,
        street,
        city,
        country,
    );

    let header_input =
        create_organization_transaction_addresses(&signer.get_public_key()?.as_hex(), &org_id);
    let header_output = header_input.clone();

    let txn = create_transaction(&payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut org_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match org_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Organization {} has been updated", org_id);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    org_status.data[0]
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
                org_status = submit::wait_for_status(url, &org_status.link)?;
            }
        }
    }
}

fn run_batch_update_command(args: &ArgMatches) -> Result<(), CliError> {
    // Extract system arguments
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    // Define uninitialized arguments
    let mut org_id: &str;
    let mut name: Option<&str>;
    let mut contact_name: Option<&str>;
    let mut contact_phone_number: Option<&str>;
    let mut contact_language_code: Option<&str>;
    let mut street: Option<&str>;
    let mut city: Option<&str>;
    let mut country: Option<&str>;

    // Read factories from provided JSON batch file
    let filepath = args.value_of("filepath").unwrap();
    let mut file = File::open(filepath)?;
    let mut data: String = String::new();
    file.read_to_string(&mut data)?;
    let org_updates: serde_json::Value = serde_json::from_str(&data).expect("Unable to parse");

    // Create signing key
    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context(SECP_256K1)?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    // Loop through map of factories and populate list of transactions
    println!("Creating transactions for {}", filepath);
    let mut txn_list: Vec<Transaction> = vec![];
    for (key, value) in org_updates.as_object().unwrap() {
        org_id = key.as_str();
        name = value.get("name").unwrap().as_str();
        contact_name = value.get("contact_name").unwrap().as_str();
        contact_phone_number = value.get("contact_phone_number").unwrap().as_str();
        contact_language_code = value.get("contact_language_code").unwrap().as_str();
        street = value.get("street_address").unwrap().as_str();
        city = value.get("city").unwrap().as_str();
        country = value.get("country").unwrap().as_str();

        let update_org_action_payload = update_organization_payload(
            org_id,
            name,
            contact_name,
            contact_phone_number,
            contact_language_code,
            street,
            city,
            country,
        );

        let header_input =
            create_organization_transaction_addresses(&signer.get_public_key()?.as_hex(), &org_id);
        let header_output = header_input.clone();
        let txn = create_transaction(
            &update_org_action_payload,
            &signer,
            header_input,
            header_output,
        )?;
        txn_list.push(txn);
    }

    println!("Creating batch list for transactions");
    let batch = create_batch_with_transactions(txn_list, &signer)?;
    let batch_list = create_batch_list(vec![batch]);

    let mut update_org_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match update_org_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => {
                println!("Organizations from file {} have been updated", filepath);
                break Ok(());
            }
            "INVALID" => {
                break Err(CliError::InvalidTransactionError(
                    update_org_status.data[0]
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
                update_org_status = submit::wait_for_status(url, &update_org_status.link)?;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_organization_payload(
    id: &str,
    name: &str,
    org_type: Organization_Type,
    contact_name: &str,
    contact_phone_number: &str,
    contact_language_code: &str,
    street: Option<&str>,
    city: Option<&str>,
    country: Option<&str>,
) -> CertificateRegistryPayload {
    let mut organization = CreateOrganizationAction::new();
    organization.set_name(String::from(name));
    organization.set_id(String::from(id));
    organization.set_organization_type(org_type);

    if org_type == Organization_Type::FACTORY {
        let mut factory_address = Factory_Address::new();
        factory_address.set_street_line_1(street.unwrap().to_string());
        factory_address.set_city(city.unwrap().to_string());
        factory_address.set_country(country.unwrap().to_string());
        organization.set_address(factory_address);
    }

    let mut contact = Organization_Contact::new();
    contact.set_name(String::from(contact_name));
    contact.set_phone_number(String::from(contact_phone_number));
    contact.set_language_code(String::from(contact_language_code));
    organization.set_contacts(protobuf::RepeatedField::from_vec(vec![contact]));

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::CREATE_ORGANIZATION;
    payload.set_create_organization(organization);
    payload
}

#[allow(clippy::too_many_arguments)]
pub fn update_organization_payload(
    id: &str,
    name: Option<&str>,
    contact_name: Option<&str>,
    contact_phone_number: Option<&str>,
    contact_language_code: Option<&str>,
    street: Option<&str>,
    city: Option<&str>,
    country: Option<&str>,
) -> CertificateRegistryPayload {
    let mut organization = UpdateOrganizationAction::new();
    organization.set_id(String::from(id));
    if let Some(name) = name {
        organization.set_name(String::from(name));
    }

    if let (Some(contact_name), Some(contact_phone_number), Some(contact_language_code)) =
        (contact_name, contact_phone_number, contact_language_code)
    {
        let mut contact = Organization_Contact::new();
        contact.set_name(String::from(contact_name));
        contact.set_phone_number(String::from(contact_phone_number));
        contact.set_language_code(String::from(contact_language_code));
        organization.set_contacts(protobuf::RepeatedField::from_vec(vec![contact]));
    };

    if let (Some(street), Some(city), Some(country)) = (street, city, country) {
        let mut factory_address = Factory_Address::new();
        factory_address.set_street_line_1(street.to_string());
        factory_address.set_city(city.to_string());
        factory_address.set_country(country.to_string());
        organization.set_address(factory_address);
    }

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::UPDATE_ORGANIZATION;
    payload.set_update_organization(organization);
    payload
}

pub fn create_organization_transaction_addresses(
    public_key: &str,
    organization_id: &str,
) -> Vec<String> {
    let agent_address = addressing::make_agent_address(public_key);
    let org_address = addressing::make_organization_address(organization_id);
    vec![agent_address, org_address]
}
