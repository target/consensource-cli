// Copyright 2018 Cargill Incorporated
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Contains functions which assist with the creation of Identity Batches and
//! Transactions

use crate::error::CliError;

use common::addressing;
use common::proto::payload;
use crypto::digest::Digest;
use crypto::sha2::Sha512;
use protobuf::{Message, RepeatedField};
use sawtooth_sdk::messages::batch::{Batch, BatchHeader, BatchList};
use sawtooth_sdk::messages::transaction::{Transaction, TransactionHeader};
use sawtooth_sdk::signing::Signer;
use std::time::Instant;

/// Creates a nonce appropriate for a TransactionHeader
fn create_nonce() -> String {
    let elapsed = Instant::now().elapsed();
    format!("{}{}", elapsed.as_secs(), elapsed.subsec_nanos())
}

/// Returns a hex string representation of the supplied bytes
///
/// # Arguments
///
/// * `b` - input bytes
fn bytes_to_hex_str(b: &[u8]) -> String {
    b.iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join("")
}

/// Returns a Transaction for the given Payload and Signer
///
/// # Arguments
///
/// * `payload` - a fully populated identity payload
/// * `signer` - the signer to be used to sign the transaction
///
/// # Errors
///
/// If an error occurs during serialization of the provided payload or
/// internally created `TransactionHeader`, a `CliError::ProtobufError` is
/// returned.
///
/// If a signing error occurs, a `CliError::SigningError` is returned.
pub fn create_transaction(
    payload: &payload::CertificateRegistryPayload,
    signer: &Signer,
    inputs: Vec<String>,
    outputs: Vec<String>,
) -> Result<Transaction, CliError> {
    let mut txn = Transaction::new();
    let mut txn_header = TransactionHeader::new();

    txn_header.set_family_name(String::from(addressing::FAMILY_NAMESPACE));
    txn_header.set_family_version(String::from(addressing::FAMILY_VERSION));
    txn_header.set_nonce(create_nonce());
    txn_header.set_signer_public_key(signer.get_public_key()?.as_hex());
    txn_header.set_batcher_public_key(signer.get_public_key()?.as_hex());

    txn_header.set_inputs(RepeatedField::from_vec(inputs));
    txn_header.set_outputs(RepeatedField::from_vec(outputs));

    let payload_bytes = payload.write_to_bytes()?;
    let mut sha = Sha512::new();
    sha.input(&payload_bytes);
    let hash: &mut [u8] = &mut [0; 64];
    sha.result(hash);
    txn_header.set_payload_sha512(bytes_to_hex_str(hash));
    txn.set_payload(payload_bytes);

    let txn_header_bytes = txn_header.write_to_bytes()?;
    txn.set_header(txn_header_bytes.clone());

    let b: &[u8] = &txn_header_bytes;
    txn.set_header_signature(signer.sign(b)?);

    Ok(txn)
}

/// Returns a Batch for the given Transaction and Signer
///
/// # Arguments
///
/// * `txn` - a Transaction
/// * `signer` - the signer to be used to sign the transaction
///
/// # Errors
///
/// If an error occurs during serialization of the provided Transaction or
/// internally created `BatchHeader`, a `CliError::ProtobufError` is
/// returned.
///
/// If a signing error occurs, a `CliError::SigningError` is returned.
pub fn create_batch(txn: Transaction, signer: &Signer) -> Result<Batch, CliError> {
    let mut batch = Batch::new();
    let mut batch_header = BatchHeader::new();

    batch_header.set_transaction_ids(RepeatedField::from_vec(vec![txn.header_signature.clone()]));
    batch_header.set_signer_public_key(signer.get_public_key()?.as_hex());
    batch.set_transactions(RepeatedField::from_vec(vec![txn]));

    let batch_header_bytes = batch_header.write_to_bytes()?;
    batch.set_header(batch_header_bytes.clone());

    let b: &[u8] = &batch_header_bytes;
    batch.set_header_signature(signer.sign(b)?);

    Ok(batch)
}

