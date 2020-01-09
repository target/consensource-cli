use clap::ArgMatches;
use commands::agent;
use reqwest;
use rpassword;

use error::CliError;
use key;
use sawtooth_sdk::signing::PublicKey;
use std::collections::HashMap;

pub fn run<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    match args.subcommand() {
        ("create", Some(args)) => run_create_command(args),
        _ => Err(CliError::InvalidInputError(String::from(
            "Invalid subcommand. Pass --help for usage",
        ))),
    }
}

fn run_create_command<'a>(args: &ArgMatches<'a>) -> Result<(), CliError> {
    let name = args.value_of("name").unwrap();
    let key = args.value_of("key");
    let url = args.value_of("url").unwrap_or("http://localhost:9009");
    //prompt user to enter password, this should keep the password out of logs
    let pw = rpassword::prompt_password_stdout("Password: ")?;
    //pass ArgMatches to create the agent associated with the user
    let _agent_create_result = agent::run(args);

    let public_key = key::load_public_key(key)?;
    let private_key = key::load_signing_key(key)?;
    //hopefully this works the same as sjcl.encrypt
    let encrypted_private_key = private_key.to_pem_with_password(&pw)?;

    //construct our HashMap to turn into JSON. See UserCreate in api/authorizations.rs
    let mut map = HashMap::new();
    map.insert("public_key", public_key.as_hex());
    map.insert("batch_id", "".to_string()); //not needed
    map.insert("transaction_id", "".to_string()); //not needed
    map.insert("encrypted_private_key", encrypted_private_key);
    map.insert("username", name.to_string());
    map.insert("password", pw);

    let client = reqwest::Client::new();
    let post_url = String::from(url) + "/api/users";
    let _res = client
        .post(&post_url)
        .json(&map)
        .send()
        .map_err(CliError::from);
    Ok(())
}

impl From<reqwest::Error> for CliError {
    fn from(err: reqwest::Error) -> Self {
        CliError::InvalidInputError(format!("Unable to post to api: {}", err))
    }
}
