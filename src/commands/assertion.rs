use clap::ArgMatches;

use common::addressing;
use common::proto::organization::{Factory_Address, Organization_Contact, Organization_Type};
use common::proto::payload::{
    AssertAction, AssertAction_FactoryAssertion, CertificateRegistryPayload,
    CertificateRegistryPayload_Action, CreateOrganizationAction, CreateStandardAction,
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
        ("factory", Some(args)) => match args.subcommand() {
            ("create", Some(args)) => run_factory_create_command(args),
            _ => Err(CliError::InvalidInputError(String::from(
                "Invalid subcommand. Pass --help for usage",
            ))),
        },
        ("certificate", Some(args)) => match args.subcommand() {
            ("create", Some(args)) => run_certificate_create_command(args),
            _ => Err(CliError::InvalidInputError(String::from(
                "Invalid subcommand. Pass --help for usage",
            ))),
        },
        ("standard", Some(args)) => match args.subcommand() {
            ("create", Some(args)) => run_standard_create_command(args),
            _ => Err(CliError::InvalidInputError(String::from(
                "Invalid subcommand. Pass --help for usage",
            ))),
        },
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_factory_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    // Extract arg values
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    let name = args.value_of("name").unwrap();
    let asserter_organization_id = args.value_of("asserter_organization_id").unwrap();
    let contact_name = args.value_of("contact_name").unwrap();
    let contact_phone_number = args.value_of("contact_phone_number").unwrap();
    let contact_language_code = args.value_of("contact_language_code").unwrap();
    let street = args.value_of("street_address");
    let city = args.value_of("city");
    let state_province = args.value_of("state_province");
    let country = args.value_of("country");
    let postal_code = args.value_of("postal_code");

    let factory_uuid = Uuid::new_v4().to_string();
    let factory_organization_id = args.value_of("factory_id").unwrap_or(&factory_uuid);

    // Generate new assertion ID
    let assertion_id = Uuid::new_v4().to_string();

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

    // Build create organization action payload
    let create_org_action_payload = build_create_organization_action_payload(
        &factory_organization_id,
        Organization_Type::FACTORY,
        name,
        contact_name,
        contact_phone_number,
        contact_language_code,
        street.unwrap(),
        city.unwrap(),
        state_province,
        country.unwrap(),
        postal_code,
    );

    let assertion_payload =
        create_factory_assertion_payload(&assertion_id, create_org_action_payload);

    submit_factory_assertion_transaction(
        assertion_payload,
        &assertion_id,
        &asserter_organization_id,
        &factory_organization_id,
        key,
        url,
    )
}

fn run_certificate_create_command<'a>(_args: &ArgMatches<'a>) -> Result<(), CliError> {
    Err(CliError::InvalidInputError(format!(
        "Certificate assertions are not yet supported"
    )))
}

fn run_standard_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    // Extract system arguments
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");

    // Extract required arguments
    let asserter_organization_id = args.value_of("asserter_organization_id").unwrap();
    let standard_name = args.value_of("name").unwrap();
    let version = args.value_of("version").unwrap();
    let description = args.value_of("description").unwrap();
    let link = args.value_of("link").unwrap();
    let approval_date = args.value_of("approval_date").unwrap();

    // Extract optional arguments
    // We use randomly generated uuid if no id was supplied
    let standard_uuid = Uuid::new_v4().to_string();
    let standard_id = args.value_of("id").unwrap_or(&standard_uuid);

    // Build create_standard_action payload
    let create_standard_action_payload = build_create_standard_action_payload(
        &standard_id,
        standard_name,
        version,
        description,
        link,
        approval_date.parse::<u64>().unwrap(),
    );

    // Generate an assertion ID for this assertion
    // Create assertion payload to be submitted
    let assertion_id = Uuid::new_v4().to_string();
    let assertion_payload =
        create_standard_assertion_payload(&assertion_id, create_standard_action_payload);

    // Submit assertion payload as a transaction
    submit_standard_assertion_transaction(
        assertion_payload,
        &assertion_id,
        &asserter_organization_id,
        &standard_id,
        key,
        url,
    )
}

fn build_create_organization_action_payload(
    id: &str,
    organization_type: Organization_Type,
    name: &str,
    contact_name: &str,
    contact_phone_number: &str,
    contact_language_code: &str,
    street: &str,
    city: &str,
    state_province: Option<&str>,
    country: &str,
    postal_code: Option<&str>,
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
        address.set_country(String::from(country));
        if let Some(state_province) = state_province {
            address.set_state_province(String::from(state_province));
        }
        if let Some(postal_code) = postal_code {
            address.set_postal_code(String::from(postal_code));
        }
        payload.set_address(address);
    }

    payload
}

fn build_create_standard_action_payload(
    standard_id: &str,
    standard_name: &str,
    version: &str,
    description: &str,
    link: &str,
    approval_date: u64,
) -> CreateStandardAction {
    let mut payload = CreateStandardAction::new();
    payload.set_standard_id(String::from(standard_id));
    payload.set_name(String::from(standard_name));
    payload.set_version(String::from(version));
    payload.set_description(String::from(description));
    payload.set_link(String::from(link));
    payload.set_approval_date(approval_date);

    payload
}