/// Returns a vector of Batch structs for a given vector of Transaction structs and a Signer
///
/// # Arguments
///
/// * `txns` - a vector of Transaction structs
/// * `signer` - the signer to be used to sign the transaction
///
/// # Errors
///
/// If an error occurs during serialization of a provided Transaction or
/// internally created `BatchHeader`, a `CliError::ProtobufError` is
/// returned.
///
/// If a signing error occurs, a `CliError::SigningError` is returned.
pub fn create_batches(txns: Vec<Transaction>, signer: &Signer) -> Result<Vec<Batch>, CliError> {
    let mut batches: Vec<Batch> = vec![];

    for txn in txns {
        let batch = match create_batch(txn, signer) {
            Ok(b) => b,
            Err(e) => {
                return Err(CliError::InvalidTransactionError(format!(
                    "Error creating batch list: {}",
                    e
                )));
            }
        };
        batches.push(batch);
    }

    Ok(batches)
}

/// Returns a BatchList containing the provided vector Batch structs
///
/// # Arguments
///
/// * `batches` - a vector of Batch structs
pub fn create_batch_list(batches: Vec<Batch>) -> BatchList {
    let mut batch_list = BatchList::new();
    batch_list.set_batches(RepeatedField::from_vec(batches));
    batch_list
}

