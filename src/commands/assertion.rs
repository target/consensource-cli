use clap::ArgMatches;

use common::addressing;
use common::proto::organization::{Factory_Address, Organization_Contact, Organization_Type};
use common::proto::payload::{
    AssertAction, AssertAction_FactoryAssertion, CertificateRegistryPayload,
    CertificateRegistryPayload_Action, CreateOrganizationAction,
};

use error::CliError;

use key;
use sawtooth_sdk::signing;
use std::{thread, time};
use submit;
use transaction::{create_batch, create_batch_list_from_one, create_transaction};
use uuid::Uuid;

const SECP_256K1: &str = "secp256k1";

pub fn run<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    // Extract arg values
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let name = args.value_of("name").unwrap();
    let contact_name = args.value_of("contact_name").unwrap();
    let contact_phone_number = args.value_of("contact_phone_number").unwrap();
    let contact_language_code = args.value_of("contact_language_code").unwrap();
    let street = args.value_of("street_address");
    let city = args.value_of("city");
    let country = args.value_of("country");

    // Generate new assertion ID
    let assertion_id = Uuid::new_v4().to_string();

    // Check org assertion type
    let assertion_payload =
        match args.value_of("assertion_type").unwrap() {
            "1" => {
                // Validate factory-specifc args
                match street {
                    None => Err(CliError::InvalidInputError(format!(
                        "A street address is required for a factory"
                    ))),
                    val => Ok(val),
                }?;
                match city {
                    None => Err(CliError::InvalidInputError(format!(
                        "A city is required for a factory"
                    ))),
                    val => Ok(val),
                }?;
                match country {
                    None => Err(CliError::InvalidInputError(format!(
                        "A country is required for a factory"
                    ))),
                    val => Ok(val),
                }?;
                let create_org_payload = create_organization_payload(
                    &(Uuid::new_v4().to_string()),
                    Organization_Type::FACTORY,
                    name,
                    contact_name,
                    contact_phone_number,
                    contact_language_code,
                    street.unwrap(),
                    city.unwrap(),
                    country.unwrap(),
                );
                // Create the assertion payload with the org data
                create_factory_assertion_payload(&assertion_id, create_org_payload)
            }
            "2" => {
                return Err(CliError::InvalidInputError(String::from(
                    "Certificate assertions are not yet supported",
                )))
            }
            "3" => {
                return Err(CliError::InvalidInputError(String::from(
                    "Standards assertions are not yet supported",
                )))
            }
            _ => return Err(CliError::InvalidInputError(String::from(
                "Invalid assertion type. Only FACTORY (1) assertions are supported at this time",
            ))),
        };

    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context(SECP_256K1)?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let header_input =
        create_assertion_transaction_addresses(&signer.get_public_key()?.as_hex(), &assertion_id);
    let header_output = header_input.clone();

    let txn = create_transaction(&assertion_payload, &signer, header_input, header_output)?;
    let batch = create_batch(txn, &signer)?;
    let batch_list = create_batch_list_from_one(batch);

    let mut batch_status = submit::submit_batch_list(url, &batch_list)
        .and_then(|link| submit::wait_for_status(url, &link))?;

    loop {
        match batch_status
            .data
            .get(0)
            .expect("Expected a batch status, but was not found")
            .status
            .as_ref()
        {
            "COMMITTED" => break Ok(()),
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
            // "UNKNOWN" case where we should recheck
            // "STATUS_UNSET" case where we should recheck
            _ => {
                thread::sleep(time::Duration::from_millis(3000));
                batch_status = submit::wait_for_status(url, &batch_status.link)?;
            }
        }
    }
}

fn create_organization_payload(
    id: &str,
    organization_type: Organization_Type,
    name: &str,
    contact_name: &str,
    contact_phone_number: &str,
    contact_language_code: &str,
    street: &str,
    city: &str,
    country: &str,
) -> CreateOrganizationAction {
    let mut payload = CreateOrganizationAction::new();
    payload.set_id(String::from(id));
    payload.set_organization_type(organization_type);
    payload.set_name(String::from(name));

    let mut new_contact = Organization_Contact::new();
    new_contact.set_name(String::from(contact_name));
    new_contact.set_phone_number(String::from(contact_phone_number));
    new_contact.set_language_code(String::from(contact_language_code));
    payload.set_contacts(protobuf::RepeatedField::from_vec(vec![new_contact]));

    if organization_type == Organization_Type::FACTORY {
        let mut address = Factory_Address::new();
        address.set_street_line_1(String::from(street));
        address.set_city(String::from(city));
        address.set_state_province("test".to_string());
        address.set_country(String::from(country));
        address.set_postal_code("test".to_string());
        payload.set_address(address);
    }

    payload
}

fn create_factory_assertion_payload(
    assertion_id: &str,
    create_organization_payload: CreateOrganizationAction,
) -> CertificateRegistryPayload {
    let mut assertion = AssertAction::new();
    assertion.set_assertion_id(String::from(assertion_id));

    let mut factory_assertion = AssertAction_FactoryAssertion::new();
    factory_assertion.set_factory(create_organization_payload);
    assertion.set_new_factory(factory_assertion);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::ASSERT_ACTION;
    payload.set_assert_action(assertion);
    payload
}

fn create_assertion_transaction_addresses(public_key: &str, assertion_id: &str) -> Vec<String> {
    let agent_address = addressing::make_agent_address(public_key);
    let assertion_address = addressing::make_assertion_address(assertion_id);
    vec![agent_address, assertion_address]
}