fn create_factory_assertion_payload(
    assertion_id: &str,
    create_organization_action_payload: CreateOrganizationAction,
) -> CertificateRegistryPayload {
    let mut assertion = AssertAction::new();
    assertion.set_assertion_id(String::from(assertion_id));

    let mut factory_assertion = AssertAction_FactoryAssertion::new();
    factory_assertion.set_factory(create_organization_action_payload);
    assertion.set_new_factory(factory_assertion);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::ASSERT_ACTION;
    payload.set_assert_action(assertion);
    payload
}

fn create_standard_assertion_payload(
    assertion_id: &str,
    create_standard_action_payload: CreateStandardAction,
) -> CertificateRegistryPayload {
    let mut assertion = AssertAction::new();
    assertion.set_assertion_id(String::from(assertion_id));
    assertion.set_new_standard(create_standard_action_payload);

    let mut payload = CertificateRegistryPayload::new();
    payload.action = CertificateRegistryPayload_Action::ASSERT_ACTION;
    payload.set_assert_action(assertion);
    payload
}

/// Creates a tuple of transaction header input/output addresses
///
/// Required inputs
/// - agent address
/// - agent's (the asserter) organization address
/// - factory organization address
/// - assertion address
///
/// Required outputs:
/// - factory organization address
/// - assertion address
fn create_factory_assertion_transaction_addresses(
    signer: &signing::Signer,
    assertion_id: &str,
    asserter_organization_id: &str,
    factory_organization_id: &str,
) -> Result<(Vec<String>, Vec<String>), CliError> {
    let agent_address = addressing::make_agent_address(&signer.get_public_key()?.as_hex());
    let asserter_organization_address =
        addressing::make_organization_address(asserter_organization_id);
    let factory_organization_address =
        addressing::make_organization_address(factory_organization_id);
    let assertion_address = addressing::make_assertion_address(assertion_id);
    Ok((
        vec![
            agent_address,
            asserter_organization_address,
            factory_organization_address.clone(),
            assertion_address.clone(),
        ],
        vec![
            factory_organization_address.clone(),
            assertion_address.clone(),
        ],
    ))
}

/// Creates a tuple of transaction header input/output addresses
/// for submitting a standard create assertion transaction.
///
/// Required inputs
/// - agent address
/// - agent's (the asserter) organization address
/// - standard id address
/// - assertion address
///
/// Required outputs:
/// - standard id address
/// - assertion address
fn create_standard_assertion_transaction_addresses(
    signer: &signing::Signer,
    assertion_id: &str,
    asserter_organization_id: &str,
    standard_id: &str,
) -> Result<(Vec<String>, Vec<String>), CliError> {
    let agent_address = addressing::make_agent_address(&signer.get_public_key()?.as_hex());
    let asserter_organization_address =
        addressing::make_organization_address(asserter_organization_id);
    let standard_id_address = addressing::make_standard_address(standard_id);
    let assertion_address = addressing::make_assertion_address(assertion_id);
    Ok((
        vec![
            agent_address,
            asserter_organization_address,
            standard_id_address.clone(),
            assertion_address.clone(),
        ],
        vec![standard_id_address.clone(), assertion_address.clone()],
    ))
}

fn submit_factory_assertion_transaction(
    assertion_payload: CertificateRegistryPayload,
    assertion_id: &str,
    asserter_organization_id: &str,
    factory_organization_id: &str,
    key: Option<&str>,
    url: &str,
) -> Result<(), CliError> {
    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context(SECP_256K1)?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let (header_input, header_output) = create_factory_assertion_transaction_addresses(
        &signer,
        assertion_id,
        &asserter_organization_id,
        factory_organization_id,
    )?;

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
            "COMMITTED" => {
                println!(
                    "Assertion {} has been created for factory {}",
                    assertion_id, factory_organization_id
                );
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
            // "UNKNOWN" case where we should recheck
            // "STATUS_UNSET" case where we should recheck
            _ => {
                thread::sleep(time::Duration::from_millis(3000));
                batch_status = submit::wait_for_status(url, &batch_status.link)?;
            }
        }
    }
}

fn submit_standard_assertion_transaction(
    assertion_payload: CertificateRegistryPayload,
    assertion_id: &str,
    asserter_organization_id: &str,
    standard_id: &str,
    key: Option<&str>,
    url: &str,
) -> Result<(), CliError> {
    let private_key = key::load_signing_key(key)?;
    let context = signing::create_context(SECP_256K1)?;
    let factory = signing::CryptoFactory::new(&*context);
    let signer = factory.new_signer(&private_key);

    let (header_input, header_output) = create_standard_assertion_transaction_addresses(
        &signer,
        assertion_id,
        &asserter_organization_id,
        standard_id,
    )?;

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
            "COMMITTED" => {
                println!(
                    "Assertion {} has been created for standard {}",
                    assertion_id, standard_id
                );
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
            // "UNKNOWN" case where we should recheck
            // "STATUS_UNSET" case where we should recheck
            _ => {
                thread::sleep(time::Duration::from_millis(3000));
                batch_status = submit::wait_for_status(url, &batch_status.link)?;
            }
        }
    }
}