/// Returns a BatchList containing the provided Batch
///
/// # Arguments
///
/// * `batch` - a Batch
pub fn create_batch_list_from_one(batch: Batch) -> BatchList {
    let mut batch_list = BatchList::new();
    batch_list.set_batches(RepeatedField::from_vec(vec![batch]));
    batch_list
}

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;

    use crate::commands::agent;

    use common::proto::payload::{
        CertificateRegistryPayload, CertificateRegistryPayload_Action, CreateAgentAction,
    };
    use sawtooth_sdk::messages::batch::Batch;
    use sawtooth_sdk::messages::transaction::Transaction;
    use sawtooth_sdk::signing;
    use sawtooth_sdk::signing::{CryptoFactory, Signer};

    #[test]
    fn create_transaction_test() {
        // Create test signer
        let context =
            signing::create_context("secp256k1").expect("Failed to create secp256k1 context");
        let private_key = context
            .new_random_private_key()
            .expect("Failed to generate random private key");
        let factory = CryptoFactory::new(&*context);
        let signer = factory.new_signer(&*private_key);

        let test_txn = create_test_transaction(&signer);

        assert!(test_txn.is_ok());
    }

    #[test]
    fn create_batch_test() {
        // Create test signer
        let context =
            signing::create_context("secp256k1").expect("Failed to create secp256k1 context");
        let private_key = context
            .new_random_private_key()
            .expect("Failed to generate random private key");
        let factory = CryptoFactory::new(&*context);
        let signer = factory.new_signer(&*private_key);

        let test_txn = create_test_transaction(&signer).expect("Failed to create test transaction");

        let test_batch = create_test_batch(test_txn, &signer);

        assert!(test_batch.is_ok());
    }

    #[test]
    fn create_batches_test() {
        // Create test signer
        let context =
            signing::create_context("secp256k1").expect("Failed to create secp256k1 context");
        let private_key = context
            .new_random_private_key()
            .expect("Failed to generate random private key");
        let factory = CryptoFactory::new(&*context);
        let signer = factory.new_signer(&*private_key);

        let test_txns =
            create_test_transactions(&signer).expect("Failed to create test transactions");
        let batches = create_batches(test_txns, &signer);

        assert!(batches.is_ok());
    }

    #[test]
    fn create_batch_list_test() {
        // Create test signer
        let context =
            signing::create_context("secp256k1").expect("Failed to create secp256k1 context");
        let private_key = context
            .new_random_private_key()
            .expect("Failed to generate random private key");
        let factory = CryptoFactory::new(&*context);
        let signer = factory.new_signer(&*private_key);

        let test_txns =
            create_test_transactions(&signer).expect("Failed to create test transactions");
        let batches = create_batches(test_txns, &signer).expect("Failed to create batches");
        let batch_list = create_batch_list(batches.clone());

        assert!(batch_list.get_batches().len() > 1);

        assert_eq!(batch_list.get_batches().get(0), batches.get(0));
        assert_eq!(batch_list.get_batches().get(1), batches.get(1));
    }

    #[test]
    fn create_batch_list_from_one_test() {
        // Create test signer
        let context =
            signing::create_context("secp256k1").expect("Failed to create secp256k1 context");
        let private_key = context
            .new_random_private_key()
            .expect("Failed to generate random private key");
        let factory = CryptoFactory::new(&*context);
        let signer = factory.new_signer(&*private_key);

        let test_txn = create_test_transaction(&signer).expect("Failed to create test transaction");

        let test_batch = create_test_batch(test_txn, &signer).expect("Failed to create test batch");

        let batch_list = create_batch_list_from_one(test_batch.clone());

        assert!(batch_list.get_batches().len() > 0);

        assert_eq!(batch_list.get_batches().get(0), Some(&test_batch));
    }

    fn create_test_transaction(signer: &Signer) -> Result<Transaction, CliError> {
        // Create test payload
        let since_the_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards");
        let timestamp = since_the_epoch.as_secs();

        let mut agent = CreateAgentAction::new();
        agent.set_name(String::from("Bob"));
        agent.set_timestamp(timestamp);

        let mut payload = CertificateRegistryPayload::new();
        payload.action = CertificateRegistryPayload_Action::CREATE_AGENT;
        payload.set_create_agent(agent);

        // Create test inputs and outputs
        let payload = agent::create_agent_payload("test", timestamp);
        let pub_key = &signer
            .get_public_key()
            .expect("Failed to get signer's public key");
        let inputs = agent::create_agent_transaction_addresses(&pub_key.as_hex());
        let outputs = inputs.clone();

        let txn = create_transaction(&payload, &signer, inputs, outputs)
            .expect("Failed to create transaction");

        Ok(txn)
    }

    fn create_test_transactions(signer: &Signer) -> Result<Vec<Transaction>, CliError> {
        let (payload1, inputs1, outputs1) = create_test_payload(signer);
        let (payload2, inputs2, outputs2) = create_test_payload(signer);

        let txn1 = create_transaction(&payload1, &signer, inputs1, outputs1)
            .expect("Failed to create transaction");
        let txn2 = create_transaction(&payload2, &signer, inputs2, outputs2)
            .expect("Failed to create transaction");

        Ok(vec![txn1, txn2])
    }

    fn create_test_batch(txn: Transaction, signer: &Signer) -> Result<Batch, CliError> {
        let batch = create_batch(txn, signer).expect("Failed to create batch");

        Ok(batch)
    }

    fn create_test_payload(
        signer: &Signer,
    ) -> (CertificateRegistryPayload, Vec<String>, Vec<String>) {
        let since_the_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards");
        let timestamp = since_the_epoch.as_secs();

        let mut agent = CreateAgentAction::new();
        agent.set_name(String::from("Bob"));
        agent.set_timestamp(timestamp);

        let mut payload = CertificateRegistryPayload::new();
        payload.action = CertificateRegistryPayload_Action::CREATE_AGENT;
        payload.set_create_agent(agent);

        // Create test inputs and outputs
        let payload = agent::create_agent_payload("test", timestamp);
        let pub_key = &signer
            .get_public_key()
            .expect("Failed to get signer's public key");
        let inputs = agent::create_agent_transaction_addresses(&pub_key.as_hex());
        let outputs = inputs.clone();

        (payload, inputs, outputs)
    }
}
